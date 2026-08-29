//! Los comandos que llama la interfaz.
//!
//! Están partidos en dos grupos y la diferencia importa:
//!
//!  - **Sin privilegios**, atendidos en este mismo proceso: el sondeo del
//!    equipo, los catálogos de zonas/idiomas/teclados, la validación de los
//!    nombres y la vista previa del particionado. Todo sale de `/proc`, `/sys` y
//!    `/usr/share`, que son legibles por cualquiera.
//!  - **Con privilegios**, reenviados al ayudante: ponerle nombre a los sistemas
//!    operativos que ya están instalados, y la instalación.
//!
//! La vista previa del particionado es del primer grupo a propósito: la persona
//! tiene que poder ver **exactamente** qué particiones se van a crear antes de
//! que exista un proceso root en la máquina.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::layout::{self, Disco, Firmware, Rol};
use crate::probe::{self, Sistema};
use crate::protocol::{CuerpoPeticion, Paso, PlanInstalacion, SistemaArchivos};
use crate::sidecar::Ayudante;
use crate::validar::{self, ErrorNombre, Fuerza};

/// El ayudante, si ya se lanzó.
///
/// Detrás de un `Arc` y no directo en el `Mutex` porque hay comandos que le
/// hablan y esperan: para no tener el mutex tomado durante esa espera, se clona
/// el `Arc` bajo el lock y se suelta antes de hablar. Ver
/// `sondear_discos_con_sistemas`.
pub struct EstadoAyudante(pub Mutex<Option<Arc<Ayudante>>>);

impl Default for EstadoAyudante {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Debug, Serialize)]
pub struct Catalogos {
    pub zonas: Vec<String>,
    pub idiomas: Vec<String>,
    pub teclados: Vec<String>,
}

/// Una partición del plan, en la forma que muestra el resumen.
///
/// Los tamaños van en bytes y no formateados: el formato es cosa de la interfaz,
/// que sabe en qué idioma está. Un «1,0 GiB» armado en Rust aparecería con coma
/// decimal en una interfaz en inglés.
#[derive(Debug, Serialize)]
pub struct ParticionVistaPrevia {
    pub rol: String,
    pub inicio_bytes: u64,
    pub tamano_bytes: u64,
    pub sistema_archivos: Option<String>,
    pub punto_montaje: Option<String>,
    pub opciones_montaje: Vec<String>,
    pub subvolumenes: Vec<String>,
    pub cifrada: bool,
}

#[derive(Debug, Serialize)]
pub struct VistaPrevia {
    pub firmware: String,
    pub particiones: Vec<ParticionVistaPrevia>,
    /// Lo que se va a perder, para que el resumen lo pueda enumerar en vez de
    /// decir «se borrará todo» y esperar que se crea.
    pub se_pierde: Vec<String>,
}

// ── Sin privilegios ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn sondear_sistema() -> Sistema {
    probe::sondear_sistema()
}

/// Los discos, sin nombre de sistema operativo.
///
/// Se puede llamar antes de que exista el ayudante, y es lo que permite que el
/// paso del disco muestre algo de inmediato en vez de después de la
/// autorización.
#[tauri::command]
pub fn sondear_discos() -> Result<Vec<Disco>, String> {
    probe::sondear_discos()
}

#[tauri::command]
pub fn catalogos() -> Catalogos {
    Catalogos {
        zonas: probe::zonas_horarias(),
        idiomas: probe::idiomas(),
        teclados: probe::teclados(),
    }
}

/// Los pasos de la instalación, en orden, como claves de traducción.
///
/// Los da el backend y no una lista en el frontend porque el backend es quien
/// decide cuándo empieza cada uno: dos listas se desincronizan y la barra
/// termina mostrando un paso que ya pasó.
#[tauri::command]
pub fn pasos_de_instalacion() -> Vec<&'static str> {
    Paso::TODOS.iter().map(|p| p.clave()).collect()
}

#[tauri::command]
pub fn validar_usuario(nombre: String) -> Result<(), ErrorNombre> {
    validar::nombre_de_usuario(&nombre)
}

#[tauri::command]
pub fn validar_equipo(nombre: String) -> Result<(), ErrorNombre> {
    validar::nombre_de_equipo(&nombre)
}

#[tauri::command]
pub fn sugerir_usuario(nombre_completo: String) -> String {
    validar::sugerir_usuario(&nombre_completo)
}

#[tauri::command]
pub fn fuerza_contrasena(contrasena: String) -> Fuerza {
    validar::fuerza(&contrasena)
}

