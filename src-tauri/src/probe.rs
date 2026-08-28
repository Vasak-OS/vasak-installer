//! Lo que se puede averiguar del equipo antes de tocarlo.
//!
//! Casi todo esto se lee de `/sys`, `/proc` y `/usr/share`, que son legibles por
//! cualquiera, así que corre en el proceso de la ventana sin pedir permisos. Lo
//! único que necesita root es enumerar los discos —`lsblk` los lista sin root
//! pero no lee la tabla de particiones de un disco cifrado ni corre `os-prober`—
//! y eso vive en el ayudante.
//!
//! Regla de todo el módulo: **si un dato no se puede averiguar, se devuelve
//! vacío y se sigue.** El instalador tiene que abrir aunque falte
//! `/usr/share/i18n/SUPPORTED`; una lista de idiomas vacía se puede completar a
//! mano, una ventana que no abre no.

use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::layout::{Disco, Firmware, ParticionExistente};

/// El equipo, para la pantalla de resumen y para las comprobaciones previas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sistema {
    pub firmware: Firmware,
    pub memoria_bytes: u64,
    pub cpu: String,
    pub nucleos: usize,
    /// Si hay ruta por defecto. La instalación baja todo de los repos, así que
    /// sin esto no se puede empezar.
    pub hay_red: bool,
    /// Si el equipo corre dentro de una máquina virtual, y de cuál. Sirve para
    /// avisar antes de que alguien instale sobre el disco de su anfitrión.
    pub virtualizacion: Option<String>,
}

/// UEFI o BIOS.
///
/// La existencia de `/sys/firmware/efi` es la comprobación canónica y la que usa
/// todo el ecosistema: el directorio lo crea el kernel sólo cuando arrancó por
/// EFI. Ojo con la variante: existe `efivars` **dentro** de `efi`, y buscar
/// `efivars` directamente falla en los equipos donde el módulo no está cargado,
/// aunque el arranque haya sido EFI.
pub fn detectar_firmware() -> Firmware {
    if std::path::Path::new("/sys/firmware/efi").is_dir() {
        Firmware::Uefi
    } else {
        Firmware::Bios
    }
}

/// Memoria total, en bytes.
///
/// `MemTotal` de `/proc/meminfo` viene en kibibytes con la unidad escrita al
/// lado (`MemTotal:  16311512 kB`), y ese `kB` son kibibytes aunque diga `kB`.
pub fn memoria_total() -> u64 {
    let Ok(contenido) = fs::read_to_string("/proc/meminfo") else {
        return 0;
    };
    contenido
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|resto| resto.split_whitespace().next())
        .and_then(|n| n.parse::<u64>().ok())
        .map(|kib| kib * 1024)
        .unwrap_or(0)
}

/// Modelo de CPU y cantidad de hilos.
pub fn cpu() -> (String, usize) {
    let Ok(contenido) = fs::read_to_string("/proc/cpuinfo") else {
        return (String::new(), 0);
    };
    let modelo = contenido
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    // Se cuentan los `processor:`, que son los hilos que ve el kernel. `cpu
    // cores` daría núcleos físicos, que es otro número y no el que importa para
    // decidir cuántas descargas paralelas aguanta.
    let hilos = contenido.lines().filter(|l| l.starts_with("processor")).count();
    (modelo, hilos)
}

/// Si hay una ruta por defecto configurada.
///
/// Se lee `/proc/net/route` en vez de invocar `ip route`: es el mismo dato sin
/// depender de que `iproute2` esté instalado ni de parsear su salida, que cambia
/// de formato entre versiones.
///
/// La ruta por defecto es la que tiene destino `00000000`. La comprobación es de
/// **configuración**, no de conectividad: puede haber ruta y no llegar a
/// internet. Eso lo descubre `pacstrap`, y no se hace acá porque golpear un
/// servidor externo desde el instalador para "probar internet" es una llamada a
/// la red que nadie pidió.
pub fn hay_ruta_por_defecto() -> bool {
    let Ok(contenido) = fs::read_to_string("/proc/net/route") else {
        return false;
    };
    contenido
        .lines()
        .skip(1) // la cabecera
        .any(|linea| {
            let mut campos = linea.split_whitespace();
            let _iface = campos.next();
            matches!(campos.next(), Some("00000000"))
        })
}

