//! Planificación de la tabla de particiones.
//!
//! archinstall **no propone nada** desde un archivo de configuración: su
//! `disk_config` con `config_type: "default_layout"` igual espera la lista
//! completa de particiones con sus posiciones y tamaños exactos. La sugerencia
//! automática vive en sus menús interactivos, que es justamente lo que no
//! usamos. Así que el plan lo armamos acá.
//!
//! Que sea una función pura es a propósito: **es el único código del instalador
//! cuyo error borra datos.** Una función que recibe un disco y devuelve una
//! lista de particiones se puede probar con cien discos distintos sin tocar
//! ninguno; el mismo cálculo hecho al vuelo mientras se escribe la tabla, no.
//!
//! Todo el aritmético va en MiB enteros. Con bytes o con flotantes los inicios
//! terminan sin alinear a 1 MiB, y una partición desalineada en un SSD escribe
//! de a dos bloques donde debería escribir uno.

use serde::{Deserialize, Serialize};

use crate::protocol::SistemaArchivos;

/// Un MiB en bytes.
const MIB: u64 = 1024 * 1024;

/// Dónde empieza la primera partición.
///
/// El MiB inicial no es desperdicio: ahí van el MBR protector y la cabecera GPT
/// primaria con su tabla de entradas, y arrancar en 1 MiB alinea todo lo que
/// sigue.
const INICIO_MIB: u64 = 1;

/// La partición del sistema EFI, montada en `/boot`.
///
/// 1 GiB y no 512 MiB porque en `/boot` viven el kernel y **los dos** initramfs
/// (el normal y el de respaldo), y cada actualización de kernel los reescribe.
/// Con 512 MiB un sistema con dos kernels y microcódigo queda al borde, y
/// `pacman` fallando por espacio en `/boot` deja un sistema que no arranca.
const ESP_MIB: u64 = 1024;

/// La partición `bios_grub` de los equipos sin UEFI.
///
/// No lleva sistema de archivos ni punto de montaje: GRUB escribe su segunda
/// etapa en crudo ahí porque en GPT no existe el hueco post-MBR que usaba en
/// discos MBR. 2 MiB es lo que recomienda GRUB y sobra.
const BIOS_GRUB_MIB: u64 = 2;

/// Lo que se deja libre al final del disco.
///
/// La cabecera GPT **secundaria** y su copia de la tabla van en los últimos
/// sectores, así que una raíz que llega hasta el final del disco no cabe. 1 MiB
/// las cubre con margen y mantiene la alineación.
const RESERVA_FINAL_MIB: u64 = 1;

/// El disco más chico que aceptamos.
///
/// Con menos no entra el escritorio: el sistema base más los paquetes de
/// VasakOS pasan holgadamente los 10 GiB, y dejar instalar en 12 GiB produce un
/// `pacstrap` que muere por espacio a los veinte minutos. Es mejor decirlo
/// antes de formatear.
pub const MINIMO_GIB: u64 = 20;

/// Un disco tal como lo ve el sondeo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disco {
    /// `/dev/nvme0n1`, `/dev/sda`.
    pub ruta: String,
    /// Lo que muestra la interfaz: `Samsung SSD 980 1TB`.
    pub modelo: String,
    pub tamano_bytes: u64,
    /// Tamaño de sector **lógico**. archinstall lo quiere dentro de cada
    /// tamaño, y en los discos de 4Kn es 4096, no 512.
    pub sector_logico: u64,
    /// `true` en discos mecánicos. Cambia las opciones de montaje de btrfs.
    pub rotacional: bool,
    /// `true` si es NVMe. También cambia las opciones de montaje.
    pub nvme: bool,
    /// `true` si el disco o alguna de sus particiones está montada ahora mismo.
    /// El disco del que arrancó la ISO cae acá, y ofrecerlo sería ofrecer
    /// borrar el instalador en marcha.
    pub en_uso: bool,
    /// Lo que hay hoy, para que el resumen pueda decir qué se va a perder.
    pub particiones: Vec<ParticionExistente>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticionExistente {
    pub ruta: String,
    pub tamano_bytes: u64,
    pub sistema_archivos: Option<String>,
    pub etiqueta: Option<String>,
    /// Lo que se pudo averiguar del sistema operativo que vive ahí, si hay uno.
    pub sistema_operativo: Option<String>,
}

