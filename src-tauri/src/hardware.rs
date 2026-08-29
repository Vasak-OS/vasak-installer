//! Qué hay adentro del equipo, para proponer los controladores que le sirven.
//!
//! Se lee **`/sys/bus/pci/devices` directo**, no `lspci`. Tres razones, en orden
//! de peso:
//!
//! 1. `lspci` es un proceso: unos milisegundos de `fork` + `exec` + carga de
//!    `libpci` + lectura de la base de identificadores, contra un puñado de
//!    lecturas de archivos de doce bytes que el kernel sirve de memoria.
//! 2. Su salida está pensada para leerla una persona, y cambia entre versiones
//!    de `pciutils`. Un parseo de texto que se rompe en silencio en el paso que
//!    elige controladores de video deja a alguien sin aceleración y sin saber
//!    por qué.
//! 3. `pciutils` tendría que estar instalado en el medio live. Los archivos de
//!    `/sys` están siempre, los pone el kernel.
//!
//! Todo lo de acá es **una sugerencia**. Lo detectado llega a la interfaz como
//! una casilla marcada de antemano con su explicación al lado, nunca como algo
//! que se instala solo: el controlador propietario de NVIDIA es una decisión con
//! consecuencias, y tomarla por alguien sin decírselo es peor que no proponerla.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Dónde vive la información de PCI. Parametrizado para poder probarlo contra
/// un árbol de mentira en vez de contra el equipo que corre los tests.
const RAIZ_PCI: &str = "/sys/bus/pci/devices";

/// Los fabricantes que nos interesan, por su identificador de PCI.
///
/// Son números asignados por el PCI-SIG y no cambian nunca: NVIDIA es `0x10de`
/// desde que existe. Por eso van como constantes y no como una tabla editable.
const VENDOR_NVIDIA: u32 = 0x10de;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_INTEL: u32 = 0x8086;
const VENDOR_BROADCOM: u32 = 0x14e4;

/// Clase de dispositivo, el byte más alto del código de clase.
///
/// `/sys/.../class` trae seis dígitos hexadecimales: clase, subclase e interfaz.
/// `0x030000` es una controladora de video; `0x028000` es una de red
/// «otra», que es donde caen las inalámbricas.
const CLASE_VIDEO: u32 = 0x03;
const CLASE_RED: u32 = 0x02;
/// Subclase de red inalámbrica.
const SUBCLASE_INALAMBRICA: u32 = 0x80;

/// Lo que se detectó, como identificadores que `complementos.toml` puede nombrar
/// en su campo `detectar`.
///
/// Cadenas y no un enum porque el archivo de complementos es editable sin
/// recompilar: un `detectar` que no coincide con nada simplemente no propone
/// nada, en vez de ser un error de compilación en un archivo de datos.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hardware {
    /// `gpu-nvidia`, `gpu-amd`, `gpu-intel`, `wifi-broadcom`…
    pub marcas: BTreeSet<String>,
    /// Para mostrar: «NVIDIA GeForce RTX 3060». Vacío si no se pudo averiguar.
    pub descripciones: Vec<String>,
}

/// Lee un archivo de `/sys` que contiene un número en hexadecimal con `0x`.
///
/// Devuelve `None` ante cualquier problema en vez de propagar: un dispositivo
/// que no informa su clase no puede impedir que se miren los demás.
fn leer_hex(ruta: &Path) -> Option<u32> {
    let texto = std::fs::read_to_string(ruta).ok()?;
    let limpio = texto.trim();
    let sin_prefijo = limpio.strip_prefix("0x").unwrap_or(limpio);
    u32::from_str_radix(sin_prefijo, 16).ok()
}

fn nombre_de_fabricante(vendor: u32) -> Option<&'static str> {
    match vendor {
        VENDOR_NVIDIA => Some("NVIDIA"),
        VENDOR_AMD => Some("AMD"),
        VENDOR_INTEL => Some("Intel"),
        VENDOR_BROADCOM => Some("Broadcom"),
        _ => None,
    }
}