/// Si corremos dentro de una máquina virtual.
///
/// `systemd-detect-virt` devuelve `none` y código 1 cuando no hay
/// virtualización, así que un código distinto de cero **no es un error acá**:
/// tratarlo como error dejaba el campo vacío en todo equipo físico.
pub fn detectar_virtualizacion() -> Option<String> {
    let salida = Command::new("systemd-detect-virt").output().ok()?;
    let texto = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    if texto.is_empty() || texto == "none" {
        None
    } else {
        Some(texto)
    }
}

pub fn sondear_sistema() -> Sistema {
    let (cpu_modelo, nucleos) = cpu();
    Sistema {
        firmware: detectar_firmware(),
        memoria_bytes: memoria_total(),
        cpu: cpu_modelo,
        nucleos,
        hay_red: hay_ruta_por_defecto(),
        virtualizacion: detectar_virtualizacion(),
    }
}

// ── Discos ──────────────────────────────────────────────────────────────────

/// La forma de la salida de `lsblk --json`, con sólo los campos que se piden.
///
/// Se declara la estructura en vez de andar indexando `serde_json::Value` para
/// que un cambio de nombre de campo en `lsblk` sea un error de parseo con
/// nombre, y no un `None` silencioso a mitad de la interfaz.
#[derive(Debug, Deserialize)]
struct SalidaLsblk {
    #[serde(default)]
    blockdevices: Vec<NodoLsblk>,
}

#[derive(Debug, Deserialize)]
struct NodoLsblk {
    path: Option<String>,
    /// El nombre del kernel del dispositivo padre (`sda` para `/dev/sda1`), o
    /// `null` en un disco. **Es así como se asocian las particiones con su
    /// disco**, y no por el anidamiento — ver `sondear_discos`.
    pkname: Option<String>,
    #[serde(rename = "type")]
    tipo: Option<String>,
    /// Con `--bytes` es un número. Sin `--bytes` es `"238,5G"`, y de ahí no se
    /// puede calcular nada.
    size: Option<u64>,
    model: Option<String>,
    rota: Option<bool>,
    #[serde(rename = "log-sec")]
    log_sec: Option<u64>,
    fstype: Option<String>,
    label: Option<String>,
    /// Una lista, con entradas `null` para los puntos que no están montados.
    #[serde(default)]
    mountpoints: Vec<Option<String>>,
    /// Sólo viene con `--tree`. Se contempla igual para que la misma función
    /// sirva con las dos formas de salida.
    #[serde(default)]
    children: Vec<NodoLsblk>,
}

impl NodoLsblk {
    fn montado(&self) -> bool {
        self.mountpoints.iter().any(|m| m.is_some())
    }

    /// El nombre del kernel: `sda` para `/dev/sda`.
    fn nombre(&self) -> Option<&str> {
        self.path.as_deref()?.rsplit('/').next()
    }
}

/// Los campos que se le piden a `lsblk`. En una constante para que la lista de
/// campos y la estructura de arriba se lean juntas.
const CAMPOS_LSBLK: &str = "PATH,PKNAME,TYPE,SIZE,MODEL,ROTA,LOG-SEC,FSTYPE,LABEL,MOUNTPOINTS";

/// Prefijos de dispositivos que `lsblk` informa como `disk` y no son discos.
///
/// `zram` es el más importante: con la memoria de intercambio comprimida
/// activada —que es lo que VasakOS instala por defecto— aparece un `/dev/zram0`
/// de tipo `disk` en la lista, y sin este filtro se ofrecía como destino de
/// instalación. `loop` es el squashfs de la propia ISO montado. `sr` son las
/// unidades ópticas y `fd` las disqueteras, que existen todavía en máquinas
/// virtuales.
const PSEUDO_DISCOS: &[&str] = &["/dev/zram", "/dev/loop", "/dev/ram", "/dev/sr", "/dev/fd"];