/// El firmware del equipo. Decide si hay ESP o `bios_grub`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Firmware {
    Uefi,
    Bios,
}

/// Una partición del plan. Todavía no es JSON de archinstall: eso lo hace
/// `archconfig.rs`. Acá están los números y las decisiones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticionPlaneada {
    pub inicio_mib: u64,
    pub tamano_mib: u64,
    /// `None` en `bios_grub`: no lleva sistema de archivos.
    pub sistema_archivos: Option<&'static str>,
    /// `None` cuando no se monta —`bios_grub`— y también en la raíz btrfs con
    /// subvolúmenes: ahí el punto de montaje lo lleva el subvolumen `@`, y
    /// poner los dos hace que archinstall monte la partición encima de sus
    /// propios subvolúmenes.
    pub punto_montaje: Option<&'static str>,
    pub opciones_montaje: Vec<String>,
    pub banderas: Vec<&'static str>,
    /// Subvolúmenes btrfs, con el nombre tal cual va al disco.
    pub subvolumenes: Vec<(&'static str, &'static str)>,
    /// Si esta partición va cifrada con LUKS.
    pub cifrada: bool,
    /// Para los mensajes de la interfaz: qué es esta partición.
    pub rol: Rol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rol {
    BiosGrub,
    Esp,
    Raiz,
}

/// Los subvolúmenes, con los mismos nombres y puntos de montaje que venía
/// usando la configuración de calamares.
///
/// Se conservan tal cual y no se adopta el juego de archinstall (`@`, `@home`,
/// `@log`, `@pkg`, `.snapshots`) porque un sistema instalado con la ISO anterior
/// y uno instalado con ésta tienen que verse igual: si los nombres cambian, un
/// respaldo de subvolúmenes hecho con la ISO vieja no se restaura en la nueva, y
/// nadie se enteraría hasta necesitarlo.
///
/// `/var/cache` como subvolumen aparte es lo que permite excluir la caché de
/// pacman de una instantánea sin excluir `/var` entero.
const SUBVOLUMENES: &[(&str, &str)] = &[
    ("@", "/"),
    ("@home", "/home"),
    ("@root", "/root"),
    ("@srv", "/srv"),
    ("@cache", "/var/cache"),
    ("@tmp", "/var/tmp"),
    ("@log", "/var/log"),
];

/// Por qué un disco no se puede usar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorPlan {
    /// Más chico que `MINIMO_GIB`.
    Chico { tiene_gib: u64, minimo_gib: u64 },
    /// Está montado. Casi siempre es el pendrive de la ISO.
    EnUso,
    /// Tamaño cero o sector lógico absurdo: el disco desapareció entre el
    /// sondeo y ahora, o `lsblk` devolvió algo que no se puede usar.
    Invalido,
}

impl std::fmt::Display for ErrorPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorPlan::Chico {
                tiene_gib,
                minimo_gib,
            } => write!(
                f,
                "el disco tiene {tiene_gib} GiB y hacen falta al menos {minimo_gib} GiB"
            ),
            ErrorPlan::EnUso => write!(f, "el disco está en uso"),
            ErrorPlan::Invalido => write!(f, "el disco informa un tamaño o un sector inválidos"),
        }
    }
}

