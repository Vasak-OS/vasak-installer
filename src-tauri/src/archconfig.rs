//! Traducción del plan de la interfaz al JSON que come archinstall.
//!
//! archinstall se maneja con **dos** archivos: `user_configuration.json` con
//! todo lo que no es secreto, y `user_credentials.json` con las contraseñas. La
//! separación es suya, no nuestra, y viene bien: el primero se puede dejar en el
//! sistema instalado como constancia de cómo se instaló, el segundo se borra.
//!
//! Este módulo es la frontera con archinstall. Todo lo que sabe de su esquema
//! —los nombres de las claves, que `Grub` va con mayúscula, que los tamaños son
//! objetos con unidad y sector— está acá y en ningún otro lado. Cuando
//! archinstall cambie de versión mayor, es el único archivo que hay que revisar.
//!
//! Lo que **no** hace: la post-configuración de VasakOS. Eso vive en el plugin
//! de Python (`plugin/vasakos.py`), enganchado a los ganchos que archinstall
//! define para eso, y no en una lista de comandos de shell dentro del JSON. Un
//! `custom_commands: ["sed -i ..."]` es código sin tests, sin manejo de errores
//! y sin forma de saber cuál de los quince falló.

use std::path::Path;

use serde_json::{json, Value};

use crate::complementos::Aporte;
use crate::layout::{Firmware, ParticionPlaneada};
use crate::protocol::PlanInstalacion;

/// Nombre del repositorio propio en el `pacman.conf` del sistema instalado.
const REPO_NOMBRE: &str = "vasakos";

/// El espejo del repositorio.
///
/// Va acá y no en `paquetes.txt` porque no es un dato editable: si el repo se
/// muda, el instalador viejo tiene que seguir sabiendo bajar `vasakos-mirrorlist`,
/// que es el paquete que después toma el relevo con la lista real de espejos.
///
/// **El orden de los componentes no es el de Arch.** Arch usa
/// `$repo/os/$arch`; el repositorio de VasakOS sirve `repo/$arch/$repo`, que es
/// lo que dice `vasakos-mirrorlist` y lo que publica `repository-script`. Con el
/// orden de Arch, `pacstrap` recibe un 404 en cada paquete `vasak-*` y la
/// instalación muere en el paso del escritorio — **después** de haber formateado
/// el disco. Hay un test que compara esta cadena con la del paquete de espejos.
const REPO_URL: &str = "https://repo.vasak.net.ar/repo/$arch/$repo";

/// Servicios que se habilitan en el sistema instalado.
///
/// Son de **sistema**. Los de usuario (el llavero, el agente de polkit, el
/// daemon de notificaciones) los habilitan sus propios paquetes con enlaces a
/// `graphical-session.target.wants`, así que no van acá — y ponerlos los haría
/// fallar, porque `systemctl enable` sin `--user` no los encuentra.
const SERVICIOS: &[&str] = &[
    // El display manager. Sin esto el sistema arranca a una consola de texto y
    // el escritorio no aparece nunca: es el único servicio del que depende que
    // la instalación parezca haber funcionado.
    "greetd",
    "NetworkManager",
    "bluetooth",
    // Hora por red. `timedatectl set-ntp` en el chroot no alcanza: escribe el
    // estado pero el servicio tiene que quedar habilitado para el arranque.
    "systemd-timesyncd",
    "avahi-daemon",
];

/// El kernel. Uno solo, el mismo que trae la ISO.
const KERNELS: &[&str] = &["linux"];

