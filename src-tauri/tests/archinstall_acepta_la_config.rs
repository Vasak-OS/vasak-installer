//! Que archinstall acepte lo que le mandamos, comprobado **contra archinstall**.
//!
//! El resto de las pruebas del particionado dicen lo que nosotros creemos que
//! archinstall espera. Ésta se lo pregunta a él: construye la configuración y la
//! pasa por sus propios modelos, los de `archinstall/lib/models/device.py`.
//!
//! Es la prueba que faltaba cuando una instalación en BIOS murió con
//! «ValueError: File system type is not set». Nuestros tests estaban en verde:
//! codificaban la idea equivocada de que una partición podía no tener sistema de
//! archivos. Los de archinstall no opinan, ejecutan.
//!
//! No toca ningún disco ni necesita root: sólo construye objetos en memoria.
//! `DiskLayoutConfiguration.parse_arg` no sirve para esto —busca el disco con
//! `device_handler.get_device`, y enumerarlos sí pide root—, así que se repiten
//! sus mismas líneas sobre cada partición.
//!
//! Si archinstall no se puede importar, la prueba se saltea **diciendo por qué**:
//! es la que corre en la ISO, no necesariamente en la máquina de quien
//! desarrolla. Y se saltea igual si está instalado pero su cadena de imports está
//! rota —pasó con `pydantic` y `pydantic-core` desparejos—, porque la pregunta
//! que esta prueba hace no se puede contestar en una máquina así.
//!
//! El salteo dice el motivo a propósito: callado escondería que la configuración
//! del particionado no se validó, que es justo lo que esto existe para no dejar
//! pasar.

use std::process::Command;

use vasak_installer_lib::archconfig::{configuracion, FuentesDePaquetes};
use vasak_installer_lib::complementos::Aporte;
use vasak_installer_lib::layout::{
    planificar_borrando, planificar_junto_a, planificar_manual, planificar_sobre, Disco,
    Firmware, ParticionExistente,
};
use vasak_installer_lib::protocol::{
    AsignacionManual, EsquemaDisco, PlanInstalacion, Secretos, SistemaArchivos,
};

/// Lo que `parse_arg` hace con cada partición, y lo que después le pide.
const COMPROBACION: &str = r#"
import json, sys
from pathlib import Path
from archinstall.lib.models.device import (
    BtrfsOptions, FilesystemType, ModificationStatus, PartitionFlag,
    PartitionModification, PartitionType, Size, SubvolumeModification,
)

problemas = []
cfg = json.loads(sys.stdin.read())
modificaciones = []
for partition in cfg["disk_config"]["device_modifications"][0]["partitions"]:
    nombre = partition["obj_id"]
    # `from_string` devuelve None para lo que archinstall no conoce, y `parse_arg`
    # lo descarta sin decir nada: una bandera que no sobrevive es una bandera que
    # no se aplica.
    descartadas = [f for f in partition.get("flags", []) if PartitionFlag.from_string(f) is None]
    if descartadas:
        problemas.append(f"{nombre}: archinstall descarta las banderas {descartadas}")

    flags = [g for f in partition.get("flags", []) if (g := PartitionFlag.from_string(f))]
    fs_type = FilesystemType(partition["fs_type"]) if partition.get("fs_type") else None
    # `ModificationStatus` es un StrEnum: un estado que no conozca revienta acá
    # con ValueError, que es exactamente lo que se quiere comprobar.
    estado = ModificationStatus(partition["status"])
    mod = PartitionModification(
        status=estado,
        fs_type=fs_type,
        start=Size.parse_args(partition["start"]),
        length=Size.parse_args(partition["size"]),
        mount_options=partition["mount_options"],
        mountpoint=Path(partition["mountpoint"]) if partition["mountpoint"] else None,
        # Del JSON y no fijo en None: en las que se crean viene nulo, y en las
        # que ya existen es obligatorio. Fijarlo acá haría que el arnés probara
        # algo distinto de lo que se manda.
        dev_path=Path(partition["dev_path"]) if partition.get("dev_path") else None,
        type=PartitionType(partition["type"]),
        flags=flags,
        btrfs_subvols=SubvolumeModification.parse_args(partition.get("btrfs", [])),
    )
    # Una partición que ya existe **tiene** que traer su ruta: sin ella
    # archinstall no sabe cuál del disco es, y `models/device.py:904` lo rechaza
    # con «Device path must be set». Es el error que aparecería recién al
    # reusar el ESP de otro sistema, o sea en el equipo de alguien.
    if mod.is_exists_or_modify() and not partition.get("dev_path"):
        problemas.append(f"{nombre}: es «{estado.value}» y va sin dev_path")
    # Y una que se crea no puede traerla: todavía no existe, y una ruta
    # adivinada apuntaría a la partición de otro.
    if estado == ModificationStatus.CREATE and partition.get("dev_path"):
        problemas.append(f"{nombre}: se crea y ya trae dev_path {partition['dev_path']}")
    try:
        # Lo que `_setup_partition` pide para toda partición que crea.
        mod.safe_fs_type
    except ValueError as e:
        problemas.append(f"{nombre}: {e}")

    if not mod.start.is_valid_start():
        problemas.append(f"{nombre}: empieza en un lugar que archinstall rechaza")

    modificaciones.append(mod)