/// Arma el plan de particionado para borrar el disco entero.
///
/// Devuelve las particiones en orden de posición en el disco. El orden de
/// **montaje** no es éste y no se calcula acá: archinstall ordena los puntos de
/// montaje por profundidad antes de montar (`mount_ordered_layout`), que es lo
/// que garantiza que `/` se monte antes que `/boot`.
pub fn planificar(
    disco: &Disco,
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Vec<ParticionPlaneada>, ErrorPlan> {
    if disco.tamano_bytes == 0 || disco.sector_logico == 0 {
        return Err(ErrorPlan::Invalido);
    }
    if disco.en_uso {
        return Err(ErrorPlan::EnUso);
    }

    let total_mib = disco.tamano_bytes / MIB;
    let tiene_gib = disco.tamano_bytes / (1024 * MIB);
    if tiene_gib < MINIMO_GIB {
        return Err(ErrorPlan::Chico {
            tiene_gib,
            minimo_gib: MINIMO_GIB,
        });
    }

    let mut plan = Vec::with_capacity(2);
    let mut cursor = INICIO_MIB;

    match firmware {
        Firmware::Uefi => {
            plan.push(ParticionPlaneada {
                inicio_mib: cursor,
                tamano_mib: ESP_MIB,
                sistema_archivos: Some("fat32"),
                punto_montaje: Some("/boot"),
                // El ESP es FAT y FAT no tiene permisos: sin `umask` queda
                // legible por cualquiera, y ahí están el kernel y el initramfs.
                // `0077` lo deja sólo para root, que es quien lo escribe.
                opciones_montaje: vec!["umask=0077".into()],
                // Las dos: `esp` es la que marca la partición como Sistema EFI
                // en GPT, y `boot` es como la nombra parted —que es la
                // herramienta que archinstall usa por debajo.
                banderas: vec!["boot", "esp"],
                subvolumenes: Vec::new(),
                // El ESP **nunca** va cifrado: el firmware tiene que poder
                // leerlo para arrancar.
                cifrada: false,
                rol: Rol::Esp,
            });
            cursor += ESP_MIB;
        }
        Firmware::Bios => {
            plan.push(ParticionPlaneada {
                inicio_mib: cursor,
                tamano_mib: BIOS_GRUB_MIB,
                sistema_archivos: None,
                punto_montaje: None,
                opciones_montaje: Vec::new(),
                banderas: vec!["bios_grub"],
                subvolumenes: Vec::new(),
                cifrada: false,
                rol: Rol::BiosGrub,
            });
            cursor += BIOS_GRUB_MIB;
        }
    }

    // Lo que sobra, menos la reserva del final. La resta se hace con
    // `saturating_sub` y después se verifica: en un disco justo al límite un
    // desbordamiento daría una partición gigante y `parted` fallaría con un
    // error sobre sectores que no le dice nada a nadie.
    let tamano_raiz = total_mib
        .saturating_sub(cursor)
        .saturating_sub(RESERVA_FINAL_MIB);
    if tamano_raiz == 0 {
        return Err(ErrorPlan::Chico {
            tiene_gib,
            minimo_gib: MINIMO_GIB,
        });
    }

    let usa_subvolumenes = fs == SistemaArchivos::Btrfs;
    plan.push(ParticionPlaneada {
        inicio_mib: cursor,
        tamano_mib: tamano_raiz,
        sistema_archivos: Some(fs.como_archinstall()),
        // Con subvolúmenes el punto de montaje lo lleva `@`. Poner los dos hace
        // que archinstall monte la partición cruda en `/` y después los
        // subvolúmenes encima, y el sistema termina instalado fuera de `@`.
        punto_montaje: if usa_subvolumenes { None } else { Some("/") },
        opciones_montaje: opciones_de_montaje(fs, disco),
        banderas: Vec::new(),
        subvolumenes: if usa_subvolumenes {
            SUBVOLUMENES.to_vec()
        } else {
            Vec::new()
        },
        cifrada: cifrar,
        rol: Rol::Raiz,
    });

    Ok(plan)
}

/// Las opciones de montaje de la raíz, según el sistema de archivos y el medio.
///
/// Son las mismas que traía la configuración de calamares, con un cambio: el
/// `defaults` no se pone. `defaults` es la lista implícita de mount y ponerlo
/// junto a `noatime` no agrega nada, pero sí ensucia el fstab.
fn opciones_de_montaje(fs: SistemaArchivos, disco: &Disco) -> Vec<String> {
    match fs {
        SistemaArchivos::Btrfs => {
            // `zstd:1` en NVMe y `zstd` (nivel 3) en el resto. En un NVMe el
            // disco es tan rápido que comprimir más fuerte lo frena en vez de
            // ayudarlo; en un disco mecánico es al revés.
            let compresion = if disco.nvme {
                "compress=zstd:1"
            } else {
                "compress=zstd"
            };
            vec![compresion.into(), "noatime".into()]
        }
        SistemaArchivos::Ext4 => vec!["noatime".into()],
        SistemaArchivos::Xfs => vec![
            "noatime".into(),
            "lazytime".into(),
            "inode64".into(),
            "logbsize=256k".into(),
            "noquota".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disco_de(tamano_gib: u64) -> Disco {
        Disco {
            ruta: "/dev/sda".into(),
            modelo: "Disco de prueba".into(),
            tamano_bytes: tamano_gib * 1024 * MIB,
            sector_logico: 512,
            rotacional: false,
            nvme: false,
            en_uso: false,
            particiones: Vec::new(),
        }
    }

    #[test]
    fn uefi_arma_esp_y_raiz_sin_huecos() {
        let disco = disco_de(100);
        let plan = planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].rol, Rol::Esp);
        assert_eq!(plan[0].inicio_mib, 1);
        assert_eq!(plan[0].tamano_mib, 1024);
        assert_eq!(plan[1].rol, Rol::Raiz);

        // Sin hueco entre las dos: la raíz empieza exactamente donde termina el
        // ESP. Un hueco de un MiB no rompe nada, pero es la clase de descuido
        // que después aparece como «el disco tiene 3 particiones».
        assert_eq!(plan[1].inicio_mib, plan[0].inicio_mib + plan[0].tamano_mib);
    }

    #[test]
    fn el_plan_no_se_pasa_del_disco() {
        // Varios tamaños, incluidos los que no son múltiplos redondos de MiB:
        // ahí es donde una división entera de más deja la última partición un
        // MiB fuera del disco.
        for gib in [20u64, 21, 64, 100, 250, 500, 931, 1863] {
            let mut disco = disco_de(gib);
            // Unos bytes de más para que el tamaño no caiga justo en un MiB.
            disco.tamano_bytes += 12345;
            let total_mib = disco.tamano_bytes / MIB;

            for firmware in [Firmware::Uefi, Firmware::Bios] {
                let plan = planificar(&disco, firmware, SistemaArchivos::Ext4, false).unwrap();
                let ultima = plan.last().unwrap();
                let fin = ultima.inicio_mib + ultima.tamano_mib;
                assert!(
                    fin <= total_mib - RESERVA_FINAL_MIB,
                    "{gib} GiB / {firmware:?}: el plan termina en {fin} MiB y el disco tiene {total_mib} MiB"
                );
            }
        }
    }

    #[test]
    fn bios_arma_bios_grub_sin_montarla() {
        let plan = planificar(&disco_de(50), Firmware::Bios, SistemaArchivos::Ext4, false).unwrap();

        assert_eq!(plan[0].rol, Rol::BiosGrub);
        assert_eq!(plan[0].tamano_mib, 2);
        // Sin sistema de archivos y sin punto de montaje: GRUB escribe crudo.
        // Si alguna de las dos se pusiera, `mkfs` la formatearía y el fstab
        // intentaría montar 2 MiB sin filesystem en cada arranque.
        assert!(plan[0].sistema_archivos.is_none());
        assert!(plan[0].punto_montaje.is_none());
        assert_eq!(plan[0].banderas, vec!["bios_grub"]);
        // Y no hay ESP: en BIOS no existe.
        assert!(!plan.iter().any(|p| p.rol == Rol::Esp));
    }

    #[test]
    fn btrfs_deja_el_punto_de_montaje_en_el_subvolumen() {
        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let raiz = plan.last().unwrap();

        // Éste es el bug que borra la instalación: con `mountpoint` y
        // subvolúmenes a la vez, archinstall monta la partición cruda en `/` y
        // el sistema queda instalado afuera de `@`, así que el primer arranque
        // encuentra un `@` vacío.
        assert!(
            raiz.punto_montaje.is_none(),
            "la raíz btrfs no puede llevar punto de montaje propio"
        );
        assert_eq!(raiz.subvolumenes.len(), 7);
        assert!(raiz.subvolumenes.contains(&("@", "/")));
        assert!(raiz.subvolumenes.contains(&("@log", "/var/log")));
    }

    #[test]
    fn sin_btrfs_la_raiz_se_monta_directo() {
        for fs in [SistemaArchivos::Ext4, SistemaArchivos::Xfs] {
            let plan = planificar(&disco_de(50), Firmware::Uefi, fs, false).unwrap();
            let raiz = plan.last().unwrap();
            assert_eq!(raiz.punto_montaje, Some("/"), "{fs:?}");
            assert!(raiz.subvolumenes.is_empty(), "{fs:?}");
        }
    }

    #[test]
    fn el_esp_nunca_va_cifrado() {
        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, true).unwrap();
        let esp = &plan[0];
        // El firmware lee el ESP antes de que exista nada que pueda descifrarlo.
        // Cifrarlo produce un equipo que no arranca.
        assert!(!esp.cifrada);
        assert!(plan.last().unwrap().cifrada, "la raíz sí tiene que ir cifrada");
    }

    #[test]
    fn nvme_comprime_mas_flojo_que_un_disco_comun() {
        let mut nvme = disco_de(50);
        nvme.nvme = true;
        let plan = planificar(&nvme, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        assert!(plan.last().unwrap().opciones_montaje.contains(&"compress=zstd:1".to_string()));

        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        assert!(plan.last().unwrap().opciones_montaje.contains(&"compress=zstd".to_string()));
    }

    #[test]
    fn un_disco_chico_se_rechaza_antes_de_tocarlo() {
        let err = planificar(&disco_de(8), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err();
        assert_eq!(
            err,
            ErrorPlan::Chico {
                tiene_gib: 8,
                minimo_gib: MINIMO_GIB
            }
        );
    }

    #[test]
    fn un_disco_montado_se_rechaza() {
        let mut disco = disco_de(500);
        disco.en_uso = true;
        // Es el pendrive del que arrancó la ISO. Borrarlo mata la instalación
        // en marcha.
        assert_eq!(
            planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err(),
            ErrorPlan::EnUso
        );
    }

    #[test]
    fn un_disco_de_cero_bytes_no_paniquea() {
        // `lsblk` puede informar tamaño cero para un lector de tarjetas vacío.
        // Antes de la comprobación explícita esto llegaba a la división y
        // salía un plan con una partición de tamaño absurdo.
        let mut disco = disco_de(0);
        disco.tamano_bytes = 0;
        assert_eq!(
            planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err(),
            ErrorPlan::Invalido
        );
    }

    #[test]
    fn los_subvolumenes_no_se_repiten_ni_en_nombre_ni_en_punto_de_montaje() {
        let mut nombres: Vec<&str> = SUBVOLUMENES.iter().map(|(n, _)| *n).collect();
        let cantidad = nombres.len();
        nombres.sort_unstable();
        nombres.dedup();
        assert_eq!(nombres.len(), cantidad, "hay un subvolumen repetido");

        let mut puntos: Vec<&str> = SUBVOLUMENES.iter().map(|(_, p)| *p).collect();
        puntos.sort_unstable();
        puntos.dedup();
        assert_eq!(puntos.len(), cantidad, "hay dos subvolúmenes en el mismo punto");

        // Todos absolutos: archinstall los usa tal cual para armar la ruta
        // bajo el destino, y uno relativo terminaría montado en el directorio
        // de trabajo del instalador.
        assert!(SUBVOLUMENES.iter().all(|(_, p)| p.starts_with('/')));
    }
}