/// Lee la lista de paquetes del archivo de datos.
///
/// Se resuelve en tiempo de ejecución y no se compila: sumar un paquete al
/// escritorio no puede obligar a recompilar el instalador.
///
/// El formato es deliberadamente tonto —un paquete por línea, `#` comenta— para
/// que el archivo se pueda leer y diferenciar con herramientas de texto contra
/// `packages.x86_64` de la ISO, que tiene la misma forma.
///
/// Desde el metapaquete `vasakos-desktop` la lista es corta: ese paquete
/// arrastra el escritorio entero por dependencia, y acá sólo queda él más el
/// kernel. El parseo sigue existiendo igual porque el archivo tiene que poder
/// crecer sin recompilar nada.
pub fn leer_paquetes(contenido: &str) -> Vec<String> {
    contenido
        .lines()
        .map(|linea| match linea.find('#') {
            Some(i) => &linea[..i],
            None => linea,
        })
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Dónde busca el archivo de paquetes, en orden.
///
/// El árbol de desarrollo primero para que `tauri dev` funcione sin instalar
/// nada, y la ruta del paquete después. Es el mismo patrón que usa `locales.rs`
/// y por la misma razón: un binario en `/usr/bin` no tiene ninguna ruta
/// relativa útil.
pub fn ruta_paquetes() -> Option<std::path::PathBuf> {
    let candidatas = [
        std::path::PathBuf::from("src-tauri/paquetes.txt"),
        std::path::PathBuf::from("paquetes.txt"),
        std::path::PathBuf::from("/usr/share/vasak-installer/paquetes.txt"),
    ];
    candidatas.into_iter().find(|c| c.is_file())
}

/// El tamaño como lo quiere archinstall: valor, unidad y el sector del disco.
///
/// El sector va adentro de cada tamaño y no una vez por disco porque así lo
/// definió archinstall. En un disco 4Kn son 4096 y no 512, y pasarle 512 le hace
/// calcular ocho veces menos espacio del que hay.
fn tamano(valor_mib: u64, sector_logico: u64) -> Value {
    json!({
        "value": valor_mib,
        "unit": "MiB",
        "sector_size": { "value": sector_logico, "unit": "B" }
    })
}

/// Convierte una partición del plan al JSON de archinstall.
fn particion(p: &ParticionPlaneada, indice: usize, sector_logico: u64) -> Value {
    json!({
        // El `obj_id` sólo tiene que ser único dentro del archivo: archinstall
        // lo usa para referenciar particiones entre secciones (el volumen LUKS
        // apunta a la suya por id). Un contador estable es preferible a un UUID
        // aleatorio porque hace que dos ejecuciones con el mismo plan produzcan
        // el mismo archivo, y eso es lo que permite compararlos cuando algo
        // falla.
        "obj_id": format!("vsk-part-{indice}"),
        "status": "create",
        "type": "primary",
        "start": tamano(p.inicio_mib, sector_logico),
        "size": tamano(p.tamano_mib, sector_logico),
        "fs_type": p.sistema_archivos,
        "mountpoint": p.punto_montaje,
        "mount_options": p.opciones_montaje,
        "flags": p.banderas,
        // `null` y no la ruta: la partición todavía no existe, y archinstall lo
        // completa cuando la crea. Adivinar `/dev/sda1` acá se rompe en NVMe,
        // donde es `/dev/nvme0n1p1`.
        "dev_path": null,
        "btrfs": p.subvolumenes
            .iter()
            .map(|(nombre, punto)| json!({ "name": nombre, "mountpoint": punto }))
            .collect::<Vec<_>>(),
    })
}

/// Arma `user_configuration.json`.
///
/// `version` viene de preguntarle a archinstall instalado, no de una constante:
/// si no coincide con la suya, archinstall avisa que el archivo es de otra
/// versión, y una constante quedaría vieja en la primera actualización.
pub fn configuracion(
    plan: &PlanInstalacion,
    particiones: &[ParticionPlaneada],
    sector_logico: u64,
    firmware: Firmware,
    paquetes: &[String],
    aporte: &Aporte,
    version_archinstall: Option<&str>,
) -> Value {
    // Los complementos se funden acá y no en `paquetes.txt`: lo de ahí es el
    // escritorio, que es siempre el mismo; esto es lo que eligió esta persona en
    // esta instalación.
    //
    // Sin duplicados y en orden estable. `pacman` no se queja de un paquete
    // repetido, pero dos instalaciones con la misma elección tienen que producir
    // el mismo archivo para poder compararlos cuando algo falla.
    let paquetes_finales: Vec<String> = {
        let mut todos: std::collections::BTreeSet<String> = paquetes.iter().cloned().collect();
        todos.extend(aporte.paquetes.iter().cloned());
        todos.into_iter().collect()
    };
    let servicios_finales: Vec<String> = {
        let mut todos: std::collections::BTreeSet<String> =
            SERVICIOS.iter().map(|s| s.to_string()).collect();
        todos.extend(aporte.servicios.iter().cloned());
        todos.into_iter().collect()
    };
    let cifrado = particiones.iter().any(|p| p.cifrada);

    let mut disk_config = json!({
        // `default_layout` y no `manual_partitioning` aunque las particiones las
        // demos nosotras: la diferencia es que `manual_partitioning` espera que
        // las particiones **ya existan** en el disco, y las nuestras hay que
        // crearlas. El `status: "create"` de cada una es lo que lo decide.
        "config_type": "default_layout",
        "device_modifications": [{
            "device": plan.disco,
            // Borra la tabla anterior. Es el punto sin retorno, y está en una
            // sola clave a propósito: el día que haya un modo «usar una
            // partición existente», es esto lo que cambia a `false`.
            "wipe": true,
            "partitions": particiones
                .iter()
                .enumerate()
                .map(|(i, p)| particion(p, i, sector_logico))
                .collect::<Vec<_>>(),
        }],
    });

    if cifrado {
        // La frase **no** va acá: va en el archivo de credenciales. Acá va sólo
        // qué se cifra.
        let ids: Vec<Value> = particiones
            .iter()
            .enumerate()
            .filter(|(_, p)| p.cifrada)
            .map(|(i, _)| json!(format!("vsk-part-{i}")))
            .collect();
        disk_config["disk_encryption"] = json!({
            "encryption_type": "luks",
            "partitions": ids,
        });
    }

    json!({
        // El idioma de **los menús de archinstall**, que no vamos a ver porque
        // corre en silencio. Se deja en inglés para que sus mensajes de error en
        // el registro sean los que aparecen en su documentación y en las
        // búsquedas: un error traducido al español es un error que no se puede
        // buscar.
        "archinstall-language": "English",
        "version": version_archinstall,
        "script": "guided",
        "silent": true,
        "debug": false,

        "hostname": plan.hostname,
        "kernels": KERNELS,
        "timezone": plan.zona_horaria,
        "ntp": plan.ntp,
        // No es una instalación offline: el paso de red de la interfaz no deja
        // seguir sin conexión, justamente porque `pacstrap` baja todo.
        "offline": false,
        // Lo que hace es saltear la espera de reflector, y la ISO no lo corre.
        "no_pkg_lookups": false,

        "locale_config": {
            "sys_lang": plan.idioma_sistema,
            // UTF-8 y nada más. Ofrecer otra codificación en 2026 es ofrecer
            // que los nombres de archivo con acentos se rompan.
            "sys_enc": "UTF-8",
            "kb_layout": plan.teclado,
        },

        "disk_config": disk_config,

        "bootloader_config": {
            // GRUB y no systemd-boot, que es el que trae archinstall por
            // defecto: GRUB es el que venía usando la configuración de
            // calamares, es el único de los dos que arranca en BIOS, y es el
            // que detecta otros sistemas con os-prober.
            "bootloader": "Grub",
            "uki": false,
            "removable": false,
        },

        "mirror_config": {
            // El repositorio de VasakOS. Sin esto `pacstrap` no encuentra
            // ninguno de los paquetes `vasak-*` y la instalación muere en el
            // paso del escritorio, después de haber formateado el disco.
            "custom_repositories": [{
                "name": REPO_NOMBRE,
                "url": REPO_URL,
                // Firma obligatoria, y la clave viene de `vasakos-keyring`, que
                // está en la lista de paquetes. `TrustAll` es lo que permite
                // usar la clave del llavero sin que nadie la firme a mano
                // primero: sin eso, la primera instalación no puede validar
                // nada y hay que desactivar la comprobación entera, que es
                // mucho peor.
                "sign_check": "Required",
                "sign_option": "TrustAll",
            }],
            "optional_repositories": [],
        },

        // NetworkManager, que es el que usa el escritorio: `vasak-settings` y el
        // panel hablan con él por D-Bus. Las otras opciones de archinstall
        // (`iwd`, `manual`) dejarían el escritorio sin poder cambiar de red.
        //
        // No es `iso`: copiar la configuración del medio live arrastraría las
        // conexiones que se crearon para instalar, con sus contraseñas, a
        // /etc/NetworkManager del sistema nuevo. Que la red haya que reconectarla
        // una vez es preferible.
        "network_config": { "type": "nm" },

        "packages": paquetes_finales,
        "services": servicios_finales,
        "custom_commands": [],

        // zram y no partición de intercambio. En una máquina con poca memoria
        // rinde más que un swap en disco, y sobre todo no le come disco a la
        // raíz ni obliga a decidir cuánto darle antes de saber cómo se va a usar
        // el equipo.
        "swap": { "enabled": plan.zram, "algorithm": "zstd" },

        "pacman_config": {
            "color": false,
            // Cinco descargas en paralelo. Es el número que usa archinstall y
            // el que trae Arch: subirlo no acelera nada en una conexión normal y
            // vuelve ilegible la salida que parseamos para el progreso.
            "parallel_downloads": 5,
        },

        // Sin perfil de archinstall. Sus perfiles instalan un escritorio de
        // terceros con su greeter y sus drivers, y VasakOS es su propio
        // escritorio: lo que haría un perfil lo hacen `packages` y `services` de
        // acá arriba, más el plugin.
        "profile_config": null,

        // Anotación nuestra, que archinstall ignora. Queda en el archivo que se
        // conserva en el sistema instalado, y es lo que permite saber después
        // con qué firmware se instaló sin tener que deducirlo de las
        // particiones.
        "_vasakos": {
            "firmware": match firmware { Firmware::Uefi => "uefi", Firmware::Bios => "bios" },
            "sistema_archivos": plan.sistema_archivos.como_archinstall(),
            // Queda en el archivo que se conserva en el sistema instalado: es la
            // única forma de saber después qué se eligió al instalar sin
            // deducirlo de la lista de paquetes.
            "complementos": plan.complementos,
        },
    })
}

/// Los grupos del usuario administrador.
///
/// Son los mismos que ponía la configuración de calamares. Cada uno da acceso a
/// una clase de dispositivo, y sacar uno se nota mucho después: sin `video` el
/// brillo no se cambia, sin `storage` no se montan discos externos, sin `uucp`
/// no se abre un puerto serie.
const GRUPOS_ADMIN: &[&str] = &[
    "wheel", "adm", "audio", "video", "network", "storage", "power", "lp", "optical", "scanner",
    "rfkill", "uucp", "sys", "users",
];

/// Los mismos menos `wheel`: es `wheel` lo que da sudo.
const GRUPOS_USUARIO: &[&str] = &[
    "adm", "audio", "video", "network", "storage", "power", "lp", "optical", "scanner", "rfkill",
    "uucp", "sys", "users",
];

/// Arma `user_credentials.json`.
///
/// Las contraseñas van **como hash**, no en claro: archinstall acepta las dos
/// formas (`enc_password` y el obsoleto `!password`), y la diferencia es que el
/// archivo en claro queda en el disco del medio live mientras dura la
/// instalación y aparece entero en cualquier volcado de ese archivo.
///
/// El hash lo produce quien llama, con `openssl passwd -6` y la contraseña por
/// entrada estándar. Acá sólo se acomoda.
pub fn credenciales(
    plan: &PlanInstalacion,
    hash_usuario: &str,
    hash_root: Option<&str>,
    frase_cifrado: Option<&str>,
) -> Value {
    let grupos: &[&str] = if plan.administrador {
        GRUPOS_ADMIN
    } else {
        GRUPOS_USUARIO
    };

    let mut creds = json!({
        "users": [{
            "username": plan.usuario,
            "enc_password": hash_usuario,
            "sudo": plan.administrador,
            "groups": grupos,
        }],
    });

    // `null` explícito y no ausente cuando root va deshabilitado: archinstall
    // distingue «no me dieron contraseña de root» de «no hay clave», y con la
    // clave ausente en algunas versiones deja la cuenta con contraseña vacía.
    creds["root_enc_password"] = match hash_root {
        Some(h) => json!(h),
        None => Value::Null,
    };

    if let Some(frase) = frase_cifrado {
        creds["encryption_password"] = json!(frase);
    }

    creds
}

/// El nombre de archivo del plugin, dentro del directorio de datos.
const PLUGIN: &str = "vasakos.py";

/// Dónde está el plugin, con el mismo orden de búsqueda que los paquetes.
pub fn ruta_plugin() -> Option<std::path::PathBuf> {
    let candidatas = [
        std::path::PathBuf::from("src-tauri/plugin").join(PLUGIN),
        std::path::PathBuf::from("plugin").join(PLUGIN),
        Path::new("/usr/share/vasak-installer/plugin").join(PLUGIN),
    ];
    candidatas.into_iter().find(|c| c.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{planificar, Disco, Firmware};
    use crate::protocol::{EsquemaDisco, Secretos, SistemaArchivos};

    fn disco() -> Disco {
        Disco {
            ruta: "/dev/nvme0n1".into(),
            modelo: "Prueba".into(),
            tamano_bytes: 256 * 1024 * 1024 * 1024,
            sector_logico: 512,
            rotacional: false,
            nvme: true,
            en_uso: false,
            particiones: Vec::new(),
        }
    }

    fn plan(cifrar: bool) -> PlanInstalacion {
        PlanInstalacion {
            disco: "/dev/nvme0n1".into(),
            esquema: EsquemaDisco::BorrarTodo,
            sistema_archivos: SistemaArchivos::Btrfs,
            cifrar,
            zram: true,
            zona_horaria: "America/Argentina/Buenos_Aires".into(),
            idioma_sistema: "es_AR".into(),
            teclado: "la-latin1".into(),
            ntp: true,
            hostname: "vasak".into(),
            nombre_completo: "Persona de Prueba".into(),
            usuario: "persona".into(),
            administrador: true,
            root_habilitado: false,
            complementos: vec!["firefox".into(), "impresoras".into()],
            secretos: Secretos {
                usuario: "clave-del-usuario".into(),
                root: String::new(),
                cifrado: if cifrar { "frase-del-disco".into() } else { String::new() },
            },
        }
    }

    fn config(cifrar: bool) -> Value {
        let d = disco();
        let particiones = planificar(&d, Firmware::Uefi, SistemaArchivos::Btrfs, cifrar).unwrap();
        configuracion(
            &plan(cifrar),
            &particiones,
            d.sector_logico,
            Firmware::Uefi,
            &["base".to_string(), "vasakos-desktop".to_string()],
            &Aporte {
                paquetes: vec!["cups".into(), "firefox".into()],
                servicios: vec!["cups.socket".into()],
            },
            Some("4.4.0"),
        )
    }

    #[test]
    fn el_comentario_no_se_cuela_como_paquete() {
        let texto = "\
# un comentario
base
   linux
vasak-desktop # con comentario al final

# otro
zsh";
        assert_eq!(
            leer_paquetes(texto),
            vec!["base", "linux", "vasak-desktop", "zsh"]
        );
    }

    /// Un `#` a mitad de línea comenta el resto. Si no se cortara ahí, el
    /// paquete se llamaría `vasak-desktop # con comentario` y `pacstrap`
    /// fallaría con «target not found» nombrando toda la línea.
    #[test]
    fn el_archivo_real_de_paquetes_parsea() {
        let contenido = include_str!("../paquetes.txt");
        let paquetes = leer_paquetes(contenido);

        // Ninguno con espacios ni con `#`: eso sería el parseo dejando pasar
        // basura que pacman rechaza. Importa más que antes, porque ahora el
        // archivo es casi todo comentario.
        for p in &paquetes {
            assert!(!p.contains(' '), "«{p}» tiene un espacio");
            assert!(!p.contains('#'), "«{p}» tiene un numeral");
        }
        // Lo que no puede faltar. `vasakos-desktop` es el escritorio entero:
        // arrastra por dependencia `vasak-desktop`, `vasak-session-manager`,
        // `greetd`, `wayfire`, `networkmanager`, `grub` y los llaveros, que es
        // lo que esta prueba enumeraba de a uno cuando la lista estaba escrita
        // acá. Sin él la instalación termina en una consola de texto.
        //
        // Que ese metapaquete tenga las dependencias correctas es asunto de su
        // PKGBUILD, que vive en otro repositorio: nombrarlas de nuevo desde acá
        // sería reinventar las dos fuentes de verdad que el metapaquete vino a
        // eliminar.
        for imprescindible in ["vasakos-desktop", "linux", "linux-headers"] {
            assert!(
                paquetes.iter().any(|p| p == imprescindible),
                "falta {imprescindible} en paquetes.txt"
            );
        }
        // Y lo que no puede estar: lo que sólo sirve en el medio live, que se
        // queda en `packages.x86_64`. El instalador anterior se removía a sí
        // mismo del sistema instalado, y calamares no tiene por qué viajar.
        for sobra in [
            "vasakos-calamares",
            "vasakos-calamares-config",
            "mkinitcpio-archiso",
            "vasak-installer",
            "archinstall",
            "memtest86+",
            "syslinux",
        ] {
            assert!(
                !paquetes.iter().any(|p| p == sobra),
                "{sobra} no tendría que estar en el sistema instalado"
            );
        }
        // El archivo se redujo al metapaquete: si vuelve a tener decenas de
        // entradas es que alguien copió la lista de la ISO de vuelta acá, que
        // es exactamente la divergencia que se sacó de encima.
        assert!(
            paquetes.len() < 10,
            "salieron {} paquetes; el escritorio va en las depends de              vasakos-desktop, no acá",
            paquetes.len()
        );
    }

    /// Lo que mató la instalación en BIOS: una partición con `fs_type` en `null`.
    ///
    /// `_setup_partition` de archinstall pide `safe_fs_type` para **todas** las
    /// que crea, y esa propiedad lanza `ValueError('File system type is not set')`
    /// si el valor no está. Pasaba con la `bios_grub`, que ya no se arma, pero la
    /// prueba mira el JSON —que es lo que archinstall lee— y no el plan.
    #[test]
    fn ninguna_particion_del_json_va_sin_fs_type() {
        let d = disco();
        for firmware in [Firmware::Uefi, Firmware::Bios] {
            for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
                let particiones = planificar(&d, firmware, fs, false).unwrap();
                let c = configuracion(
                    &plan(false),
                    &particiones,
                    d.sector_logico,
                    firmware,
                    &["base".to_string()],
                    &Aporte::default(),
                    Some("4.4.0"),
                );
                let del_json = c["disk_config"]["device_modifications"][0]["partitions"]
                    .as_array()
                    .expect("hay particiones");
                assert!(!del_json.is_empty());
                for p in del_json {
                    assert!(
                        p["fs_type"].is_string(),
                        "fs_type = {} con {firmware:?}/{fs:?}",
                        p["fs_type"]
                    );
                }
            }
        }
    }

    /// **El JSON tiene que llevar una partición que archinstall reconozca como
    /// arranque, en los dos firmwares.**
    ///
    /// Es el mismo control que en `layout`, pero sobre lo que de verdad se le
    /// entrega a archinstall: entre el plan y el JSON hay una traducción, y un
    /// error ahí da el mismo `Could not detect boot at mountpoint /mnt` con el
    /// plan perfectamente bien.
    #[test]
    fn el_json_siempre_lleva_una_particion_de_arranque() {
        for firmware in [Firmware::Uefi, Firmware::Bios] {
            for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
                let d = disco();
                let particiones = planificar(&d, firmware, fs, false).unwrap();
                let c = configuracion(
                    &plan(false),
                    &particiones,
                    d.sector_logico,
                    firmware,
                    &["base".to_string()],
                    &Aporte::default(),
                    Some("4.4.0"),
                );
                let del_json = c["disk_config"]["device_modifications"][0]["partitions"]
                    .as_array()
                    .unwrap();

                // Lo que hace `get_boot_partition`: bandera `boot` y punto de
                // montaje, las dos cosas en la misma partición.
                let arranque = del_json
                    .iter()
                    .find(|p| {
                        let con_bandera = p["flags"]
                            .as_array()
                            .map(|f| f.iter().any(|x| x == "boot"))
                            .unwrap_or(false);
                        con_bandera && !p["mountpoint"].is_null()
                    })
                    .unwrap_or_else(|| {
                        panic!("{firmware:?}/{fs:?}: el JSON no lleva partición de arranque: {del_json:#?}")
                    });

                assert_eq!(arranque["mountpoint"], "/boot", "{firmware:?}/{fs:?}");
                assert!(
                    !arranque["fs_type"].is_null(),
                    "{firmware:?}/{fs:?}: sin fs_type archinstall muere en safe_fs_type"
                );
            }
        }
    }

    #[test]
    fn en_bios_el_json_no_marca_ninguna_esp() {
        // `esp` en un disco que arranca por BIOS marca una partición de sistema
        // EFI que nadie va a leer.
        let d = disco();
        let particiones = planificar(&d, Firmware::Bios, SistemaArchivos::Btrfs, false).unwrap();
        let c = configuracion(
            &plan(false),
            &particiones,
            d.sector_logico,
            Firmware::Bios,
            &["base".to_string()],
            &Aporte::default(),
            Some("4.4.0"),
        );
        let del_json = c["disk_config"]["device_modifications"][0]["partitions"]
            .as_array()
            .unwrap();

        for p in del_json {
            if let Some(banderas) = p["flags"].as_array() {
                assert!(
                    !banderas.iter().any(|x| x == "esp"),
                    "en BIOS quedó una bandera esp: {p:#?}"
                );
            }
        }
    }

    #[test]
    fn el_repositorio_de_vasakos_va_en_la_configuracion() {
        let c = config(false);
        let repos = &c["mirror_config"]["custom_repositories"];
        assert_eq!(repos[0]["name"], REPO_NOMBRE);
        assert_eq!(repos[0]["sign_check"], "Required");
        // Sin el repo, `pacstrap` no encuentra ningún `vasak-*` y la
        // instalación muere **después** de formatear el disco.
        assert!(repos[0]["url"].as_str().unwrap().contains("$repo"));
    }

    /// La URL tiene el orden de componentes de VasakOS y no el de Arch.
    ///
    /// Arch sirve `$repo/os/$arch`; VasakOS sirve `repo/$arch/$repo`. Escribir
    /// el de Arch —que es el que uno recuerda— produce un 404 en cada paquete
    /// `vasak-*`, y eso se descubre a los veinte minutos de instalación, con el
    /// disco ya formateado. El test fija el orden exacto.
    #[test]
    fn la_url_del_repositorio_tiene_el_orden_de_vasakos() {
        assert_eq!(REPO_URL, "https://repo.vasak.net.ar/repo/$arch/$repo");
        // El error concreto que se cometió: la convención de Arch.
        assert!(
            !REPO_URL.contains("/os/"),
            "«/os/» es la convención de Arch, no la de VasakOS"
        );
        // `$arch` va antes que `$repo`.
        let arch = REPO_URL.find("$arch").expect("falta $arch");
        let repo = REPO_URL.rfind("$repo").expect("falta $repo");
        assert!(arch < repo, "$arch tiene que ir antes que $repo: {REPO_URL}");
    }

    #[test]
    fn la_raiz_btrfs_va_sin_punto_de_montaje_y_con_subvolumenes() {
        let c = config(false);
        let particiones = c["disk_config"]["device_modifications"][0]["partitions"]
            .as_array()
            .unwrap();
        assert_eq!(particiones.len(), 2);

        let esp = &particiones[0];
        assert_eq!(esp["mountpoint"], "/boot");
        assert_eq!(esp["fs_type"], "fat32");

        let raiz = &particiones[1];
        assert!(raiz["mountpoint"].is_null(), "{raiz}");
        assert_eq!(raiz["btrfs"].as_array().unwrap().len(), 7);
        assert_eq!(raiz["btrfs"][0]["name"], "@");
        assert_eq!(raiz["btrfs"][0]["mountpoint"], "/");
    }

    #[test]
    fn los_tamanos_llevan_el_sector_del_disco() {
        let mut d = disco();
        d.sector_logico = 4096; // un disco 4Kn
        let particiones = planificar(&d, Firmware::Uefi, SistemaArchivos::Ext4, false).unwrap();
        let c = configuracion(
            &plan(false),
            &particiones,
            d.sector_logico,
            Firmware::Uefi,
            &[],
            &Aporte::default(),
            None,
        );
        let esp = &c["disk_config"]["device_modifications"][0]["partitions"][0];
        // Con 512 fijo, archinstall calcularía ocho veces menos espacio del que
        // hay y la última partición no entraría.
        assert_eq!(esp["size"]["sector_size"]["value"], 4096);
        assert_eq!(esp["start"]["sector_size"]["value"], 4096);
        assert_eq!(esp["size"]["unit"], "MiB");
    }

    #[test]
    fn sin_cifrado_no_aparece_la_seccion_de_cifrado() {
        let c = config(false);
        assert!(c["disk_config"].get("disk_encryption").is_none(), "{c}");
    }

    #[test]
    fn con_cifrado_se_apunta_a_la_raiz_y_no_al_esp() {
        let c = config(true);
        let cifrado = &c["disk_config"]["disk_encryption"];
        assert_eq!(cifrado["encryption_type"], "luks");
        let ids = cifrado["partitions"].as_array().unwrap();
        // Sólo una, y es la segunda: cifrar el ESP produce un equipo que no
        // arranca porque el firmware lo lee antes de que exista quien descifre.
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "vsk-part-1");
    }

    #[test]
    fn el_json_no_lleva_ninguna_contrasena() {
        let c = config(true);
        let texto = c.to_string();
        // La configuración se conserva en el sistema instalado. Una contraseña
        // acá quedaría en el disco para siempre.
        assert!(!texto.contains("clave-del-usuario"), "{texto}");
        assert!(!texto.contains("frase-del-disco"), "{texto}");
    }

    #[test]
    fn las_credenciales_llevan_el_hash_y_no_la_contrasena() {
        let p = plan(true);
        let creds = credenciales(&p, "$6$sal$hash", None, Some("frase-del-disco"));
        let texto = creds.to_string();

        assert!(!texto.contains("clave-del-usuario"), "{texto}");
        assert_eq!(creds["users"][0]["enc_password"], "$6$sal$hash");
        assert_eq!(creds["users"][0]["username"], "persona");
        assert_eq!(creds["users"][0]["sudo"], true);
        // La frase de LUKS sí va en claro: cryptsetup necesita la frase, no un
        // hash. Por eso este archivo se borra al terminar.
        assert_eq!(creds["encryption_password"], "frase-del-disco");
    }

    #[test]
    fn root_deshabilitado_deja_la_clave_en_null_explicito() {
        let creds = credenciales(&plan(false), "$6$x$y", None, None);
        // Presente y `null`, no ausente: con la clave ausente algunas versiones
        // de archinstall dejan root con contraseña vacía, que es peor que
        // bloqueada.
        assert!(creds.get("root_enc_password").is_some());
        assert!(creds["root_enc_password"].is_null());
    }

    #[test]
    fn un_usuario_sin_privilegios_no_va_a_wheel() {
        let mut p = plan(false);
        p.administrador = false;
        let creds = credenciales(&p, "$6$x$y", None, None);
        let grupos: Vec<String> = creds["users"][0]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g.as_str().unwrap().to_string())
            .collect();
        assert!(!grupos.contains(&"wheel".to_string()), "{grupos:?}");
        assert_eq!(creds["users"][0]["sudo"], false);
        // Pero sí a los de dispositivos: sin `video` no puede cambiar el brillo
        // de su propia pantalla.
        assert!(grupos.contains(&"video".to_string()));
        assert!(grupos.contains(&"audio".to_string()));
    }

    /// Los paquetes de los complementos se suman a los del escritorio.
    #[test]
    fn los_complementos_se_funden_en_la_lista_de_paquetes() {
        let c = config(false);
        let paquetes: Vec<&str> = c["packages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();

        assert!(paquetes.contains(&"vasakos-desktop"), "{paquetes:?}");
        assert!(paquetes.contains(&"firefox"), "{paquetes:?}");
        assert!(paquetes.contains(&"cups"), "{paquetes:?}");

        // Ordenados y sin repetidos: dos instalaciones con la misma elección
        // tienen que producir el mismo archivo para poder compararlos.
        let mut ordenados = paquetes.clone();
        ordenados.sort_unstable();
        assert_eq!(paquetes, ordenados, "los paquetes no salen ordenados");
        ordenados.dedup();
        assert_eq!(paquetes.len(), ordenados.len(), "hay paquetes repetidos");
    }

    /// Los servicios de un complemento se suman a los fijos, sin pisarlos.
    ///
    /// `greetd` es el único del que depende que la instalación parezca haber
    /// funcionado: si la fusión lo reemplazara en vez de sumarse, el equipo
    /// arrancaría a una consola de texto.
    #[test]
    fn los_servicios_de_los_complementos_no_pisan_los_del_sistema() {
        let c = config(false);
        let servicios: Vec<&str> = c["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();

        assert!(servicios.contains(&"greetd"), "{servicios:?}");
        assert!(servicios.contains(&"NetworkManager"), "{servicios:?}");
        assert!(servicios.contains(&"cups.socket"), "{servicios:?}");
    }

    /// Sin complementos, la configuración es la de antes de que existieran.
    #[test]
    fn sin_complementos_no_se_suma_nada() {
        let d = disco();
        let particiones = planificar(&d, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let mut p = plan(false);
        p.complementos.clear();
        let c = configuracion(
            &p,
            &particiones,
            d.sector_logico,
            Firmware::Uefi,
            &["base".to_string()],
            &Aporte::default(),
            None,
        );
        assert_eq!(c["packages"].as_array().unwrap().len(), 1);
        assert_eq!(
            c["services"].as_array().unwrap().len(),
            SERVICIOS.len(),
            "sin complementos los servicios son sólo los fijos"
        );
    }

    #[test]
    fn no_hay_perfil_de_archinstall() {
        // Un perfil instalaría un escritorio de terceros con su propio greeter,
        // que competiría con greetd por el arranque gráfico.
        assert!(config(false)["profile_config"].is_null());
    }

    #[test]
    fn greetd_esta_entre_los_servicios() {
        let c = config(false);
        let servicios: Vec<&str> = c["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        // Es el único servicio del que depende que la instalación *parezca*
        // haber funcionado: sin él el equipo arranca a una consola de texto.
        assert!(servicios.contains(&"greetd"), "{servicios:?}");
        assert!(servicios.contains(&"NetworkManager"), "{servicios:?}");
    }
}