# Las instantáneas.
#
# `setup_btrfs_snapshot` —el que instala snapper y grub-btrfs, que es lo que
# pone las instantáneas en el menú de arranque— corre sólo si se dan **las dos**
# cosas: que el JSON traiga `btrfs_options.snapshot_config`, y que
# `has_default_btrfs_vols()` sea cierto.
#
# Lo primero se comprueba acá contra el propio `BtrfsOptions.parse_arg`.
btrfs_arg = cfg["disk_config"].get("btrfs_options")
pedimos_instantaneas = False
if btrfs_arg is not None:
    opciones = BtrfsOptions.parse_arg(btrfs_arg)
    if opciones is None or opciones.snapshot_config is None:
        problemas.append(f"btrfs_options: archinstall lo descarta entero: {btrfs_arg}")
    else:
        pedimos_instantaneas = True

# Lo segundo **hoy no se puede dar**, y no por nada nuestro.
#
# `SubvolumeModification.parse_args` guarda el nombre tal como viene del JSON,
# que es un `str`; `is_default_root()` lo compara contra `Path('@')`. Un `str`
# nunca es igual a un `Path`, así que da falso siempre. El menú interactivo no
# lo sufre porque ahí los subvolúmenes se arman con `SubvolumeModification(
# Path('@'), Path('/'))` (disk_menu.py:588). O sea: en cualquier instalación
# guiada por un archivo de configuración —la nuestra— archinstall no arma las
# instantáneas, mande uno lo que mande.
#
# Por eso `vasak-desktop-settings` las arma por su cuenta. La clave se manda
# igual: es correcta, no cuesta nada, y el día que arreglen esto arriba empieza
# a funcionar sola.
#
# Y ese día hay que enterarse, porque entonces sobra nuestro armado y quedarían
# los dos. Esta línea es el aviso: se imprime cuando la puerta se abre.
#
# El cuerpo es el de `has_default_btrfs_vols`, evaluado sobre los objetos de
# archinstall que ya construimos: `DiskLayoutConfiguration` no se puede armar
# sin enumerar discos, que pide root.
archinstall_las_haria = any(
    m.is_create_or_modify()
    and m.fs_type == FilesystemType.BTRFS
    and any(s.is_default_root() for s in m.btrfs_subvols)
    for m in modificaciones
)
if archinstall_las_haria:
    problemas.append(
        "UPSTREAM ARREGLADO: has_default_btrfs_vols ya da verdadero. archinstall "
        "arma snapper y grub-btrfs solo; sacar el armado propio de "
        "vasak-desktop-settings.install antes de que queden los dos."
    )