/// Enumera los discos del equipo, con sus particiones.
///
/// **Las particiones se asocian por `PKNAME`, no por anidamiento.** `lsblk`
/// anida los hijos dentro del disco sólo cuando se le pasa `--tree`; sin él, en
/// util-linux 2.42, devuelve una lista plana donde las particiones son hermanas
/// de su propio disco. Confiar en `children` hacía que todo disco apareciera sin
/// particiones — y con eso se caía la comprobación de «está en uso», que es la
/// que impide formatear el medio del que se arrancó, porque el disco no está
/// montado: lo que se monta es su partición.
///
/// Se contemplan las dos formas igual: si una versión futura vuelve a anidar,
/// el aplanado de abajo la absorbe sin cambiar nada.
pub fn sondear_discos() -> Result<Vec<Disco>, String> {
    let salida = Command::new("lsblk")
        .args(["--bytes", "--json", "--output", CAMPOS_LSBLK])
        .output()
        .map_err(|e| format!("no se pudo ejecutar lsblk: {e}"))?;

    if !salida.status.success() {
        return Err(format!(
            "lsblk falló: {}",
            String::from_utf8_lossy(&salida.stderr).trim()
        ));
    }

    let parseada: SalidaLsblk = serde_json::from_slice(&salida.stdout)
        .map_err(|e| format!("no se entendió la salida de lsblk: {e}"))?;

    let mut planos = Vec::new();
    for nodo in &parseada.blockdevices {
        aplanar(nodo, &mut planos);
    }

    Ok(planos
        .iter()
        .filter(|n| n.tipo.as_deref() == Some("disk"))
        .filter(|n| {
            let ruta = n.path.as_deref().unwrap_or("");
            !PSEUDO_DISCOS.iter().any(|p| ruta.starts_with(p))
        })
        .map(|disco| convertir_disco(disco, &planos))
        .collect())
}

fn aplanar<'a>(nodo: &'a NodoLsblk, salida: &mut Vec<&'a NodoLsblk>) {
    salida.push(nodo);
    for hijo in &nodo.children {
        aplanar(hijo, salida);
    }
}

fn convertir_disco(nodo: &NodoLsblk, todos: &[&NodoLsblk]) -> Disco {
    let ruta = nodo.path.clone().unwrap_or_default();
    let nombre = nodo.nombre().unwrap_or_default();

    // Los descendientes del disco: sus particiones y, a través de ellas, los
    // volúmenes LVM y los mapeos de LUKS que cuelguen de una partición. Se
    // recorre en dos niveles porque una partición cifrada monta su contenido a
    // través de un `dm-*` cuyo padre es la partición, no el disco: sin ese
    // segundo nivel, un disco cifrado y montado no aparecía «en uso».
    let hijos_directos: Vec<&&NodoLsblk> = todos
        .iter()
        .filter(|n| n.pkname.as_deref() == Some(nombre))
        .collect();
    let nietos: Vec<&&NodoLsblk> = hijos_directos
        .iter()
        .flat_map(|hijo| {
            let nombre_hijo = hijo.nombre().unwrap_or_default().to_string();
            todos
                .iter()
                .filter(move |n| n.pkname.as_deref() == Some(nombre_hijo.as_str()))
        })
        .collect();

    // El disco está en uso si él o cualquiera de sus descendientes está
    // montado. Mirar sólo el disco deja pasar el pendrive de la ISO, que nunca
    // está montado él mismo.
    let en_uso = nodo.montado()
        || hijos_directos.iter().any(|h| h.montado())
        || nietos.iter().any(|n| n.montado());

    let particiones = hijos_directos
        .iter()
        .filter(|h| h.tipo.as_deref() == Some("part"))
        .map(|h| ParticionExistente {
            ruta: h.path.clone().unwrap_or_default(),
            tamano_bytes: h.size.unwrap_or(0),
            sistema_archivos: h.fstype.clone(),
            etiqueta: h.label.clone(),
            sistema_operativo: None,
        })
        .collect();

    Disco {
        // NVMe no informa `model` en algunos firmwares y quedaba una tarjeta con
        // el nombre vacío. La ruta como respaldo es fea pero identifica.
        modelo: nodo
            .model
            .clone()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| ruta.clone()),
        ruta,
        tamano_bytes: nodo.size.unwrap_or(0),
        // 512 por defecto: es lo que tiene el 99% de los discos, y un `None`
        // acá venía de que la versión de `lsblk` no soportara `LOG-SEC`, no de
        // que el disco fuera raro.
        sector_logico: nodo.log_sec.unwrap_or(512),
        rotacional: nodo.rota.unwrap_or(false),
        // Por la ruta y no por el transporte: `lsblk` informa `nvme` en `TRAN`
        // sólo para el controlador, no para el namespace, que es el que se
        // particiona.
        nvme: nodo.path.as_deref().is_some_and(|p| p.contains("/nvme")),
        en_uso,
        particiones,
    }
}

