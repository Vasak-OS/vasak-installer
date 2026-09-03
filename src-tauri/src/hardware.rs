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
//! # Dos clases de detección, con dos destinos distintos
//!
//! **Lo que se propone.** El controlador propietario de NVIDIA y el módulo de
//! Broadcom llegan a la interfaz como una casilla marcada de antemano con su
//! explicación al lado, nunca como algo que se instala solo: son decisiones con
//! consecuencias, y tomarlas por alguien sin decírselo es peor que no
//! proponerlas. Eso son complementos, y son opcionales por diseño.
//!
//! **Lo que hace falta.** El firmware del audio de una portátil Intel no es una
//! decisión: sin él no hay sonido. Eso no puede ser un complemento —el propio
//! `complementos.toml` dice que ninguno puede ser necesario, justamente para que
//! un fallo instalando uno no sea fatal— así que va a la lista de paquetes del
//! sistema, y lo calcula [`paquetes`].
//!
//! # Por qué se mira qué usa el medio vivo
//!
//! Para el segundo grupo, la señal no es una tabla de identificadores PCI: es
//! **qué encontró el kernel que está corriendo**. La ISO trae el firmware de
//! todo, así que si el medio vivo tiene un `/sys/class/bluetooth/hci0`, esa
//! máquina tiene Bluetooth funcionando y el sistema instalado necesita `bluez`.
//! Si no lo tiene, no lo va a necesitar nunca.
//!
//! Es más confiable que adivinar por identificadores —no hay tabla que
//! mantener, y cubre el hardware que todavía no existía cuando se escribió
//! esto— y es exactamente la pregunta que importa: no «¿qué placa hay?» sino
//! «¿qué anduvo?».

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Dónde vive la información de PCI. Parametrizado para poder probarlo contra
/// un árbol de mentira en vez de contra el equipo que corre los tests.
const RAIZ_PCI: &str = "/sys/bus/pci/devices";

/// Dónde el kernel publica lo que encontró y ató.
///
/// Se miran las tres como directorios: que exista una entrada adentro es la
/// señal. Parametrizadas junto con la de PCI para poder probarlas.
const RAIZ_BLUETOOTH: &str = "/sys/class/bluetooth";
const RAIZ_RED: &str = "/sys/class/net";
const RAIZ_MODULOS: &str = "/sys/module";

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

/// Las señales de lo que el kernel del medio vivo encontró y ató.
///
/// `bluetooth`, `wifi` y `audio-sof`. Van como marcas junto a las de PCI porque
/// alimentan lo mismo: la decisión de qué paquetes necesita esta máquina.
///
/// Cada ruta se pasa por separado para poder probarlas con directorios de
/// mentira; en el equipo son las tres constantes de arriba.
pub fn señales_del_medio(bluetooth: &Path, red: &Path, modulos: &Path) -> BTreeSet<String> {
    let mut marcas = BTreeSet::new();

    // Un `hciN` significa que hay una controladora y que el kernel la ató. Sin
    // adaptador el directorio existe y está vacío.
    if std::fs::read_dir(bluetooth).into_iter().flatten().flatten().next().is_some() {
        marcas.insert("bluetooth".to_string());
    }

    // `phy80211` es el enlace que sólo tienen las interfaces inalámbricas. Mirar
    // el nombre —`wlan0`, `wlp3s0`— sería adivinar: los nombres predecibles de
    // systemd no garantizan el prefijo.
    if let Ok(entradas) = std::fs::read_dir(red) {
        if entradas.flatten().any(|e| e.path().join("phy80211").exists()) {
            marcas.insert("wifi".to_string());
        }
    }

    // El audio de las portátiles Intel de la última década pasa por SOF, y su
    // firmware es un paquete aparte: sin él la máquina queda muda. Que el módulo
    // esté cargado en el medio vivo es la señal de que esta máquina lo usa.
    if modulos.join("snd_sof").is_dir() {
        marcas.insert("audio-sof".to_string());
    }

    marcas
}