/// Recorre los dispositivos PCI de un árbol y saca las marcas.
///
/// Separado de `detectar()` para poder probarlo con un directorio armado a mano:
/// no hay forma de tener una NVIDIA y una AMD en la misma máquina de pruebas, y
/// el código que decide qué controlador proponer es justamente el que no puede
/// estar sin probar.
pub fn detectar_en(raiz: &Path) -> Hardware {
    let mut hw = Hardware::default();

    let Ok(entradas) = std::fs::read_dir(raiz) else {
        // Sin `/sys/bus/pci` no se detecta nada y no se propone nada. Pasa en
        // contenedores y en máquinas sin PCI (algunas ARM): el instalador sigue
        // funcionando, sólo que sin sugerencias.
        return hw;
    };

    for entrada in entradas.flatten() {
        let dispositivo = entrada.path();
        let (Some(vendor), Some(clase_completa)) = (
            leer_hex(&dispositivo.join("vendor")),
            leer_hex(&dispositivo.join("class")),
        ) else {
            continue;
        };

        // El byte alto de los tres es la clase; el del medio, la subclase.
        let clase = clase_completa >> 16;
        let subclase = (clase_completa >> 8) & 0xff;

        let marca = match (clase, vendor) {
            (CLASE_VIDEO, VENDOR_NVIDIA) => Some("gpu-nvidia"),
            (CLASE_VIDEO, VENDOR_AMD) => Some("gpu-amd"),
            (CLASE_VIDEO, VENDOR_INTEL) => Some("gpu-intel"),
            // Sólo las inalámbricas de Broadcom: sus placas de red por cable
            // andan con el controlador del kernel y no necesitan el módulo
            // propietario. Proponerlo para una placa cableada instalaría un
            // DKMS que se recompila en cada actualización de kernel para nada.
            (CLASE_RED, VENDOR_BROADCOM) if subclase == SUBCLASE_INALAMBRICA => {
                Some("wifi-broadcom")
            }
            _ => None,
        };

        let Some(marca) = marca else { continue };
        hw.marcas.insert(marca.to_string());

        // La descripción es para que la interfaz pueda decir qué encontró. El
        // modelo exacto necesitaría la base de identificadores de `pciutils`, que
        // es justamente lo que no queremos depender: con el fabricante y el tipo
        // alcanza para que alguien reconozca su equipo.
        if let Some(fabricante) = nombre_de_fabricante(vendor) {
            let tipo = if clase == CLASE_VIDEO {
                "video"
            } else {
                "red inalámbrica"
            };
            let descripcion = format!("{fabricante} ({tipo})");
            if !hw.descripciones.contains(&descripcion) {
                hw.descripciones.push(descripcion);
            }
        }
    }

    hw
}