/// Busca sistemas operativos instalados y los anota en las particiones.
///
/// Es lo que permite que el resumen diga «vas a borrar un Windows 11» en vez de
/// «vas a borrar una partición ntfs». Necesita root, así que corre en el
/// ayudante.
///
/// Best-effort de punta a punta: `os-prober` monta particiones ajenas para
/// mirarlas, tarda, y falla seguido. Si no está o falla, las particiones quedan
/// sin anotar y el resumen dice lo que sabe.
pub fn anotar_sistemas_operativos(discos: &mut [Disco]) {
    let Ok(salida) = Command::new("os-prober").output() else {
        return;
    };
    if !salida.status.success() {
        return;
    }

    // Cada línea es `ruta:nombre largo:etiqueta corta:tipo`, con `:` como
    // separador. El nombre puede tener `:` adentro, así que se parte por la
    // izquierda una sola vez y el resto se toma como el nombre hasta el
    // siguiente separador — que es exactamente lo que hace os-prober al armarlo.
    let texto = String::from_utf8_lossy(&salida.stdout);
    for linea in texto.lines() {
        let mut partes = linea.splitn(4, ':');
        let (Some(ruta), Some(nombre)) = (partes.next(), partes.next()) else {
            continue;
        };
        for disco in discos.iter_mut() {
            for particion in disco.particiones.iter_mut() {
                if particion.ruta == ruta {
                    particion.sistema_operativo = Some(nombre.to_string());
                }
            }
        }
    }
}

// ── Catálogos del sistema: zonas, idiomas y teclados ────────────────────────

/// Las zonas horarias que conoce el sistema.
///
/// Se lee `zone1970.tab` y no se recorre el árbol de `/usr/share/zoneinfo`
/// porque el árbol tiene además los enlaces de compatibilidad (`US/Eastern`,
/// `Brazil/East`), las zonas `posix/` y `right/` duplicadas, y `leapseconds`.
/// Un desplegable con eso adentro tiene tres entradas para la misma ciudad.
///
/// Si el archivo no está se recorre el árbol como respaldo: es peor lista, pero
/// una lista.
pub fn zonas_horarias() -> Vec<String> {
    if let Ok(contenido) = fs::read_to_string("/usr/share/zoneinfo/zone1970.tab") {
        let mut zonas: BTreeSet<String> = BTreeSet::new();
        for linea in contenido.lines() {
            if linea.starts_with('#') || linea.trim().is_empty() {
                continue;
            }
            // Columnas separadas por tabulación: códigos de país, coordenadas,
            // nombre de la zona, comentario.
            if let Some(zona) = linea.split('\t').nth(2) {
                zonas.insert(zona.trim().to_string());
            }
        }
        if !zonas.is_empty() {
            return zonas.into_iter().collect();
        }
    }
    zonas_recorriendo_el_arbol()
}

fn zonas_recorriendo_el_arbol() -> Vec<String> {
    // Sólo las regiones reales. `posix` y `right` son copias del mismo árbol con
    // otra interpretación de los segundos intercalares, y `SystemV` son alias.
    const REGIONES: &[&str] = &[
        "Africa", "America", "Antarctica", "Arctic", "Asia", "Atlantic", "Australia", "Europe",
        "Indian", "Pacific",
    ];
    let mut zonas = BTreeSet::new();
    for region in REGIONES {
        let base = std::path::Path::new("/usr/share/zoneinfo").join(region);
        recorrer_zonas(&base, region, &mut zonas);
    }
    zonas.insert("UTC".to_string());
    zonas.into_iter().collect()
}

fn recorrer_zonas(dir: &std::path::Path, prefijo: &str, salida: &mut BTreeSet<String>) {
    let Ok(entradas) = fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let nombre = entrada.file_name().to_string_lossy().to_string();
        let completo = format!("{prefijo}/{nombre}");
        match entrada.file_type() {
            Ok(t) if t.is_dir() => recorrer_zonas(&entrada.path(), &completo, salida),
            Ok(_) => {
                salida.insert(completo);
            }
            Err(_) => {}
        }
    }
}