/// Los paquetes que **esta** máquina necesita y que no van en el metapaquete.
///
/// La regla es la que pedía este cambio: `vasakos-desktop` lleva lo que necesita
/// cualquier VasakOS, y lo que depende de la máquina lo pone el instalador. Antes
/// toda instalación cargaba los 41 MiB del controlador de vídeo de Intel y los
/// 43 del firmware de audio de Intel, en una máquina AMD también.
///
/// Lo que **no** está acá y podría parecer que falta:
///
/// * El microcódigo. archinstall lee el fabricante de la CPU y agrega el que
///   corresponde (`_get_microcode`); nombrarlo sería instalar los dos.
/// * `linux-firmware`. Está en la lista base de archinstall, que no se puede
///   cambiar por configuración. Son 407 MiB repartidos en diez subpaquetes que
///   Arch ya separó por fabricante, así que hay margen para bajarlos — pero no
///   desde acá.
/// * Las herramientas del sistema de archivos elegido: también las agrega
///   archinstall, según la elección (`installation_pkg`).
/// * El controlador propietario de NVIDIA y el módulo de Broadcom: son
///   complementos, porque son decisiones y no requisitos.
pub fn paquetes(hardware: &Hardware) -> BTreeSet<String> {
    let mut paquetes = BTreeSet::new();

    for marca in &hardware.marcas {
        match marca.as_str() {
            // El controlador de vídeo de Intel para la aceleración de vídeo.
            // AMD no lleva nada: su VAAPI viene en `mesa`, que sí es del
            // escritorio. NVIDIA tampoco: el propietario es un complemento.
            "gpu-intel" => {
                paquetes.insert("intel-media-driver".to_string());
            }
            "bluetooth" => {
                paquetes.insert("bluez".to_string());
                paquetes.insert("bluez-utils".to_string());
            }
            // La base de datos de regulaciones por país. `wpa_supplicant` no se
            // nombra: lo pide `networkmanager`, que sí es del escritorio.
            "wifi" => {
                paquetes.insert("wireless-regdb".to_string());
            }
            "audio-sof" => {
                paquetes.insert("sof-firmware".to_string());
            }
            _ => {}
        }
    }

    paquetes
}

