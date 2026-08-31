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

use vasak_installer_lib::archconfig::configuracion;
use vasak_installer_lib::complementos::Aporte;
use vasak_installer_lib::layout::{planificar, Disco, Firmware};
use vasak_installer_lib::protocol::{EsquemaDisco, PlanInstalacion, Secretos, SistemaArchivos};

/// Lo que `parse_arg` hace con cada partición, y lo que después le pide.
const COMPROBACION: &str = r#"
import json, sys
from pathlib import Path
from archinstall.lib.models.device import (
    FilesystemType, ModificationStatus, PartitionFlag, PartitionModification,
    PartitionType, Size, SubvolumeModification,
)

problemas = []
cfg = json.loads(sys.stdin.read())
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
                    &["base".to_string()],
                    &Aporte::default(),
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
