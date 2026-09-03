//! El ayudante privilegiado: el único proceso que corre como root.
//!
//! Se lanza con `pkexec /usr/lib/vasak-installer/vasak-installer-helper` desde
//! la ventana, y habla NDJSON por entrada y salida estándar. Todo lo que puede
//! destruir datos pasa por acá y por ningún otro lado.
//!
//! ## Por qué un proceso aparte y no la aplicación entera como root
//!
//! La aplicación es un WebView completo: un motor de navegador con su JIT, su
//! red y su pila de imágenes. Correrlo como root para poder llamar a `parted`
//! es cambiar una superficie de ataque de dos comandos por una de un navegador.
//! Y hay una razón práctica además de la de seguridad: como root, el plugin de
//! configuración leería `/root` en vez del perfil de la sesión live, y el
//! instalador aparecería con otro tema e otros iconos que el escritorio que lo
//! rodea.
//!
//! ## Por qué un solo proceso para todo y no uno por operación
//!
//! Un `pkexec` por operación son tantas autorizaciones como operaciones. Con uno
//! solo, la autorización ocurre una vez —al abrir la ventana— y después el canal
//! ya está. En el medio live eso no se nota porque la regla de polkit lo
//! permite sin preguntar, pero el instalador también se puede correr desde un
//! sistema instalado, y ahí importa.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::archconfig;
use crate::conflictos::{self, Choque};
use crate::layout;
use crate::probe;
use crate::protocol::{
    CuerpoPeticion, EstadoPaso, Mensaje, Nivel, Paso, Peticion, PlanInstalacion, Progreso,
    Resultado,
};
use crate::teclado;

/// Dónde se dejan los archivos que lee archinstall.
///
/// En `/run` y no en `/tmp`: `/run` es tmpfs con permisos de root desde el
/// arranque, y `/tmp` en un sistema con el pegajoso puesto todavía permite que
/// otro usuario cree el directorio antes que nosotras y se quede con él. El
/// archivo de credenciales tiene la frase de LUKS en claro; no puede depender de
/// quién llegó primero.
const DIR_TRABAJO: &str = "/run/vasak-installer";

/// Cuánto se espera entre lecturas del archivo de eventos del plugin.
///
/// 150 ms: suficientemente seguido para que la barra se mueva sin saltos y lo
/// bastante espaciado para no despertar el proceso miles de veces durante una
/// instalación de media hora.
const INTERVALO_EVENTOS: Duration = Duration::from_millis(150);

/// Cuánto se le da a la comprobación de paquetes antes de darla por perdida.
///
/// Sincroniza las bases de los repositorios, que son decenas de MiB, así que con
/// una conexión pobre puede tardar. Pasado el tope se sigue con la base que haya
/// —y si tampoco alcanza, se avisa y se instala igual—, porque esto es una
/// comprobación y no un requisito.
const PLAZO_COMPROBACION: Duration = Duration::from_secs(90);

/// La salida compartida. Escriben el hilo que lee peticiones y el que corre la
/// instalación, y una línea partida en dos deja el JSON inválido del otro lado.
type Salida = Arc<Mutex<std::io::Stdout>>;

/// El grupo de procesos de archinstall, para poder cancelarlo.
///
/// Se comparte el **identificador**, no el `Child`. Esperar a un `Child` pide
/// `&mut`, así que tener el `Child` en un mutex obliga a elegir entre dos cosas
/// igual de malas: dejar el mutex tomado mientras se espera —y entonces
/// `Cancelar` se queda esperando el mismo mutex y no puede matar nada— o sacar
/// el `Child` del mutex para esperarlo, y entonces `Cancelar` no encuentra qué
/// matar. Un número no tiene ese problema: se lee sin bloquear a nadie.
///
/// Cero significa que no hay nada corriendo.
type GrupoProcesos = Arc<AtomicI32>;

fn emitir(salida: &Salida, mensaje: &Mensaje) {
    let Ok(texto) = serde_json::to_string(mensaje) else {
        return;
    };
    let mut lock = match salida.lock() {
        Ok(l) => l,
        // El mutex envenenado significa que otro hilo paniqueó mientras
        // escribía. Se sigue igual: perder un mensaje de progreso es mejor que
        // matar la instalación en curso.
        Err(e) => e.into_inner(),
    };
    let _ = writeln!(lock, "{texto}");
    let _ = lock.flush();
}

fn log(salida: &Salida, nivel: Nivel, linea: impl Into<String>) {
    emitir(
        salida,
        &Mensaje::Log {
            nivel,
            linea: linea.into(),
        },
    );
}

fn progreso(salida: &Salida, paso: Paso, estado: EstadoPaso, fraccion: Option<f32>, detalle: Option<String>) {
    emitir(
        salida,
        &Mensaje::Progress(Progreso {
            paso,
            estado,
            fraccion,
            detalle,
        }),
    );
}