# Y el contrato al revés: si el plan trae la raíz btrfs con `@` montado en `/`,
# el JSON **tiene** que pedir las instantáneas. Es la mitad que importa —la otra
# comprobación sólo dice que lo que mandamos está bien formado, no que lo
# mandemos cuando corresponde—.
#
# Se mira el JSON crudo y no `is_default_root()` sobre los objetos de
# archinstall, porque eso hoy da falso siempre por el problema de arriba: usarlo
# acá haría que esta comprobación no comprobara nada.
hay_raiz_btrfs_default = any(
    p.get("fs_type") == "btrfs"
    and any(s["name"] == "@" and s["mountpoint"] == "/" for s in p.get("btrfs", []))
    for p in cfg["disk_config"]["device_modifications"][0]["partitions"]
)
if hay_raiz_btrfs_default and not pedimos_instantaneas:
    problemas.append("hay raíz btrfs con @ en / y el JSON no pide instantáneas")
if pedimos_instantaneas and not hay_raiz_btrfs_default:
    problemas.append("el JSON pide instantáneas y no hay raíz btrfs con @ en /")

print("\n".join(problemas))
"#;

fn disco() -> Disco {
    Disco {
        ruta: "/dev/vda".into(),
        modelo: "QEMU HARDDISK".into(),
        tamano_bytes: 50 * 1024 * 1024 * 1024,
        sector_logico: 512,
        rotacional: true,
        nvme: false,
        en_uso: false,
        particiones: Vec::new(),
    }
}

/// El mismo disco, pero con un Windows ya instalado y un hueco libre al final.
///
/// El ESP es de 512 MiB y no de los 100 que hace Windows, porque con 100 el
/// plan falla antes —a propósito— y no habría nada que pasarle a archinstall.
fn disco_con_windows() -> Disco {
    let mib = 1024 * 1024;
    let mut d = disco();
    d.particiones = vec![
        ParticionExistente {
            ruta: "/dev/vda1".into(),
            inicio_bytes: mib,
            tamano_bytes: 512 * mib,
            sistema_archivos: Some("vfat".into()),
            etiqueta: Some("SYSTEM".into()),
            numero: Some(1),
            tipo_particion: Some("c12a7328-f81f-11d2-ba4b-00a0c93ec93b".into()),
            sistema_operativo: None,
        },
        ParticionExistente {
            ruta: "/dev/vda2".into(),
            inicio_bytes: 513 * mib,
            tamano_bytes: 20 * 1024 * mib,
            sistema_archivos: Some("ntfs".into()),
            etiqueta: Some("Windows".into()),
            numero: Some(2),
            tipo_particion: Some("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7".into()),
            sistema_operativo: Some("Windows 11".into()),
        },
    ];
    d
}

fn plan(fs: SistemaArchivos, cifrar: bool) -> PlanInstalacion {
    PlanInstalacion {
        disco: "/dev/vda".into(),
        esquema: EsquemaDisco::BorrarTodo,
        particion_destino: None,
        asignaciones: Vec::new(),
        sistema_archivos: fs,
        cifrar,
        zram: true,
        zona_horaria: "America/Argentina/Buenos_Aires".into(),
        idioma_sistema: "es_AR".into(),
        teclado: "es".into(),
        ntp: true,
        hostname: "vasakos".into(),
        nombre_completo: "Prueba".into(),
        usuario: "prueba".into(),
        administrador: true,
        root_habilitado: false,
        complementos: Vec::new(),
        secretos: Secretos {
            usuario: "x".into(),
            root: String::new(),
            cifrado: if cifrar { "y".into() } else { String::new() },
        },
    }
}

/// El módulo que la comprobación necesita, y que por lo tanto hay que probar.
///
/// **No alcanza con `import archinstall`.** Su `__init__` no trae este módulo,
/// así que importar el paquete a secas puede funcionar mientras esto falla: pasó
/// con un `python-pydantic-core` desparejo respecto de `python-pydantic` —dos
/// repositorios sirviendo versiones distintas— y el resultado fue que el guard
/// dejaba pasar y los cuatro tests reventaban con el traceback de Python en
/// lugar de saltearse. El guard tiene que probar lo mismo que usa la prueba.
const MODULO: &str = "archinstall.lib.models.device";