/// Los locales UTF-8 que soporta el sistema, sin la codificación.
///
/// `SUPPORTED` trae `es_AR.UTF-8 UTF-8` y también `es_AR ISO-8859-1`. Se
/// quedan sólo los UTF-8 y se les saca el sufijo, porque archinstall pide el
/// idioma y la codificación en dos campos separados.
pub fn idiomas() -> Vec<String> {
    let Ok(contenido) = fs::read_to_string("/usr/share/i18n/SUPPORTED") else {
        return Vec::new();
    };
    let mut lista: BTreeSet<String> = BTreeSet::new();
    for linea in contenido.lines() {
        let Some((local, codificacion)) = linea.split_once(' ') else {
            continue;
        };
        if codificacion.trim() != "UTF-8" {
            continue;
        }
        lista.insert(local.trim_end_matches(".UTF-8").to_string());
    }
    lista.into_iter().collect()
}

/// Los mapas de teclado de consola.
///
/// Son los que consume archinstall: su `kb_layout` termina en `KEYMAP` de
/// `/etc/vconsole.conf`, que es lo que carga `loadkeys`. **No** son los mismos
/// nombres que los diseños de XKB que usa el escritorio: en consola el
/// latinoamericano es `la-latin1` y en XKB es `latam`. La traducción entre los
/// dos está en `teclado.rs`.
pub fn teclados() -> Vec<String> {
    let mut lista = BTreeSet::new();
    recorrer_teclados(std::path::Path::new("/usr/share/kbd/keymaps"), &mut lista);
    lista.into_iter().collect()
}