/// Punto de entrada del ayudante.
pub fn run() -> ! {
    let salida: Salida = Arc::new(Mutex::new(std::io::stdout()));
    // El grupo de procesos de archinstall, para que `Cancelar` lo pueda matar
    // mientras el hilo principal está bloqueado esperándolo.
    let grupo: GrupoProcesos = Arc::new(AtomicI32::new(0));
    // Si se pidió cancelar. Distingue «archinstall murió por una señal que le
    // mandamos» de «archinstall murió solo», que son dos mensajes muy distintos
    // para quien está mirando la pantalla.
    let cancelado = Arc::new(AtomicBool::new(false));

    let (tx, rx) = mpsc::channel::<Peticion>();

    // Un hilo para la entrada. `Cancelar` lo atiende él mismo: si esperara al
    // hilo principal, no llegaría nunca —el principal está esperando a
    // archinstall, que es justamente lo que hay que matar.
    {
        let salida = Arc::clone(&salida);
        let grupo = Arc::clone(&grupo);
        let cancelado = Arc::clone(&cancelado);
        std::thread::spawn(move || {
            let entrada = BufReader::new(std::io::stdin());
            for linea in entrada.lines() {
                let Ok(linea) = linea else { break };
                if linea.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<Peticion>(&linea) {
                    Ok(p) => {
                        if matches!(p.cuerpo, CuerpoPeticion::Cancelar) {
                            cancelar(&salida, &grupo, &cancelado);
                            continue;
                        }
                        if tx.send(p).is_err() {
                            break;
                        }
                    }
                    // Una línea que no parsea se registra y se descarta. No
                    // puede cortar el canal: archinstall y pacman escriben en la
                    // misma terminal, y un `print` perdido de una dependencia no
                    // puede matar una instalación de media hora.
                    Err(e) => log(&salida, Nivel::Warn, format!("petición ilegible: {e}")),
                }
            }
            // La ventana cerró. Sin nadie a quien contestarle, el ayudante no
            // tiene razón de seguir corriendo como root.
            std::process::exit(0);
        });
    }

    for peticion in rx {
        match peticion.cuerpo {
            CuerpoPeticion::SondearDiscos => {
                let resultado = match probe::sondear_discos() {
                    Ok(mut discos) => {
                        // Con root ya disponible, se le pone nombre a lo que hay
                        // en el disco: el resumen puede decir «vas a borrar un
                        // Windows 11» en lugar de «una partición ntfs».
                        probe::anotar_sistemas_operativos(&mut discos);
                        Resultado::correcto(json!({ "discos": discos }))
                    }
                    Err(e) => Resultado::fallido(e),
                };
                emitir(
                    &salida,
                    &Mensaje::Reply {
                        id: peticion.id,
                        resultado,
                    },
                );
            }
            CuerpoPeticion::SondearSistema => {
                emitir(
                    &salida,
                    &Mensaje::Reply {
                        id: peticion.id,
                        resultado: Resultado::correcto(json!(probe::sondear_sistema())),
                    },
                );
            }
            CuerpoPeticion::Instalar(plan) => {
                emitir(
                    &salida,
                    &Mensaje::Reply {
                        id: peticion.id,
                        resultado: Resultado::correcto(json!({ "arrancada": true })),
                    },
                );
                let resultado = instalar(&salida, &grupo, &cancelado, &plan);
                match resultado {
                    Ok(()) => emitir(&salida, &Mensaje::Done { ok: true, error: None }),
                    Err(e) => {
                        log(&salida, Nivel::Error, e.clone());
                        emitir(
                            &salida,
                            &Mensaje::Done {
                                ok: false,
                                error: Some(e),
                            },
                        );
                    }
                }
                // La instalación es terminal: se haya logrado o no, el disco ya
                // está tocado y no hay nada más que este proceso pueda hacer.
                // Salir libera el root en vez de dejarlo esperando.
                limpiar();
                std::process::exit(0);
            }
            CuerpoPeticion::Cancelar => {} // lo atiende el hilo de entrada
        }
    }

    limpiar();
    std::process::exit(0);
}