/// Por qué no se puede correr la comprobación acá, o `None` si sí se puede.
///
/// Devuelve el motivo en lugar de un `bool` para poder decirlo al saltear: un
/// salteo mudo esconde que la configuración no se validó, que es justamente lo
/// que esta prueba existe para no dejar pasar.
fn por_que_no_se_puede() -> Option<String> {
    let salida = Command::new("python3")
        .args(["-c", &format!("import {MODULO}")])
        .output()
        .ok()?;

    if salida.status.success() {
        return None;
    }

    // La última línea del traceback, que es la que dice qué pasó. El resto son
    // los diez marcos de la cadena de imports, que no agregan nada.
    let error = String::from_utf8_lossy(&salida.stderr);
    let motivo = error
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("python3 falló sin decir por qué")
        .trim()
        .to_string();

    Some(motivo)
}

#[test]
fn archinstall_acepta_todas_las_particiones_que_le_mandamos() {
    if let Some(motivo) = por_que_no_se_puede() {
        eprintln!("no se puede comprobar contra archinstall, se saltea: {motivo}");
        return;
    }

    let d = disco();
    for firmware in [Firmware::Uefi, Firmware::Bios] {
        for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
            for cifrar in [false, true] {
                let particiones = planificar_borrando(&d, firmware, fs, cifrar).unwrap();
                let c = configuracion(
                    &plan(fs, cifrar),
                    &particiones,
                    d.sector_logico,
                    firmware,
                    &FuentesDePaquetes {
                        escritorio: &["base".to_string()],
                        aporte: &Aporte::default(),
                        // Sin paquetes de hardware: lo que se prueba acá es que
                        // archinstall acepta el JSON, y los del hardware entran
                        // en la misma lista que los demás.
                        necesarios: &Default::default(),
                    },
                    Some("4.4.0"),
                );

                let salida = ejecutar(&serde_json::to_string(&c).unwrap());
                assert!(
                    salida.is_empty(),
                    "archinstall rechaza la configuración de {firmware:?}/{fs:?} (cifrado: {cifrar}):\n{salida}"
                );
            }
        }
    }
}

/// **Y que acepte el plan que no borra el disco.**
///
/// Es el que estrena `existing` y `dev_path`, o sea el camino que nunca pasó
/// por archinstall. Un rechazo acá aparecería recién al instalar al lado de un
/// Windows, con el ESP ajeno ya en juego.
#[test]
fn archinstall_acepta_el_plan_que_no_borra_el_disco() {
    if let Some(motivo) = por_que_no_se_puede() {
        eprintln!("no se puede comprobar contra archinstall, se saltea: {motivo}");
        return;
    }

    let d = disco_con_windows();
    for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
        for cifrar in [false, true] {
            let plan_disco = planificar_junto_a(&d, Firmware::Uefi, fs, cifrar).unwrap();

            // Que sea de verdad el modo no destructivo y no una copia del otro:
            // sin esto el test pasaría igual comprobando lo de siempre.
            assert!(!plan_disco.borrar_disco, "{fs:?}/{cifrar}");
            assert!(
                plan_disco.a_destruir(&d).is_empty(),
                "{fs:?}/{cifrar}: el plan destruiría algo"
            );

            let c = configuracion(
                &plan(fs, cifrar),
                &plan_disco,
                d.sector_logico,
                Firmware::Uefi,
                &FuentesDePaquetes {
                    escritorio: &["base".to_string()],
                    aporte: &Aporte::default(),
                    necesarios: &Default::default(),
                },
                Some("4.4.0"),
            );

            assert_eq!(
                c["disk_config"]["device_modifications"][0]["wipe"], false,
                "{fs:?}/{cifrar}: el JSON pide borrar el disco"
            );

            let salida = ejecutar(&serde_json::to_string(&c).unwrap());
            assert!(
                salida.is_empty(),
                "archinstall rechaza el plan no destructivo de {fs:?} (cifrado: {cifrar}):\n{salida}"
            );
        }
    }
}