/// Calcula qué particiones se van a crear, sin crear nada.
///
/// Es la pantalla que la persona mira antes de aceptar el punto sin retorno, así
/// que usa **la misma función** que después ejecuta el ayudante. Un cálculo
/// distinto para mostrar y para hacer es un resumen que puede mentir.
#[tauri::command]
pub fn vista_previa_particionado(
    disco: String,
    sistema_archivos: SistemaArchivos,
    cifrar: bool,
) -> Result<VistaPrevia, String> {
    let discos = probe::sondear_discos()?;
    let elegido = discos
        .iter()
        .find(|d| d.ruta == disco)
        .ok_or_else(|| format!("no se encontró el disco {disco}"))?;

    let firmware = probe::detectar_firmware();
    let plan = layout::planificar(elegido, firmware, sistema_archivos, cifrar)
        .map_err(|e| e.to_string())?;

    Ok(vista_previa_de(elegido, firmware, &plan))
}

/// Convierte el plan a lo que muestra el resumen.
///
/// Separada del comando porque el comando hace E/S —sondea los discos— y esto
/// es aritmética pura. Es lo que permite comparar la vista previa contra el plan
/// real en un test, con números conocidos y sin depender de que la máquina que
/// corre los tests tenga un disco.
fn vista_previa_de(
    disco: &Disco,
    firmware: Firmware,
    plan: &[layout::ParticionPlaneada],
) -> VistaPrevia {
    const MIB: u64 = 1024 * 1024;
    VistaPrevia {
        firmware: match firmware {
            Firmware::Uefi => "uefi".into(),
            Firmware::Bios => "bios".into(),
        },
        particiones: plan
            .iter()
            .map(|p| ParticionVistaPrevia {
                rol: match p.rol {
                    Rol::BiosGrub => "bios_grub".into(),
                    Rol::Esp => "esp".into(),
                    Rol::Raiz => "raiz".into(),
                },
                inicio_bytes: p.inicio_mib * MIB,
                tamano_bytes: p.tamano_mib * MIB,
                sistema_archivos: p.sistema_archivos.map(str::to_owned),
                punto_montaje: p.punto_montaje.map(str::to_owned),
                opciones_montaje: p.opciones_montaje.clone(),
                subvolumenes: p
                    .subvolumenes
                    .iter()
                    .map(|(nombre, punto)| format!("{nombre} → {punto}"))
                    .collect(),
                cifrada: p.cifrada,
            })
            .collect(),
        se_pierde: disco
            .particiones
            .iter()
            .map(|p| match (&p.sistema_operativo, &p.sistema_archivos) {
                // El sistema operativo primero: «Windows 11» dice mucho más que
                // «ntfs», y es lo que hace que alguien se detenga a mirar.
                (Some(os), _) => format!("{} — {os}", p.ruta),
                (None, Some(fs)) => format!("{} — {fs}", p.ruta),
                (None, None) => p.ruta.clone(),
            })
            .collect(),
    }
}

// ── Con privilegios ─────────────────────────────────────────────────────────

/// Lanza el ayudante si no está.
///
/// Se llama al entrar al paso del disco: es el primer momento en que root hace
/// falta, y ya está claro para qué. Idempotente — si el ayudante vive, no hace
/// nada, porque el paso se puede visitar varias veces yendo y viniendo.
/// `async` por la misma razón que el sondeo: `Ayudante::lanzar` invoca `pkexec`,
/// y fuera del medio live eso abre un diálogo de autenticación. El lanzamiento
/// en sí no espera a que la persona conteste —pkexec vuelve enseguida y el
/// canal falla después si no autorizó—, pero sí hace trabajo de arranque de
/// proceso que no tiene por qué ocurrir en el hilo que repinta la ventana.
#[tauri::command]
pub async fn preparar_ayudante(app: AppHandle, estado: State<'_, EstadoAyudante>) -> Result<(), String> {
    let mut guard = estado
        .0
        .lock()
        .map_err(|_| "el estado del ayudante está roto".to_string())?;

    if guard.as_ref().is_some_and(|a| a.vivo()) {
        return Ok(());
    }

    // Uno muerto se reemplaza. Antes se devolvía `Ok` si había *algo* guardado,
    // y después de una autorización rechazada el instalador quedaba con un
    // ayudante cadáver y sin forma de volver a intentar sin cerrar la ventana.
    *guard = Some(Arc::new(Ayudante::lanzar(app)?));
    Ok(())
}

#[tauri::command]
pub fn ayudante_listo(estado: State<'_, EstadoAyudante>) -> bool {
    estado
        .0
        .lock()
        .map(|g| g.as_ref().is_some_and(|a| a.vivo()))
        .unwrap_or(false)
}