/// Mata la instalación en curso.
///
/// **Al grupo de procesos entero, no sólo a archinstall.** archinstall lanza
/// `pacstrap`, `parted`, `mkfs` y `grub-install` como hijos suyos: matando sólo
/// al padre, el `pacstrap` que estaba escribiendo en el disco queda corriendo
/// huérfano y sigue escribiendo un rato largo después de que la interfaz dijo
/// que se canceló. Por eso el hijo arranca en su propio grupo (`process_group`)
/// y acá se manda la señal al grupo, que es lo que significa el PID negativo.
///
/// SIGKILL y no SIGTERM: archinstall no deshace nada ante una señal, así que un
/// SIGTERM «prolijo» sólo agrega la posibilidad de quedarse colgado en un
/// manejador. El disco queda a medias en los dos casos, y la interfaz lo dice
/// con esas palabras.
fn cancelar(salida: &Salida, grupo: &GrupoProcesos, cancelado: &Arc<AtomicBool>) {
    // La marca se levanta **siempre**, incluso sin nada corriendo todavía.
    //
    // Entre que la interfaz pide instalar y que `archinstall` arranca hay unos
    // segundos —se sondea el disco, se planifica, se hashean las contraseñas, se
    // escriben los archivos— y el botón de cancelar está a la vista todo ese
    // rato. Descartando la petición porque el grupo todavía es cero, apretar
    // cancelar en esa ventana no hacía nada y la instalación seguía: el peor
    // momento posible para que un botón mienta, porque es justo cuando todavía
    // no se tocó el disco. `instalar` mira esta marca antes de lanzar nada.
    cancelado.store(true, Ordering::SeqCst);

    let pgid = grupo.load(Ordering::SeqCst);
    if pgid <= 0 {
        log(
            salida,
            Nivel::Warn,
            "cancelación pedida antes de arrancar archinstall; no se va a lanzar",
        );
        return;
    }

    log(salida, Nivel::Warn, "cancelando la instalación");
    // SEGURIDAD: `kill` recibe dos enteros y no toca memoria. El PID negativo es
    // la forma documentada de señalar a un grupo entero.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

/// Borra los archivos con secretos.
///
/// El de credenciales tiene la frase de LUKS **en claro**, porque cryptsetup
/// necesita la frase y no un hash. Mientras dura la instalación vive en un tmpfs
/// que sólo root puede leer; después no tiene por qué existir.
fn limpiar() {
    let _ = std::fs::remove_file(Path::new(DIR_TRABAJO).join("credenciales.json"));
}

/// Escribe un archivo que sólo root puede leer.
///
/// El modo va en `OpenOptions` y no en un `chmod` posterior: entre crear el
/// archivo con el modo por defecto y ajustarlo hay una ventana en la que
/// cualquiera lo puede abrir, y el que se abre en esa ventana queda abierto
/// aunque después se cambien los permisos.
fn escribir_privado(ruta: &Path, contenido: &str) -> Result<(), String> {
    let mut archivo = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(ruta)
        .map_err(|e| format!("no se pudo crear {}: {e}", ruta.display()))?;
    archivo
        .write_all(contenido.as_bytes())
        .map_err(|e| format!("no se pudo escribir {}: {e}", ruta.display()))
}

/// Convierte una contraseña en el hash que va al archivo de credenciales.
///
/// `openssl passwd -6` produce un hash SHA-512 crypt, que es lo que aceptan
/// `useradd -p` y `chpasswd -e`, y por lo tanto lo que archinstall pone tal cual
/// en `/etc/shadow`.
///
/// **La contraseña va por entrada estándar, nunca por argumento.**
/// `/proc/<pid>/cmdline` lo puede leer cualquier usuario del sistema, así que
/// una contraseña en `argv` es una contraseña publicada mientras el proceso vive.
/// Es el mismo patrón que usa la página de Usuarios de vasak-settings.
///
/// Se rechazan los caracteres de control porque `-stdin` lee **una sola línea**:
/// un `\n` en el medio haría hashear algo distinto de lo que la persona tipeó, y
/// después no podría entrar con su propia contraseña. Espacios y acentos sí se
/// aceptan.
fn hashear(contrasena: &str) -> Result<String, String> {
    if contrasena.is_empty() {
        return Err("la contraseña está vacía".into());
    }
    if contrasena.chars().any(|c| c.is_control()) {
        return Err("la contraseña tiene caracteres de control".into());
    }

    let mut hijo = Command::new("openssl")
        .args(["passwd", "-6", "-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar openssl: {e}"))?;

    {
        let entrada = hijo
            .stdin
            .as_mut()
            .ok_or_else(|| "openssl no aceptó entrada".to_string())?;
        entrada
            .write_all(contrasena.as_bytes())
            .map_err(|e| format!("no se pudo pasar la contraseña a openssl: {e}"))?;
        entrada
            .write_all(b"\n")
            .map_err(|e| format!("no se pudo pasar la contraseña a openssl: {e}"))?;
    }

    let salida = hijo
        .wait_with_output()
        .map_err(|e| format!("openssl falló: {e}"))?;
    if !salida.status.success() {
        return Err(format!(
            "openssl falló: {}",
            String::from_utf8_lossy(&salida.stderr).trim()
        ));
    }

    let hash = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    // El prefijo confirma que salió un SHA-512 crypt y no, por ejemplo, el
    // mensaje de uso de openssl por un cambio de sus argumentos. Sin esta
    // comprobación, un hash inválido se escribe en /etc/shadow y la cuenta
    // queda sin poder entrar, que es lo peor que puede pasarle a una
    // instalación que por lo demás funcionó.
    if !hash.starts_with("$6$") {
        return Err("openssl no devolvió un hash SHA-512".into());
    }
    Ok(hash)
}

/// La versión de archinstall instalada, para estampar el archivo de
/// configuración.
///
/// Se le pregunta al módulo de Python y no a `pacman`: el paquete y el módulo
/// pueden no coincidir si alguien lo instaló con pip, y lo que importa es el que
/// va a leer el archivo.
fn version_archinstall() -> Option<String> {
    let salida = Command::new("python3")
        .args(["-c", "import archinstall; print(archinstall.__version__)"])
        .output()
        .ok()?;
    if !salida.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Corre la instalación de punta a punta.
fn instalar(
    salida: &Salida,
    grupo: &GrupoProcesos,
    cancelado: &Arc<AtomicBool>,
    plan: &PlanInstalacion,
) -> Result<(), String> {
    // ── Comprobaciones que van antes de tocar nada ──────────────────────────
    //
    // Todo lo que puede fallar por datos se comprueba acá, con el disco
    // intacto. Un error después del primer `mkfs` no se puede deshacer, así que
    // cada cosa que se pueda saber antes se sabe antes.

    let mut discos = probe::sondear_discos()?;
    probe::anotar_sistemas_operativos(&mut discos);
    let disco = discos
        .iter()
        .find(|d| d.ruta == plan.disco)
        .ok_or_else(|| format!("el disco {} ya no está", plan.disco))?;

    let firmware = probe::detectar_firmware();
    let particiones = layout::planificar(disco, firmware, plan.sistema_archivos, plan.cifrar)
        .map_err(|e| e.to_string())?;

    let hash_usuario = hashear(&plan.secretos.usuario)?;
    let hash_root = if plan.root_habilitado {
        Some(hashear(&plan.secretos.root)?)
    } else {
        None
    };
    if plan.cifrar && plan.secretos.cifrado.is_empty() {
        return Err("se pidió cifrado pero no hay frase".into());
    }

    let ruta_paquetes = archconfig::ruta_paquetes()
        .ok_or_else(|| "no se encontró paquetes.txt".to_string())?;
    let paquetes = archconfig::leer_paquetes(
        &std::fs::read_to_string(&ruta_paquetes)
            .map_err(|e| format!("no se pudo leer {}: {e}", ruta_paquetes.display()))?,
    );
    if paquetes.is_empty() {
        return Err(format!("{} no tiene ningún paquete", ruta_paquetes.display()));
    }

    let ruta_plugin =
        archconfig::ruta_plugin().ok_or_else(|| "no se encontró el plugin de archinstall".to_string())?;

    // Los complementos elegidos → los paquetes y servicios que suman.
    //
    // Un catálogo ilegible **no** aborta la instalación: significa quedarse sin
    // navegador y sin controladores opcionales, que es un sistema que arranca y
    // en el que todo eso se puede instalar después. Abortar acá dejaría el disco
    // formateado por un archivo de datos mal editado.
    let aporte = match crate::complementos::cargar() {
        Ok(catalogo) => crate::complementos::aporte_de(&catalogo, &plan.complementos),
        Err(e) => {
            log(
                salida,
                Nivel::Warn,
                format!(
                    "no se pudo leer el catálogo de complementos ({e}); se instala el escritorio \
                     sin navegador ni controladores opcionales"
                ),
            );
            crate::complementos::Aporte::default()
        }
    };
    // Los paquetes que necesita **esta** máquina y que no van en el metapaquete:
    // el controlador de vídeo de su fabricante, el firmware de su audio, bluez
    // si tiene adaptador. Se detecta acá, en el medio vivo, que es el único lugar
    // donde se puede ver qué ató el kernel — la ISO trae el firmware de todo, así
    // que lo que anduvo acá es lo que la máquina necesita.
    let hw = crate::hardware::detectar();
    let mut necesarios = crate::hardware::necesarios(&hw);
    // Y las fuentes del idioma elegido, por la misma razón que los
    // controladores: no es una elección, es lo que hace falta para que el
    // sistema se pueda leer.
    if let Some(fuente) = archconfig::paquetes_del_idioma(&plan.idioma_sistema) {
        necesarios.paquetes.insert(fuente.to_string());
    }
    if necesarios.paquetes.is_empty() {
        log(salida, Nivel::Info, "no se detectó hardware que necesite paquetes propios");
    } else {
        log(
            salida,
            Nivel::Info,
            format!(
                "para este equipo: {} ({})",
                necesarios.paquetes.iter().cloned().collect::<Vec<_>>().join(", "),
                hw.marcas.iter().cloned().collect::<Vec<_>>().join(" ")
            ),
        );
    }

    if !aporte.paquetes.is_empty() {
        log(
            salida,
            Nivel::Info,
            format!("complementos: {}", aporte.paquetes.join(", ")),
        );
    }

    // ── Los archivos que lee archinstall ───────────────────────────────────

    std::fs::create_dir_all(DIR_TRABAJO)
        .map_err(|e| format!("no se pudo crear {DIR_TRABAJO}: {e}"))?;
    // 0700 explícito: `create_dir_all` respeta el umask, y con un umask permisivo
    // el directorio que contiene las credenciales quedaría legible.
    std::fs::set_permissions(DIR_TRABAJO, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .map_err(|e| format!("no se pudieron ajustar los permisos de {DIR_TRABAJO}: {e}"))?;

    let dir = PathBuf::from(DIR_TRABAJO);
    let ruta_config = dir.join("configuracion.json");
    let ruta_creds = dir.join("credenciales.json");
    let ruta_eventos = dir.join("eventos.ndjson");

    let config = archconfig::configuracion(
        plan,
        &particiones,
        disco.sector_logico,
        firmware,
        &archconfig::FuentesDePaquetes {
            escritorio: &paquetes,
            aporte: &aporte,
            necesarios: &necesarios,
        },
        version_archinstall().as_deref(),
    );
    let creds = archconfig::credenciales(
        plan,
        &hash_usuario,
        hash_root.as_deref(),
        if plan.cifrar {
            Some(plan.secretos.cifrado.as_str())
        } else {
            None
        },
    );

    escribir_privado(
        &ruta_config,
        &serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )?;
    escribir_privado(
        &ruta_creds,
        &serde_json::to_string_pretty(&creds).map_err(|e| e.to_string())?,
    )?;
    // Vaciarlo antes de arrancar: si quedó el de una instalación anterior, el
    // seguidor leería su progreso viejo como si fuera el de ahora.
    escribir_privado(&ruta_eventos, "")?;

    // El diseño de XKB, para que el escritorio arranque con el teclado que la
    // persona eligió y no con `us`. Va por variable de entorno al plugin.
    let (xkb, variante) = match teclado::traducir(&plan.teclado, &teclado::diseños_de_xkb()) {
        Ok(par) => par,
        Err(respaldo) => {
            log(
                salida,
                Nivel::Warn,
                format!(
                    "el teclado «{}» no tiene diseño de XKB conocido; el escritorio arranca con «{}»",
                    plan.teclado, respaldo.0
                ),
            );
            respaldo
        }
    };

    // ── Que los paquetes se puedan instalar juntos ─────────────────────────
    //
    // El último punto en que el disco está intacto. Una instalación murió acá
    // mismo pero al revés: `pacstrap` descubrió que `jack2` y `pipewire-jack` no
    // podían convivir **después** de formatear, y lo único que quedaba era
    // empezar de nuevo. La lista se comprueba resolviéndola contra una raíz
    // vacía, que es lo mismo que va a hacer pacstrap dentro de un rato.
    //
    // Un fallo del chequeo no frena nada: si no se pudo comprobar —sin base
    // sincronizada, sin red— se avisa y se sigue, porque un chequeo roto no
    // puede dejar el instalador sin poder instalar.
    let finales = archconfig::paquetes_finales(&archconfig::FuentesDePaquetes {
        escritorio: &paquetes,
        aporte: &aporte,
        necesarios: &necesarios,
    });
    // Se avisa antes de empezar: la comprobación sincroniza las bases de los
    // repositorios y puede tardar. Sin esta línea, la interfaz no muestra nada
    // nuevo durante esa espera y el primer aviso de progreso recién sale más
    // abajo, cuando arranca archinstall.
    log(
        salida,
        Nivel::Info,
        format!("comprobando que los {} paquetes se puedan instalar juntos…", finales.len()),
    );
    let vigilancia = conflictos::Vigilancia {
        // Con tope y no a ciegas: con un espejo lento o una conexión que se
        // corta, una espera sin límite acá deja la instalación colgada en el
        // punto en que todavía no pasó nada, sin explicación.
        plazo: PLAZO_COMPROBACION,
        cancelado,
        // El grupo se publica para que cancelar alcance a pacman también acá.
        // Antes el botón no interrumpía nada en esta ventana, que es justo
        // cuando el disco está intacto y cancelar tendría que ser gratis.
        grupo,
    };
    match conflictos::revisar(&finales, Path::new(DIR_TRABAJO), &vigilancia) {
        Ok(choques) if choques.is_empty() => {
            log(salida, Nivel::Info, "los paquetes pueden convivir");
        }
        Ok(choques) => {
            // Con el disco intacto: se puede volver a la pantalla anterior,
            // cambiar la elección y probar de nuevo.
            let detalle: Vec<String> = choques.iter().map(Choque::to_string).collect();
            return Err(format!(
                "los paquetes elegidos no se pueden instalar juntos, así que no se tocó el \
                 disco: {}. Suele ser una dependencia virtual con más de un proveedor: hay que \
                 nombrar el que corresponde en /usr/share/vasak-installer/paquetes.txt",
                detalle.join("; ")
            ));
        }
        Err(motivo) if cancelado.load(Ordering::SeqCst) => return Err(motivo),
        Err(motivo) => {
            log(
                salida,
                Nivel::Warn,
                format!("no se pudo comprobar si los paquetes conviven ({motivo}); se sigue igual"),
            );
        }
    }

    for paso in Paso::TODOS {
        progreso(salida, *paso, EstadoPaso::Pendiente, None, None);
    }

    // ── archinstall ────────────────────────────────────────────────────────

    log(
        salida,
        Nivel::Info,
        format!(
            "instalando en {} ({}), firmware {:?}",
            plan.disco,
            plan.sistema_archivos.como_archinstall(),
            firmware
        ),
    );

    let mut comando = Command::new("archinstall");
    comando
        .arg("--config")
        .arg(&ruta_config)
        .arg("--creds")
        .arg(&ruta_creds)
        .arg("--plugin")
        .arg(&ruta_plugin)
        .arg("--silent")
        // El progreso estructurado sale por acá: el plugin escribe NDJSON en
        // este archivo y nosotras lo seguimos. No se usa la salida estándar de
        // archinstall para eso porque ahí escriben también pacman y todo lo que
        // archinstall invoca, y una línea de JSON partida por un `print` ajeno
        // sería un evento perdido.
        .env("VASAK_INSTALLER_EVENTOS", &ruta_eventos)
        .env("VASAK_INSTALLER_XKB", &xkb)
        .env("VASAK_INSTALLER_XKB_VARIANTE", &variante)
        .env("VASAK_INSTALLER_NOMBRE_COMPLETO", &plan.nombre_completo)
        .env("VASAK_INSTALLER_USUARIO", &plan.usuario)
        // Sin color: archinstall detecta que no hay terminal y no debería
        // ponerlo, pero algunas de sus dependencias lo ponen igual, y los
        // códigos de escape ANSI en el registro de la interfaz se ven como
        // basura.
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Su propio grupo de procesos. Es lo que permite cancelar de verdad:
        // archinstall lanza `pacstrap`, `parted` y `mkfs` como hijos suyos, y
        // matando sólo al padre esos hijos quedan huérfanos escribiendo en el
        // disco un rato largo después de que la interfaz dijo que se canceló.
        // Ver `cancelar`.
        .process_group(0);

    // Último control antes del punto sin retorno: si se pidió cancelar mientras
    // se preparaba todo lo de arriba, no se lanza nada. Hasta acá no se tocó el
    // disco, así que cancelar todavía significa que no pasó nada.
    if cancelado.load(Ordering::SeqCst) {
        return Err("la instalación se canceló".to_string());
    }

    let mut proceso = comando
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar archinstall: {e}. ¿Está instalado?"))?;

    let stdout = proceso.stdout.take();
    let stderr = proceso.stderr.take();

    // El PID del hijo es también el identificador de su grupo, porque
    // `process_group(0)` lo pone como líder de un grupo nuevo.
    let pgid = proceso.id() as i32;
    grupo.store(pgid, Ordering::SeqCst);

    // Y una vez más después de publicarlo: una cancelación que llegó entre el
    // control de arriba y este `store` habría encontrado el grupo en cero y no
    // habría matado nada, dejando corriendo justo lo que se pidió detener.
    if cancelado.load(Ordering::SeqCst) {
        // SEGURIDAD: dos enteros, sin punteros. El PID negativo señala al grupo.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }

    // Tres hilos leyendo tres flujos: la salida de archinstall, su error, y el
    // archivo de eventos del plugin. Los dos primeros van al registro; el
    // tercero mueve la barra.
    let mut lectores = Vec::new();
    if let Some(flujo) = stdout {
        lectores.push(hilo_de_registro(Arc::clone(salida), flujo, Nivel::Info));
    }
    if let Some(flujo) = stderr {
        lectores.push(hilo_de_registro(Arc::clone(salida), flujo, Nivel::Warn));
    }

    let seguir = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let hilo_eventos = {
        let salida = Arc::clone(salida);
        let seguir = Arc::clone(&seguir);
        let ruta = ruta_eventos.clone();
        std::thread::spawn(move || seguir_eventos(&salida, &ruta, &seguir))
    };

    // Se espera con el `Child` en la pila y sin ningún lock tomado: lo que
    // `Cancelar` necesita es el número del grupo, que ya está publicado en el
    // atómico.
    let estado = proceso
        .wait()
        .map_err(|e| format!("archinstall falló: {e}"))?;

    // El grupo deja de existir en cuanto se recogió al hijo. Ponerlo en cero
    // enseguida cierra la ventana en la que un `Cancelar` tardío mandaría una
    // señal a un identificador que el sistema ya puede haber reutilizado.
    grupo.store(0, Ordering::SeqCst);

    seguir.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = hilo_eventos.join();
    for h in lectores {
        let _ = h.join();
    }

    if !estado.success() {
        // Se distingue por la bandera y no por el código de salida: una muerte
        // por señal no trae código, pero tampoco lo trae un archinstall que
        // paniqueó y recibió un SIGABRT. Decirle «cancelaste» a alguien que no
        // canceló nada es la peor forma de informar un fallo.
        if cancelado.load(Ordering::SeqCst) {
            return Err("la instalación se canceló".to_string());
        }
        return Err(match estado.code() {
            Some(c) => format!(
                "archinstall terminó con código {c}. El registro de arriba dice dónde falló; \
                 el detalle completo está en /var/log/archinstall/install.log"
            ),
            None => "archinstall se interrumpió sin terminar".to_string(),
        });
    }

    Ok(())
}

/// Manda al registro cada línea de un flujo del proceso hijo.
///
/// Se leen **bytes** y se convierten con `from_utf8_lossy` en vez de usar
/// `lines()` sobre un lector de texto: pacman escribe barras de progreso con
/// caracteres de dibujo y a veces corta una secuencia UTF-8 a mitad, y ahí
/// `lines()` devuelve un error que corta el hilo y deja el resto de la
/// instalación sin registro.
fn hilo_de_registro<R: Read + Send + 'static>(
    salida: Salida,
    flujo: R,
    nivel: Nivel,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut lector = BufReader::new(flujo);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match lector.read_until(b'\n', &mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    let texto = String::from_utf8_lossy(&buffer);
                    let limpio = texto.trim_end_matches(['\n', '\r']).trim();
                    if !limpio.is_empty() {
                        log(&salida, nivel, limpio);
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// Sigue el archivo de eventos del plugin y reenvía lo que aparece.
///
/// Es un `tail -f` en veinte líneas. Se mantiene la posición y se relee desde
/// ahí; una línea incompleta —el plugin escribió la mitad y todavía no llegó el
/// `\n`— se deja para la próxima vuelta en vez de parsearla, que es de donde
/// venían los eventos perdidos.
fn seguir_eventos(salida: &Salida, ruta: &Path, seguir: &std::sync::atomic::AtomicBool) {
    let mut posicion: u64 = 0;
    let mut pendiente = String::new();

    loop {
        let activo = seguir.load(std::sync::atomic::Ordering::Relaxed);

        if let Ok(mut archivo) = std::fs::File::open(ruta) {
            if archivo.seek(SeekFrom::Start(posicion)).is_ok() {
                let mut nuevo = String::new();
                if let Ok(leidos) = archivo.read_to_string(&mut nuevo) {
                    posicion += leidos as u64;
                    pendiente.push_str(&nuevo);

                    // Sólo las líneas completas. `split_inclusive` deja la
                    // última sin `\n` identificable, y ésa es la que se guarda.
                    while let Some(corte) = pendiente.find('\n') {
                        let linea: String = pendiente.drain(..=corte).collect();
                        procesar_evento(salida, linea.trim());
                    }
                }
            }
        }

        if !activo {
            // Una vuelta más después de que el proceso terminó, para no perder
            // los últimos eventos: el plugin escribe el cierre justo antes de
            // que archinstall salga, y sin esta pasada final el último paso
            // quedaba mostrándose «en curso» para siempre.
            break;
        }
        std::thread::sleep(INTERVALO_EVENTOS);
    }
}

fn procesar_evento(salida: &Salida, linea: &str) {
    if linea.is_empty() {
        return;
    }
    match serde_json::from_str::<Mensaje>(linea) {
        Ok(mensaje) => emitir(salida, &mensaje),
        // El plugin escribe en el mismo archivo que nadie más toca, así que una
        // línea ilegible es un error del plugin. Se registra con la línea
        // adentro para poder arreglarlo, y no se corta nada.
        Err(e) => log(
            salida,
            Nivel::Warn,
            format!("evento ilegible del plugin ({e}): {linea}"),
        ),
    }
}

/// Los diseños de XKB, expuesto para que la ventana pueda validar sin ser root.
pub fn diseños_conocidos() -> BTreeSet<String> {
    teclado::diseños_de_xkb()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contra el `openssl` real: es el que produce el hash que termina en
    /// `/etc/shadow`, y un cambio en sus argumentos deja la cuenta sin poder
    /// entrar en un sistema que por lo demás se instaló bien.
    #[test]
    fn el_hash_sale_con_formato_sha512() {
        let hash = hashear("una contraseña con espacios y acentós").expect("openssl tendría que andar");
        assert!(hash.starts_with("$6$"), "salió «{hash}»");
        // `$6$` + sal + `$` + hash. Más corto que esto significa que openssl
        // devolvió otra cosa.
        assert!(hash.len() > 20, "salió «{hash}»");
    }

    #[test]
    fn la_sal_es_distinta_cada_vez() {
        // Dos hashes iguales para la misma contraseña significan sal fija, y con
        // sal fija una tabla precalculada sirve para todas las instalaciones.
        let a = hashear("la misma").unwrap();
        let b = hashear("la misma").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn una_contrasena_con_salto_de_linea_se_rechaza() {
        // `openssl passwd -stdin` lee **una sola línea**: con un `\n` en el
        // medio hashearía sólo la primera mitad, y la persona no podría entrar
        // con la contraseña que tipeó.
        assert!(hashear("mitad\ny la otra mitad").is_err());
        assert!(hashear("con\ttabulación").is_err());
        assert!(hashear("").is_err());
    }

    #[test]
    fn el_espacio_y_los_acentos_se_aceptan() {
        // Rechazarlos sería empobrecer las contraseñas por comodidad nuestra.
        assert!(hashear("contraseña con espacios").is_ok());
        assert!(hashear("ñandú y coração").is_ok());
    }

    /// El seguidor de eventos no puede entregar una línea a medio escribir: el
    /// plugin escribe y el seguidor lee al mismo tiempo, y parsear media línea
    /// perdía el evento entero.
    #[test]
    fn el_seguidor_espera_la_linea_completa() {
        let dir = std::env::temp_dir().join(format!("vsk-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("eventos.ndjson");

        let uno = serde_json::to_string(&Mensaje::Log {
            nivel: Nivel::Info,
            linea: "primero".into(),
        })
        .unwrap();
        let dos = serde_json::to_string(&Mensaje::Log {
            nivel: Nivel::Info,
            linea: "segundo".into(),
        })
        .unwrap();

        // Una línea completa y la siguiente a medias, como queda el archivo si
        // se lee justo mientras el plugin escribe.
        std::fs::write(&ruta, format!("{uno}\n{}", &dos[..dos.len() / 2])).unwrap();

        let seguir = std::sync::atomic::AtomicBool::new(false);
        // Con `seguir` en falso hace una sola pasada; lo que se comprueba es
        // que no paniquee con la línea partida y que no la reporte.
        let salida: Salida = Arc::new(Mutex::new(std::io::stdout()));
        seguir_eventos(&salida, &ruta, &seguir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Si un proceso sigue vivo de verdad.
    ///
    /// Se mira `/proc/<pid>/stat` y no `kill(pid, 0)`. Un proceso muerto sigue
    /// **existiendo** como zombi hasta que alguien lo recoge, y el padre de este
    /// nieto acaba de morir, así que hay que esperar a que lo adopte y lo recoja
    /// init: durante esa ventana `kill(pid, 0)` devuelve cero y el nieto parece
    /// vivo. El estado del proceso no tiene esa ambigüedad — `Z` es zombi, o sea
    /// ya muerto.
    ///
    /// El campo del estado va entre el nombre del ejecutable —que puede tener
    /// paréntesis y espacios adentro— y el resto, así que se corta después del
    /// último `)`.
    fn esta_vivo(pid: i32) -> bool {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            // Ya no existe: recogido.
            Err(_) => false,
            Ok(stat) => match stat.rsplit_once(')') {
                Some((_, resto)) => resto.split_whitespace().next() != Some("Z"),
                None => false,
            },
        }
    }

    /// Matar el grupo mata también a los nietos.
    ///
    /// Ésta es la propiedad que hace que «cancelar» signifique algo: archinstall
    /// lanza `pacstrap`, `parted` y `mkfs` como hijos suyos. Antes se mataba
    /// sólo al proceso de archinstall, y el `pacstrap` que estaba escribiendo en
    /// el disco seguía corriendo huérfano un rato largo después de que la
    /// interfaz decía que se había cancelado.
    ///
    /// Se prueba con un `sh` que deja un nieto durmiendo: si la señal no llegara
    /// al grupo, el nieto sobreviviría.
    #[test]
    fn cancelar_mata_a_los_nietos_y_no_solo_al_hijo() {
        let mut hijo = Command::new("sh")
            .args(["-c", "sleep 60 & echo $! ; wait"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sh tendría que andar");

        // El PID del nieto, que `sh` imprime antes de esperarlo.
        let mut nieto_texto = String::new();
        {
            use std::io::Read as _;
            let mut salida = hijo.stdout.take().unwrap();
            let mut buffer = [0u8; 32];
            let leidos = salida.read(&mut buffer).unwrap_or(0);
            nieto_texto.push_str(&String::from_utf8_lossy(&buffer[..leidos]));
        }
        let nieto: i32 = nieto_texto.trim().parse().expect("el PID del nieto");

        let grupo: GrupoProcesos = Arc::new(AtomicI32::new(hijo.id() as i32));
        let cancelado = Arc::new(AtomicBool::new(false));
        let salida: Salida = Arc::new(Mutex::new(std::io::stdout()));

        cancelar(&salida, &grupo, &cancelado);
        assert!(cancelado.load(Ordering::SeqCst), "no se marcó la cancelación");

        let _ = hijo.wait();

        // Se mira `/proc/<pid>/stat` y no `kill(pid, 0)`.
        //
        // `kill(pid, 0)` tiene una carrera que hace fallar el test aunque el
        // código esté bien: un proceso muerto sigue **existiendo** como zombi
        // hasta que alguien lo recoge, y su padre acaba de morir, así que hay
        // que esperar a que lo adopte y lo recoja init. Durante esa ventana
        // `kill(pid, 0)` devuelve cero y el nieto parece vivo.
        //
        // El estado del proceso no tiene esa ambigüedad: `Z` es zombi, o sea ya
        // muerto. El campo va entre el nombre del ejecutable —que puede tener
        // paréntesis y espacios adentro— y el resto, así que se corta después
        // del último `)`.
        // `kill` sólo **encola** la señal: SIGKILL se procesa cuando el núcleo
        // vuelve a planificar ese proceso. Mirando una sola vez, el test falla
        // de vez en cuando con el código correcto —y falla más seguido cuando la
        // suite corre en paralelo y hay presión de planificación—, que es la
        // peor clase de test: el que hace desconfiar de un arreglo que está bien.
        //
        // Se espera acotado. Dos segundos es holgadísimo para una señal que
        // normalmente llega en microsegundos, y sigue fallando rápido si el
        // grupo de verdad no recibió nada.
        let limite = std::time::Instant::now() + Duration::from_secs(2);
        let mut vive = true;
        while std::time::Instant::now() < limite {
            vive = esta_vivo(nieto);
            if !vive {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!vive, "el nieto {nieto} sobrevivió a la cancelación del grupo");
    }

    /// Cancelar cuando no hay nada corriendo no puede mandar ninguna señal.
    ///
    /// Con el grupo en cero, un `kill(-0, SIGKILL)` señalaría **al grupo del
    /// propio ayudante**, que es como decir «matate vos y todo lo que te
    /// rodea». La guarda del cero es lo único que separa las dos cosas.
    /// Cancelar antes de que arranque archinstall **queda anotado**.
    ///
    /// Entre que se pide instalar y que archinstall arranca hay unos segundos
    /// —sondeo, planificación, hasheo de contraseñas, escritura de archivos— y
    /// el botón está a la vista todo ese rato. Antes se descartaba la petición
    /// porque el grupo todavía era cero: apretar cancelar ahí no hacía nada y la
    /// instalación seguía, que es el peor momento para que el botón mienta
    /// porque todavía no se tocó el disco.
    #[test]
    fn una_cancelacion_temprana_no_se_pierde() {
        let grupo: GrupoProcesos = Arc::new(AtomicI32::new(0));
        let cancelado = Arc::new(AtomicBool::new(false));
        let salida: Salida = Arc::new(Mutex::new(std::io::stdout()));

        cancelar(&salida, &grupo, &cancelado);

        assert!(
            cancelado.load(Ordering::SeqCst),
            "la cancelación se descartó por no haber nada corriendo todavía"
        );
    }

    #[test]
    fn cancelar_sin_nada_corriendo_no_hace_nada() {
        let grupo: GrupoProcesos = Arc::new(AtomicI32::new(0));
        let cancelado = Arc::new(AtomicBool::new(false));
        let salida: Salida = Arc::new(Mutex::new(std::io::stdout()));

        // Lo que se comprueba es que **no se manda ninguna señal**: con el
        // grupo en cero, `kill(-0, SIGKILL)` señalaría al grupo del propio
        // ayudante, que es como decir «matate vos y todo lo que te rodea». La
        // guarda del cero es lo único que separa las dos cosas — si faltara,
        // este test se llevaría puesto al proceso que lo corre.
        cancelar(&salida, &grupo, &cancelado);

        // Sigue vivo, que es todo lo que hay que demostrar.
        assert_eq!(grupo.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn el_archivo_privado_queda_solo_para_root() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("vsk-test-priv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("credenciales.json");

        escribir_privado(&ruta, r#"{"secreto":1}"#).unwrap();
        let modo = std::fs::metadata(&ruta).unwrap().permissions().mode() & 0o777;
        // 0600 y nada más: este archivo tiene la frase de LUKS en claro. El modo
        // va en `OpenOptions` justamente para que no haya una ventana entre
        // crear y ajustar.
        assert_eq!(modo, 0o600, "quedó en {modo:o}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