/// **Y que acepte el plan que formatea una partición que ya existe.**
///
/// Estrena `modify`, que es el estado del que más depende que esto no rompa
/// nada: archinstall borra la partición y la rehace **con la geometría que le
/// mandemos**. Un número mal puesto no da error, mueve la partición encima de
/// la de al lado.
#[test]
fn archinstall_acepta_el_plan_que_formatea_una_particion() {
    if let Some(motivo) = por_que_no_se_puede() {
        eprintln!("no se puede comprobar contra archinstall, se saltea: {motivo}");
        return;
    }

    let d = disco_con_windows();
    let destino = d.particiones[1].clone();

    for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
        for cifrar in [false, true] {
            let plan_disco =
                planificar_sobre(&d, &destino.ruta, Firmware::Uefi, fs, cifrar).unwrap();

            // Que sea de verdad este modo: se pierde la elegida y nada más.
            let victimas: Vec<&str> = plan_disco
                .a_destruir(&d)
                .iter()
                .map(|p| p.ruta.as_str())
                .collect();
            assert_eq!(victimas, [destino.ruta.as_str()], "{fs:?}/{cifrar}");

            let c = configuracion(
                &plan(fs, cifrar),
                &plan_disco,
                d.sector_logico,
                Firmware::Uefi,
                &FuentesDePaquetes {
                    escritorio: &["base".to_string()],
                    aporte: &Aporte::default(),
                    necesarios: &Default::default(),
                },
                Some("4.4.0"),
            );

            let particiones = c["disk_config"]["device_modifications"][0]["partitions"]
                .as_array()
                .unwrap();
            let raiz = particiones
                .iter()
                .find(|p| p["dev_path"] == destino.ruta.as_str())
                .unwrap_or_else(|| panic!("{fs:?}/{cifrar}: la elegida no está en el JSON"));
            assert_eq!(raiz["status"], "modify", "{fs:?}/{cifrar}");

            // La geometría, contra los bytes del disco y no contra el plan: es
            // lo que archinstall va a usar para rehacer la partición.
            //
            // `value` va en MiB —lo dice el `unit` de al lado— y no en
            // sectores. Es lo primero que se escribe mal acá.
            let mib = 1024 * 1024;
            assert_eq!(raiz["start"]["unit"], "MiB");
            assert_eq!(
                raiz["start"]["value"].as_u64().unwrap() * mib,
                destino.inicio_bytes,
                "{fs:?}/{cifrar}: el JSON movería la partición"
            );
            assert_eq!(
                raiz["size"]["value"].as_u64().unwrap() * mib,
                destino.tamano_bytes,
                "{fs:?}/{cifrar}: el JSON cambiaría el tamaño"
            );

            let salida = ejecutar(&serde_json::to_string(&c).unwrap());
            assert!(
                salida.is_empty(),
                "archinstall rechaza el plan sobre una partición de {fs:?} (cifrado: {cifrar}):\n{salida}"
            );
        }
    }
}