/// Los discos con el nombre de los sistemas operativos que ya están instalados.
///
/// Necesita el ayudante porque `os-prober` monta particiones ajenas para mirar
/// qué tienen adentro.
/// **`async` y con la espera en `spawn_blocking`.**
///
/// Un comando de Tauri declarado sin `async` corre en el hilo principal, y este
/// espera al ayudante, que a su vez corre `os-prober` — que monta cada partición
/// ajena para mirar qué tiene adentro. En un disco con tres sistemas eso son
/// varios segundos con la ventana congelada: sin repintar, sin responder al
/// mouse, sin poder cerrarse. Y el plazo de la espera son dos minutos.
///
/// El `Arc` se clona **bajo el lock** y el lock se suelta antes de hablar: con
/// el mutex tomado durante la espera, cualquier otro comando que lo necesite
/// —`cancelar_instalacion`, por ejemplo— se quedaría esperando lo mismo.
#[tauri::command]
pub async fn sondear_discos_con_sistemas(
    estado: State<'_, EstadoAyudante>,
) -> Result<Vec<Disco>, String> {
    let ayudante = {
        let guard = estado
            .0
            .lock()
            .map_err(|_| "el estado del ayudante está roto".to_string())?;
        guard
            .as_ref()
            .ok_or_else(|| "el ayudante privilegiado no está corriendo".to_string())?
            .clone()
    };

    let data = tauri::async_runtime::spawn_blocking(move || {
        ayudante.pedir(CuerpoPeticion::SondearDiscos)
    })
    .await
    .map_err(|e| format!("el sondeo de discos se interrumpió: {e}"))??;

    serde_json::from_value(data["discos"].clone())
        .map_err(|e| format!("no se entendió la respuesta del ayudante: {e}"))
}

/// Arranca la instalación. **Es el punto sin retorno.**
///
/// Vuelve en cuanto el ayudante confirma que arrancó, no cuando termina: lo que
/// sigue llega por los eventos `instalacion://progreso`, `…/registro` y
/// `…/fin`. Un comando que se quedara esperando media hora dejaría el hilo del
/// IPC tomado y la ventana sin poder ni siquiera mostrar el progreso.
#[tauri::command]
pub fn instalar(
    plan: PlanInstalacion,
    estado: State<'_, EstadoAyudante>,
) -> Result<(), String> {
    // Se vuelve a validar acá aunque el frontend ya lo haya hecho: lo que el
    // frontend valida es para poder decirlo mientras se escribe, y esta es la
    // comprobación que decide. Un nombre inválido que pase de acá lo rechaza
    // `useradd` con el disco ya formateado.
    validar::nombre_de_usuario(&plan.usuario)
        .map_err(|e| format!("el nombre de usuario no es válido: {e:?}"))?;
    validar::nombre_de_equipo(&plan.hostname)
        .map_err(|e| format!("el nombre del equipo no es válido: {e:?}"))?;

    // La zona, el idioma y el teclado se pueden escribir a mano cuando el
    // sistema no pudo dar los catálogos, así que acá pueden llegar valores que
    // nadie eligió de una lista. Un valor inválido no falla al principio: falla
    // dentro del chroot, cuando `localectl` o `loadkeys` lo rechazan, con el
    // disco ya formateado y el mensaje enterrado en el registro de archinstall.
    validar::zona_horaria(&plan.zona_horaria)
        .map_err(|e| format!("la zona horaria no es válida: {e}"))?;
    validar::idioma(&plan.idioma_sistema)
        .map_err(|e| format!("el idioma del sistema no es válido: {e}"))?;
    validar::teclado(&plan.teclado)
        .map_err(|e| format!("la distribución de teclado no es válida: {e}"))?;
    if plan.secretos.usuario.is_empty() {
        return Err("falta la contraseña del usuario".into());
    }
    if plan.root_habilitado && plan.secretos.root.is_empty() {
        return Err("se habilitó root pero falta su contraseña".into());
    }
    if plan.cifrar && plan.secretos.cifrado.is_empty() {
        return Err("se pidió cifrado pero falta la frase".into());
    }

    let guard = estado
        .0
        .lock()
        .map_err(|_| "el estado del ayudante está roto".to_string())?;
    let ayudante = guard
        .as_ref()
        .ok_or_else(|| "el ayudante privilegiado no está corriendo".to_string())?;

    ayudante.pedir(CuerpoPeticion::Instalar(Box::new(plan)))?;
    Ok(())
}

#[tauri::command]
pub fn cancelar_instalacion(estado: State<'_, EstadoAyudante>) -> Result<(), String> {
    let guard = estado
        .0
        .lock()
        .map_err(|_| "el estado del ayudante está roto".to_string())?;
    let ayudante = guard
        .as_ref()
        .ok_or_else(|| "el ayudante privilegiado no está corriendo".to_string())?;
    ayudante.enviar(CuerpoPeticion::Cancelar)
}