fn recorrer_teclados(dir: &std::path::Path, salida: &mut BTreeSet<String>) {
    let Ok(entradas) = fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let ruta = entrada.path();
        if ruta.is_dir() {
            recorrer_teclados(&ruta, salida);
            continue;
        }
        // Los mapas son `nombre.map.gz`. `include/` trae fragmentos con la misma
        // extensión que no son mapas cargables, y ofrecerlos hace que `loadkeys`
        // falle con un error de sintaxis.
        let nombre = entrada.file_name().to_string_lossy().to_string();
        if !nombre.ends_with(".map.gz") {
            continue;
        }
        if ruta.components().any(|c| c.as_os_str() == "include") {
            continue;
        }
        salida.insert(nombre.trim_end_matches(".map.gz").to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_lee_la_memoria_de_este_equipo() {
        // Sin valor esperado: lo que se comprueba es que el parseo funcione
        // contra el /proc/meminfo real, que es donde estaba el bug —el `kB` del
        // final hacía fallar el parse del número si se tomaba la línea entera.
        let m = memoria_total();
        assert!(m > 64 * 1024 * 1024, "salió {m} bytes de memoria total");
    }

    #[test]
    fn se_lee_la_cpu_de_este_equipo() {
        let (modelo, hilos) = cpu();
        assert!(!modelo.is_empty(), "el modelo de CPU salió vacío");
        assert!(hilos >= 1, "salieron {hilos} hilos");
    }

    /// Contra el `lsblk` de verdad: la estructura de deserialización tiene que
    /// coincidir con los nombres que devuelve la versión instalada. Cuando
    /// `mountpoints` pasó de cadena a lista, esto es lo que lo habría agarrado.
    #[test]
    fn lsblk_se_parsea_en_este_equipo() {
        let discos = sondear_discos().expect("lsblk tendría que andar acá");
        for d in &discos {
            assert!(d.ruta.starts_with("/dev/"), "ruta rara: {}", d.ruta);
            assert!(d.tamano_bytes > 0, "{} informó tamaño cero", d.ruta);
            assert!(
                d.sector_logico == 512 || d.sector_logico == 4096,
                "{} informó sector {}",
                d.ruta,
                d.sector_logico
            );
            assert!(!d.modelo.is_empty(), "{} salió sin modelo", d.ruta);
        }
    }

    /// Ningún dispositivo que no sea un disco de verdad se puede colar como
    /// destino. `zram` es el caso real: `lsblk` lo informa con tipo `disk`, y
    /// VasakOS activa la memoria de intercambio comprimida por defecto, así que
    /// aparece en todo equipo instalado.
    #[test]
    fn el_sondeo_no_devuelve_pseudodiscos() {
        for d in sondear_discos().unwrap() {
            for prefijo in ["/dev/loop", "/dev/zram", "/dev/ram", "/dev/sr", "/dev/fd"] {
                assert!(!d.ruta.starts_with(prefijo), "se coló {}", d.ruta);
            }
            assert_ne!(d.ruta.find("/dm-"), Some(4), "se coló {}", d.ruta);
        }
    }

    /// El disco montado tiene que salir marcado en uso y con sus particiones.
    ///
    /// Éste es el test que agarró el bug serio: `lsblk --json` **sin `--tree`**
    /// devuelve las particiones como hermanas del disco, no anidadas, así que
    /// leer `children` daba todo disco sin particiones. Con eso, el disco del
    /// que se arrancó no figuraba en uso —el disco no está montado, lo está su
    /// partición— y el instalador lo habría ofrecido para formatear.
    #[test]
    fn el_disco_montado_sale_en_uso_y_con_sus_particiones() {
        let discos = sondear_discos().unwrap();
        // El equipo donde corre el test tiene su raíz en algún disco, así que
        // tiene que haber al menos uno en uso.
        let en_uso: Vec<&Disco> = discos.iter().filter(|d| d.en_uso).collect();
        assert!(
            !en_uso.is_empty(),
            "ningún disco salió en uso, y este equipo está arrancado de uno: {discos:#?}"
        );
        for d in en_uso {
            assert!(
                !d.particiones.is_empty(),
                "{} salió en uso pero sin particiones",
                d.ruta
            );
            // Y cada partición pertenece a su disco, no a otro.
            for p in &d.particiones {
                assert!(
                    p.ruta.starts_with(&d.ruta),
                    "{} no es partición de {}",
                    p.ruta,
                    d.ruta
                );
            }
        }
    }

    #[test]
    fn el_firmware_es_uno_de_los_dos() {
        // No se puede afirmar cuál en una máquina cualquiera; lo que importa es
        // que no paniquee ni devuelva algo raro.
        let f = detectar_firmware();
        assert!(matches!(f, Firmware::Uefi | Firmware::Bios));
    }

    #[test]
    fn las_zonas_horarias_salen_sin_duplicados_ni_alias() {
        let zonas = zonas_horarias();
        assert!(zonas.len() > 100, "salieron {} zonas", zonas.len());

        let mut unicas = zonas.clone();
        unicas.dedup();
        assert_eq!(unicas.len(), zonas.len(), "hay zonas repetidas");

        assert!(zonas.iter().any(|z| z == "America/Argentina/Buenos_Aires"));
        // Los alias de compatibilidad no van: con ellos hay tres entradas para
        // la misma ciudad en el desplegable.
        assert!(!zonas.iter().any(|z| z.starts_with("posix/")), "{zonas:?}");
        assert!(!zonas.iter().any(|z| z.starts_with("right/")));
        assert!(!zonas.iter().any(|z| z == "leapseconds"));
        assert!(!zonas.iter().any(|z| z.ends_with(".tab")));
    }

    #[test]
    fn los_idiomas_salen_sin_la_codificacion() {
        let lista = idiomas();
        if lista.is_empty() {
            // `/usr/share/i18n/SUPPORTED` viene con glibc; si no está, el
            // respaldo es una lista vacía y la interfaz deja escribir a mano.
            return;
        }
        assert!(lista.iter().any(|l| l == "es_AR"), "no está es_AR");
        assert!(lista.iter().any(|l| l == "en_US"), "no está en_US");
        // Sin sufijo: archinstall pide el idioma y la codificación por separado,
        // y `es_AR.UTF-8` en `sys_lang` produce un `locale.gen` con la línea
        // duplicada.
        assert!(
            !lista.iter().any(|l| l.contains(".UTF-8")),
            "quedó la codificación pegada"
        );
    }

    #[test]
    fn los_teclados_no_incluyen_los_fragmentos_de_include() {
        let lista = teclados();
        if lista.is_empty() {
            return; // sin `kbd` instalado
        }
        assert!(lista.iter().any(|t| t == "us"), "no está us");
        // Los fragmentos de `include/` tienen la misma extensión y no son mapas
        // cargables: ofrecerlos hace que `loadkeys` falle con un error de
        // sintaxis en medio de la instalación.
        for sospechoso in ["linux-keys-bare", "compose.latin1", "euro"] {
            assert!(
                !lista.iter().any(|t| t == sospechoso),
                "se coló el fragmento {sospechoso}"
            );
        }
    }
}