/// **Y que acepte un plan armado a mano.**
///
/// Estrena la combinación que ningún otro modo produce: una partición
/// `existing` **con punto de montaje** —un `/home` que se conserva y se monta—
/// y un `fs_type` que no salió de nosotros sino de `lsblk`. Ahí es donde un
/// nombre mal traducido (`vfat` en vez de `fat32`) hace que archinstall
/// rechace el archivo entero.
#[test]
fn archinstall_acepta_un_plan_manual() {
    if let Some(motivo) = por_que_no_se_puede() {
        eprintln!("no se puede comprobar contra archinstall, se saltea: {motivo}");
        return;
    }

    let mut d = disco_con_windows();
    // Una tercera partición para el /home que se conserva.
    let mib = 1024 * 1024;
    let ultima = d.particiones.last().unwrap().clone();
    d.particiones.push(ParticionExistente {
        ruta: "/dev/vda3".into(),
        inicio_bytes: ultima.inicio_bytes + ultima.tamano_bytes,
        tamano_bytes: 21 * 1024 * mib,
        sistema_archivos: Some("ext4".into()),
        etiqueta: Some("home".into()),
        numero: Some(3),
        tipo_particion: Some("0fc63daf-8483-4772-8e79-3d69d8477de4".into()),
        sistema_operativo: None,
    });

    let asignar = |ruta: &str, punto: &str, formatear: bool| AsignacionManual {
        particion: ruta.into(),
        punto_montaje: Some(punto.into()),
        formatear,
    };

    for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
        for cifrar in [false, true] {
            let asignaciones = vec![
                // El ESP de Windows: se conserva y se monta.
                asignar("/dev/vda1", "/boot", false),
                // La de Windows pasa a ser la raíz.
                asignar("/dev/vda2", "/", true),
                // Y un /home que ya existe, que se conserva con lo que tenga.
                asignar("/dev/vda3", "/home", false),
            ];
            let plan_disco =
                planificar_manual(&d, &asignaciones, Firmware::Uefi, fs, cifrar).unwrap();

            // Sólo se pierde la que se formatea.
            let victimas: Vec<&str> = plan_disco
                .a_destruir(&d)
                .iter()
                .map(|p| p.ruta.as_str())
                .collect();
            assert_eq!(victimas, ["/dev/vda2"], "{fs:?}/{cifrar}");

            let c = configuracion(
                &plan(fs, cifrar),
                &plan_disco,
                d.sector_logico,
                Firmware::Uefi,
                &FuentesDePaquetes {
                    escritorio: &["base".to_string()],
                    aporte: &Aporte::default(),
                    necesarios: &Default::default(),
                },
                Some("4.4.0"),
            );

            let particiones = c["disk_config"]["device_modifications"][0]["partitions"]
                .as_array()
                .unwrap();
            let home = particiones
                .iter()
                .find(|p| p["dev_path"] == "/dev/vda3")
                .unwrap_or_else(|| panic!("{fs:?}/{cifrar}: el /home no está en el JSON"));
            assert_eq!(home["status"], "existing", "{fs:?}/{cifrar}");
            assert_eq!(home["mountpoint"], "/home", "{fs:?}/{cifrar}");
            // El nombre que archinstall conoce, no el de `lsblk`.
            assert_eq!(home["fs_type"], "ext4", "{fs:?}/{cifrar}");

            let esp = particiones
                .iter()
                .find(|p| p["dev_path"] == "/dev/vda1")
                .unwrap();
            assert_eq!(esp["fs_type"], "fat32", "{fs:?}/{cifrar}: quedó como vfat");

            let salida = ejecutar(&serde_json::to_string(&c).unwrap());
            assert!(
                salida.is_empty(),
                "archinstall rechaza el plan manual de {fs:?} (cifrado: {cifrar}):\n{salida}"
            );
        }
    }
}

/// Que el guard siga probando lo que la comprobación usa.
///
/// Es el error que dejó pasar cuatro tests reventados: el guard importaba
/// `archinstall` y la comprobación importaba `archinstall.lib.models.device`, y
/// entre los dos había toda una cadena de dependencias que podía estar rota.
/// Si alguien cambia los imports de `COMPROBACION`, esto falla y se entera acá.
#[test]
fn el_guard_prueba_el_modulo_que_la_comprobacion_importa() {
    assert!(
        COMPROBACION.contains(MODULO),
        "la comprobación ya no importa {MODULO}: el guard está probando otra cosa"
    );
}

/// Corre la comprobación con el JSON por la entrada estándar.
fn ejecutar(json: &str) -> String {
    use std::io::Write;
    let mut hijo = Command::new("python3")
        .args(["-c", COMPROBACION])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("python3");
    hijo.stdin.as_mut().unwrap().write_all(json.as_bytes()).unwrap();
    let salida = hijo.wait_with_output().unwrap();
    assert!(
        salida.status.success(),
        "la comprobación no corrió: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    String::from_utf8_lossy(&salida.stdout).trim().to_string()
}