/// Reinicia el equipo desde la pantalla final.
///
/// `systemctl` y no una llamada a logind por D-Bus: en el medio live el usuario
/// está en `wheel` y `systemctl reboot` funciona por polkit sin preguntar. Es la
/// misma decisión que toma `vasak-session-manager` para sus botones de energía.
#[tauri::command]
pub fn reiniciar() -> Result<(), String> {
    ejecutar_systemctl("reboot")
}

#[tauri::command]
pub fn apagar() -> Result<(), String> {
    ejecutar_systemctl("poweroff")
}

fn ejecutar_systemctl(accion: &str) -> Result<(), String> {
    let salida = std::process::Command::new("systemctl")
        .arg(accion)
        .output()
        .map_err(|e| format!("no se pudo ejecutar systemctl: {e}"))?;
    if salida.status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl {accion} falló: {}",
            String::from_utf8_lossy(&salida.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La vista previa muestra **exactamente** lo que se va a hacer.
    ///
    /// Es la pantalla que alguien mira antes de aceptar el punto sin retorno. Si
    /// alguien duplicara el cálculo para «mostrar algo rápido», el resumen
    /// podría decir una cosa y el disco terminar con otra, y nadie se enteraría
    /// hasta después de formatear.
    ///
    /// Se compara campo por campo contra `planificar`, que es lo que ejecuta el
    /// ayudante. Antes esto afirmaba `p.inicio_mib * MIB % MIB == 0`, que es cero
    /// para cualquier valor: el test pasaba siempre y no comprobaba nada.
    #[test]
    fn la_vista_previa_coincide_con_el_plan_real() {
        // Un disco inventado, no el de la máquina que corre el test: así se
        // compara la conversión con números conocidos y el test también sirve en
        // un contenedor sin ningún dispositivo de bloque.
        let disco = Disco {
            ruta: "/dev/prueba".into(),
            modelo: "Disco de prueba".into(),
            tamano_bytes: 256 * 1024 * 1024 * 1024,
            sector_logico: 512,
            rotacional: false,
            nvme: true,
            en_uso: false,
            particiones: Vec::new(),
        };

        const MIB: u64 = 1024 * 1024;
        for firmware in [Firmware::Uefi, Firmware::Bios] {
            for fs in [SistemaArchivos::Btrfs, SistemaArchivos::Ext4] {
                let plan = layout::planificar(&disco, firmware, fs, false).unwrap();
                let vista = vista_previa_de(&disco, firmware, &plan);

                assert_eq!(vista.particiones.len(), plan.len());
                for (previa, real) in vista.particiones.iter().zip(plan.iter()) {
                    assert_eq!(previa.inicio_bytes, real.inicio_mib * MIB);
                    assert_eq!(previa.tamano_bytes, real.tamano_mib * MIB);
                    assert_eq!(previa.sistema_archivos.as_deref(), real.sistema_archivos);
                    assert_eq!(previa.punto_montaje.as_deref(), real.punto_montaje);
                    assert_eq!(previa.opciones_montaje, real.opciones_montaje);
                    assert_eq!(previa.cifrada, real.cifrada);
                    assert_eq!(previa.subvolumenes.len(), real.subvolumenes.len());
                }
            }
        }
    }

    /// Con cifrado, la raíz de la vista previa tiene que salir marcada.
    ///
    /// Es el único aviso visible de que el disco va a pedir una frase en cada
    /// arranque.
    #[test]
    fn la_vista_previa_marca_la_particion_cifrada() {
        let disco = Disco {
            ruta: "/dev/prueba".into(),
            modelo: "Disco de prueba".into(),
            tamano_bytes: 64 * 1024 * 1024 * 1024,
            sector_logico: 512,
            rotacional: false,
            nvme: false,
            en_uso: false,
            particiones: Vec::new(),
        };
        let plan = layout::planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, true).unwrap();
        let vista = vista_previa_de(&disco, Firmware::Uefi, &plan);

        assert_eq!(vista.particiones[0].cifrada, false, "el ESP nunca va cifrado");
        assert!(vista.particiones[1].cifrada, "la raíz sí");
    }

    #[test]
    fn los_pasos_llegan_al_frontend_en_orden() {
        let pasos = pasos_de_instalacion();
        assert_eq!(pasos.first(), Some(&"particionar"));
        assert_eq!(pasos.last(), Some(&"cierre"));
        assert_eq!(pasos.len(), Paso::TODOS.len());
    }
}