pub fn detectar() -> Hardware {
    detectar_en(&PathBuf::from(RAIZ_PCI))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arma un árbol de `/sys` de mentira con los dispositivos que se le pidan.
    fn arbol(dispositivos: &[(&str, u32, u32)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (nombre, vendor, clase) in dispositivos {
            let d = dir.path().join(nombre);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
            std::fs::write(d.join("class"), format!("0x{clase:06x}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn una_nvidia_de_video_se_detecta() {
        let dir = arbol(&[("0000:01:00.0", VENDOR_NVIDIA, 0x030000)]);
        let hw = detectar_en(dir.path());
        assert!(hw.marcas.contains("gpu-nvidia"), "{hw:?}");
        assert!(hw.descripciones.iter().any(|d| d.contains("NVIDIA")));
    }

    /// El caso más común en una portátil: gráficos integrados de Intel y una
    /// NVIDIA dedicada. Las dos tienen que aparecer.
    #[test]
    fn una_hibrida_detecta_las_dos_placas() {
        let dir = arbol(&[
            ("0000:00:02.0", VENDOR_INTEL, 0x030000),
            ("0000:01:00.0", VENDOR_NVIDIA, 0x030000),
        ]);
        let hw = detectar_en(dir.path());
        assert!(hw.marcas.contains("gpu-intel"), "{hw:?}");
        assert!(hw.marcas.contains("gpu-nvidia"), "{hw:?}");
    }

    #[test]
    fn una_amd_de_video_se_detecta() {
        let dir = arbol(&[("0000:03:00.0", VENDOR_AMD, 0x030000)]);
        assert!(detectar_en(dir.path()).marcas.contains("gpu-amd"));
    }

    /// Una placa de audio de AMD **no** es una placa de video de AMD.
    ///
    /// Toda GPU moderna trae además un dispositivo de audio HDMI del mismo
    /// fabricante. Mirando sólo el fabricante, un equipo con audio integrado de
    /// AMD y video de otra marca proponía el controlador equivocado.
    #[test]
    fn la_clase_del_dispositivo_importa_y_no_solo_el_fabricante() {
        // 0x040300 es audio, no video.
        let dir = arbol(&[("0000:01:00.1", VENDOR_AMD, 0x040300)]);
        let hw = detectar_en(dir.path());
        assert!(hw.marcas.is_empty(), "{hw:?}");
    }

    /// Las placas de red **cableadas** de Broadcom no llevan el módulo DKMS.
    ///
    /// Andan con el controlador del kernel. Proponerlo instalaría un módulo que
    /// se recompila en cada actualización de kernel para nada.
    #[test]
    fn solo_el_wifi_de_broadcom_propone_el_modulo() {
        // 0x020000 es Ethernet; 0x028000 es «otra» de red, donde caen las
        // inalámbricas.
        let cableada = arbol(&[("0000:02:00.0", VENDOR_BROADCOM, 0x020000)]);
        assert!(!detectar_en(cableada.path()).marcas.contains("wifi-broadcom"));

        let inalambrica = arbol(&[("0000:02:00.0", VENDOR_BROADCOM, 0x028000)]);
        assert!(detectar_en(inalambrica.path()).marcas.contains("wifi-broadcom"));
    }

    #[test]
    fn un_fabricante_desconocido_no_propone_nada() {
        let dir = arbol(&[("0000:04:00.0", 0x1234, 0x030000)]);
        assert!(detectar_en(dir.path()).marcas.is_empty());
    }

    #[test]
    fn un_dispositivo_sin_sus_archivos_no_rompe_el_resto() {
        let dir = arbol(&[("0000:01:00.0", VENDOR_NVIDIA, 0x030000)]);
        // Un directorio a medias, como los que aparecen mientras el kernel está
        // enumerando.
        std::fs::create_dir_all(dir.path().join("0000:05:00.0")).unwrap();
        // Y uno con basura en vez de un número.
        let roto = dir.path().join("0000:06:00.0");
        std::fs::create_dir_all(&roto).unwrap();
        std::fs::write(roto.join("vendor"), "no es un numero\n").unwrap();
        std::fs::write(roto.join("class"), "\n").unwrap();

        let hw = detectar_en(dir.path());
        assert!(hw.marcas.contains("gpu-nvidia"), "{hw:?}");
    }

    #[test]
    fn sin_arbol_de_pci_no_paniquea() {
        // Pasa en contenedores y en máquinas ARM sin PCI: el instalador tiene que
        // seguir funcionando, sólo que sin sugerencias.
        let hw = detectar_en(Path::new("/no/existe/este/arbol"));
        assert!(hw.marcas.is_empty());
        assert!(hw.descripciones.is_empty());
    }

    #[test]
    fn dos_placas_del_mismo_fabricante_no_duplican_la_descripcion() {
        let dir = arbol(&[
            ("0000:01:00.0", VENDOR_AMD, 0x030000),
            ("0000:02:00.0", VENDOR_AMD, 0x030000),
        ]);
        let hw = detectar_en(dir.path());
        assert_eq!(hw.descripciones.len(), 1, "{hw:?}");
    }

    /// Contra el equipo real: lo que importa es que no paniquee y que lo que
    /// devuelva tenga sentido.
    #[test]
    fn el_equipo_real_se_puede_sondear() {
        let hw = detectar();
        for marca in &hw.marcas {
            assert!(
                ["gpu-nvidia", "gpu-amd", "gpu-intel", "wifi-broadcom"].contains(&marca.as_str()),
                "marca inesperada: {marca}"
            );
        }
    }
}
