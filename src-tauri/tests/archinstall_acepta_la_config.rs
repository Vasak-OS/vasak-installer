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
//! Si archinstall no está instalado, la prueba se saltea: es la que corre en la
//! ISO, no necesariamente en la máquina de quien desarrolla.

use std::process::Command;

use vasak_installer_lib::archconfig::{configuracion, FuentesDePaquetes};
use vasak_installer_lib::complementos::Aporte;
use vasak_installer_lib::layout::{planificar, Disco, Firmware};
use vasak_installer_lib::protocol::{EsquemaDisco, PlanInstalacion, Secretos, SistemaArchivos};

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
    mod = PartitionModification(
        status=ModificationStatus(partition["status"]),
        fs_type=fs_type,
        start=Size.parse_args(partition["start"]),
        length=Size.parse_args(partition["size"]),
        mount_options=partition["mount_options"],
        mountpoint=Path(partition["mountpoint"]) if partition["mountpoint"] else None,
        dev_path=None,
        type=PartitionType(partition["type"]),
        flags=flags,
        btrfs_subvols=SubvolumeModification.parse_args(partition.get("btrfs", [])),
    )
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

fn plan(fs: SistemaArchivos, cifrar: bool) -> PlanInstalacion {
    PlanInstalacion {
        disco: "/dev/vda".into(),
        esquema: EsquemaDisco::BorrarTodo,
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

fn hay_archinstall() -> bool {
    Command::new("python3")
        .args(["-c", "import archinstall"])
        .output()
        .map(|s| s.status.success())
        .unwrap_or(false)
}

#[test]
fn archinstall_acepta_todas_las_particiones_que_le_mandamos() {
    if !hay_archinstall() {
        eprintln!("archinstall no está instalado: se saltea");
        return;
    }

    let d = disco();
    for firmware in [Firmware::Uefi, Firmware::Bios] {
        for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
            for cifrar in [false, true] {
                let particiones = planificar(&d, firmware, fs, cifrar).unwrap();
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