pub fn detectar() -> Hardware {
    let mut hw = detectar_en(&PathBuf::from(RAIZ_PCI));
    hw.marcas.extend(señales_del_medio(
        Path::new(RAIZ_BLUETOOTH),
        Path::new(RAIZ_RED),
        Path::new(RAIZ_MODULOS),
    ));
    hw
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
                [
                    "gpu-nvidia",
                    "gpu-amd",
                    "gpu-intel",
                    "wifi-broadcom",
                    "bluetooth",
                    "wifi",
                    "audio-sof",
                ]
                .contains(&marca.as_str()),
                "marca inesperada: {marca}"
            );
        }
    }

    /// Arma un árbol con las señales del medio vivo que se le pidan.
    fn medio(bt: bool, wifi: bool, sof: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let (b, r, m) = (dir.path().join("bt"), dir.path().join("net"), dir.path().join("mod"));
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&r).unwrap();
        std::fs::create_dir_all(&m).unwrap();
        if bt {
            std::fs::create_dir_all(b.join("hci0")).unwrap();
        }
        // Una cableada siempre, para que «hay interfaces» no sea lo que decide.
        std::fs::create_dir_all(r.join("enp3s0")).unwrap();
        if wifi {
            std::fs::create_dir_all(r.join("wlp2s0").join("phy80211")).unwrap();
        }
        if sof {
            std::fs::create_dir_all(m.join("snd_sof")).unwrap();
        }
        dir
    }

    fn marcas_de(dir: &tempfile::TempDir) -> BTreeSet<String> {
        señales_del_medio(
            &dir.path().join("bt"),
            &dir.path().join("net"),
            &dir.path().join("mod"),
        )
    }

    #[test]
    fn sin_nada_atado_no_se_marca_nada() {
        // Los directorios existen y están vacíos: es lo que se ve en una máquina
        // sin adaptador Bluetooth. Marcar por «el directorio está» instalaría
        // bluez en todas.
        assert!(marcas_de(&medio(false, false, false)).is_empty());
    }

    #[test]
    fn un_hci_significa_que_hay_bluetooth() {
        assert!(marcas_de(&medio(true, false, false)).contains("bluetooth"));
    }

    #[test]
    fn el_wifi_se_reconoce_por_phy80211_y_no_por_el_nombre() {
        // Los nombres predecibles de systemd no garantizan ningún prefijo, así
        // que mirar si empieza con «wl» es adivinar. El enlace `phy80211` sólo
        // lo tienen las inalámbricas.
        let con = medio(false, true, false);
        assert!(marcas_de(&con).contains("wifi"));

        // Y una máquina con sólo cableada no lo lleva, aunque tenga interfaces.
        assert!(!marcas_de(&medio(false, false, false)).contains("wifi"));
    }

    #[test]
    fn el_modulo_de_sof_significa_que_ese_audio_necesita_su_firmware() {
        assert!(marcas_de(&medio(false, false, true)).contains("audio-sof"));
    }

    #[test]
    fn las_rutas_que_no_existen_no_paniquean() {
        // Pasa en un contenedor, y pasaría en una máquina sin esas clases.
        let vacio = Path::new("/no/existe/esto");
        assert!(señales_del_medio(vacio, vacio, vacio).is_empty());
    }

    fn con_marcas(marcas: &[&str]) -> Hardware {
        Hardware {
            marcas: marcas.iter().map(|m| m.to_string()).collect(),
            descripciones: Vec::new(),
        }
    }

    #[test]
    fn una_maquina_intel_lleva_su_controlador_de_video() {
        assert!(paquetes(&con_marcas(&["gpu-intel"])).contains("intel-media-driver"));
    }

    #[test]
    fn una_maquina_amd_no_lleva_nada_de_video() {
        // Su VAAPI viene en `mesa`, que sí es del escritorio. Antes toda máquina
        // AMD cargaba los 41 MiB del controlador de Intel.
        assert!(paquetes(&con_marcas(&["gpu-amd"])).is_empty());
    }

    #[test]
    fn una_nvidia_no_agrega_nada_acá() {
        // El propietario es un complemento: es una decisión con consecuencias y
        // se pregunta, no se instala sola.
        assert!(paquetes(&con_marcas(&["gpu-nvidia"])).is_empty());
    }

    #[test]
    fn el_bluetooth_trae_el_servicio_y_su_herramienta() {
        let p = paquetes(&con_marcas(&["bluetooth"]));
        assert!(p.contains("bluez"));
        assert!(p.contains("bluez-utils"));
    }

    #[test]
    fn el_wifi_no_nombra_wpa_supplicant() {
        // Lo pide `networkmanager`, que es del escritorio: nombrarlo acá sería
        // repetir una dependencia que ya llega.
        let p = paquetes(&con_marcas(&["wifi"]));
        assert!(p.contains("wireless-regdb"));
        assert!(!p.contains("wpa_supplicant"));
    }

    #[test]
    fn nunca_se_nombra_lo_que_archinstall_ya_pone() {
        // El microcódigo lo elige según la CPU y `linux-firmware` está en su
        // lista base. Nombrarlos sería instalar los dos microcódigos en cada
        // máquina, que es exactamente lo que este cambio vino a sacar.
        let todas = con_marcas(&[
            "gpu-intel", "gpu-amd", "gpu-nvidia", "bluetooth", "wifi", "audio-sof",
            "wifi-broadcom",
        ]);
        let p = paquetes(&todas);
        for prohibido in ["amd-ucode", "intel-ucode", "linux-firmware", "base", "mkinitcpio"] {
            assert!(!p.contains(prohibido), "{prohibido} lo pone archinstall");
        }
    }

    #[test]
    fn sin_hardware_detectado_no_se_agrega_nada() {
        assert!(paquetes(&Hardware::default()).is_empty());
    }
}
