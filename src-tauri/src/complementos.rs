//! Lo que se puede sumar al sistema durante la instalación.
//!
//! Un complemento es un conjunto de paquetes y servicios con un nombre que una
//! persona entiende: «Firefox», «Impresoras», «Controlador de NVIDIA». La
//! interfaz muestra los nombres; esto los convierte en las listas que espera
//! archinstall.
//!
//! El catálogo vive en `complementos.toml` y se lee en tiempo de ejecución, por
//! la misma razón que `paquetes.txt`: **sumar un navegador tiene que ser editar
//! un archivo de datos, no recompilar el instalador ni rehacer la ISO.**
//!
//! Los tres casos que motivaron esto —elegir navegador, instalar controladores
//! según el hardware, y las impresoras— son la misma forma: paquetes opcionales
//! más servicios opcionales, elegidos en la interfaz. Resolverlos con un
//! mecanismo y no con tres es lo que evita que el cuarto vuelva a empezar de
//! cero.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Un complemento del catálogo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Complemento {
    /// Identificador estable. Va en el plan y en las claves de traducción.
    pub id: String,
    pub categoria: Categoria,
    #[serde(default)]
    pub paquetes: Vec<String>,
    /// Unidades de **sistema**. Las de usuario las habilitan sus paquetes; un
    /// `systemctl enable` sin `--user` no las encuentra y falla la instalación.
    #[serde(default)]
    pub servicios: Vec<String>,
    /// Nombre del tema de iconos, sin el sufijo `-symbolic`.
    #[serde(default)]
    pub icono: String,
    /// Marca de hardware que lo propone. Ver `hardware.rs`.
    #[serde(default)]
    pub detectar: Option<String>,
    /// Los exclusivos de una misma categoría son un grupo de uno solo.
    #[serde(default)]
    pub exclusivo: bool,
    /// Viene marcado de entrada.
    #[serde(default)]
    pub por_defecto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Categoria {
    Navegador,
    Impresoras,
    Drivers,
    Extras,
}

impl Categoria {
    /// El orden en que se muestran las categorías.
    ///
    /// Navegador primero porque es el que todo el mundo va a mirar; los
    /// controladores después, porque es donde hay algo detectado que explicar; y
    /// los extras al final, que son los que más se pueden dejar para más
    /// adelante desde el sistema instalado.
    pub const TODAS: &'static [Categoria] = &[
        Categoria::Navegador,
        Categoria::Drivers,
        Categoria::Impresoras,
        Categoria::Extras,
    ];

    /// La clave del catálogo de idioma.
    pub fn clave(self) -> &'static str {
        match self {
            Categoria::Navegador => "navegador",
            Categoria::Impresoras => "impresoras",
            Categoria::Drivers => "drivers",
            Categoria::Extras => "extras",
        }
    }
}

#[derive(Debug, Deserialize)]
struct Archivo {
    #[serde(default, rename = "complemento")]
    complementos: Vec<Complemento>,
}

/// Errores del catálogo que hacen que no se pueda usar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCatalogo {
    NoSeEncontro,
    NoSePudoLeer(String),
    NoSeEntiende(String),
    /// Dos complementos con el mismo `id`. El plan los nombra por id, así que un
    /// duplicado hace que elegir uno instale el otro.
    IdRepetido(String),
}

impl std::fmt::Display for ErrorCatalogo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCatalogo::NoSeEncontro => write!(f, "no se encontró complementos.toml"),
            ErrorCatalogo::NoSePudoLeer(e) => write!(f, "no se pudo leer complementos.toml: {e}"),
            ErrorCatalogo::NoSeEntiende(e) => write!(f, "complementos.toml tiene un error: {e}"),
            ErrorCatalogo::IdRepetido(id) => {
                write!(f, "hay dos complementos con el identificador «{id}»")
            }
        }
    }
}

/// Dónde busca el catálogo, con el mismo orden que `paquetes.txt`.
pub fn ruta() -> Option<PathBuf> {
    let candidatas = [
        PathBuf::from("src-tauri/complementos.toml"),
        PathBuf::from("complementos.toml"),
        PathBuf::from("/usr/share/vasak-installer/complementos.toml"),
    ];
    candidatas.into_iter().find(|c| c.is_file())
}

/// Parsea el catálogo.
pub fn parsear(contenido: &str) -> Result<Vec<Complemento>, ErrorCatalogo> {
    let archivo: Archivo =
        toml::from_str(contenido).map_err(|e| ErrorCatalogo::NoSeEntiende(e.to_string()))?;

    // Los ids se comprueban acá y no al usarlos: un duplicado hace que elegir un
    // complemento instale los paquetes del otro, y eso se descubre mirando qué
    // quedó instalado, que es tardísimo.
    let mut vistos = BTreeSet::new();
    for c in &archivo.complementos {
        if !vistos.insert(c.id.clone()) {
            return Err(ErrorCatalogo::IdRepetido(c.id.clone()));
        }
    }

    Ok(archivo.complementos)
}

pub fn cargar() -> Result<Vec<Complemento>, ErrorCatalogo> {
    let ruta = ruta().ok_or(ErrorCatalogo::NoSeEncontro)?;
    let contenido =
        std::fs::read_to_string(&ruta).map_err(|e| ErrorCatalogo::NoSePudoLeer(e.to_string()))?;
    parsear(&contenido)
}

/// Lo que hay que sumarle a la configuración de archinstall.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aporte {
    pub paquetes: Vec<String>,
    pub servicios: Vec<String>,
}

/// Junta los paquetes y servicios de los complementos elegidos.
///
/// Los ids que no existen en el catálogo se **ignoran en silencio**. Es
/// deliberado: el catálogo se puede editar entre que la interfaz lo leyó y que
/// se arma el plan, y un id viejo no puede impedir una instalación que por lo
/// demás está bien. Lo que no se puede hacer es instalar algo que no se pidió.
///
/// Sin duplicados y en orden estable: `pacman` no se queja de un paquete
/// repetido, pero dos ejecuciones con la misma elección tienen que producir el
/// mismo archivo para poder compararlos cuando algo falla.
pub fn aporte_de(catalogo: &[Complemento], elegidos: &[String]) -> Aporte {
    let pedidos: BTreeSet<&str> = elegidos.iter().map(String::as_str).collect();

    let mut paquetes = BTreeSet::new();
    let mut servicios = BTreeSet::new();

    for c in catalogo {
        if !pedidos.contains(c.id.as_str()) {
            continue;
        }
        paquetes.extend(c.paquetes.iter().cloned());
        servicios.extend(c.servicios.iter().cloned());
    }

    Aporte {
        paquetes: paquetes.into_iter().collect(),
        servicios: servicios.into_iter().collect(),
    }
}

/// Qué complementos vienen marcados al abrir el paso.
///
/// Los `por_defecto` siempre, y los que el hardware detectado propone. **Nada
/// más**: un instalador que deja marcado lo que se le ocurre instala un sistema
/// que nadie eligió.
///
/// Entre los exclusivos de una categoría queda uno solo — el primero del
/// catálogo que califique. Dos marcados en un grupo del que se elige uno es un
/// estado que la interfaz no puede representar.
pub fn preseleccion(catalogo: &[Complemento], hardware: &BTreeSet<String>) -> Vec<String> {
    let mut elegidos: Vec<String> = Vec::new();
    let mut categorias_exclusivas_tomadas = BTreeSet::new();

    for c in catalogo {
        let propuesto = c.por_defecto
            || c.detectar
                .as_deref()
                .is_some_and(|marca| hardware.contains(marca));
        if !propuesto {
            continue;
        }

        // Entre los exclusivos de una categoría queda el primero del catálogo que
        // califique: `insert` devuelve `false` si esa categoría ya fue tomada.
        if c.exclusivo && !categorias_exclusivas_tomadas.insert(c.categoria) {
            continue;
        }
        elegidos.push(c.id.clone());
    }

    elegidos
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogo_de_prueba() -> Vec<Complemento> {
        parsear(
            r#"
[[complemento]]
id = "firefox"
categoria = "navegador"
paquetes = ["firefox"]
exclusivo = true
por_defecto = true

[[complemento]]
id = "chromium"
categoria = "navegador"
paquetes = ["chromium"]
exclusivo = true

[[complemento]]
id = "impresoras"
categoria = "impresoras"
paquetes = ["cups", "hplip"]
servicios = ["cups.socket", "avahi-daemon"]

[[complemento]]
id = "nvidia"
categoria = "drivers"
paquetes = ["nvidia-dkms"]
detectar = "gpu-nvidia"

[[complemento]]
id = "amd"
categoria = "drivers"
paquetes = ["vulkan-radeon"]
detectar = "gpu-amd"
"#,
        )
        .unwrap()
    }

    #[test]
    fn el_catalogo_real_parsea() {
        let contenido = include_str!("../complementos.toml");
        let catalogo = parsear(contenido).expect("complementos.toml tiene que parsear");

        assert!(catalogo.len() > 5, "salieron {} complementos", catalogo.len());

        for c in &catalogo {
            assert!(!c.id.is_empty());
            // El icono se dibuja al lado del nombre; sin él queda un hueco.
            assert!(!c.icono.is_empty(), "«{}» no tiene icono", c.id);
            // Ningún nombre de paquete con espacios ni con `#`: eso sería basura
            // que `pacstrap` rechaza con «target not found».
            for p in &c.paquetes {
                assert!(!p.contains(' '), "«{p}» en «{}» tiene un espacio", c.id);
                assert!(!p.is_empty());
            }
        }
    }

    /// Ningún complemento puede traer lo que hace falta para arrancar.
    ///
    /// Si algo imprescindible viviera acá, desmarcarlo produciría un sistema que
    /// no enciende — y la interfaz presenta todo esto como opcional.
    #[test]
    fn ningun_complemento_es_imprescindible() {
        let catalogo = parsear(include_str!("../complementos.toml")).unwrap();
        let imprescindibles = [
            "vasakos-desktop",
            "vasak-desktop",
            "vasak-session-manager",
            "greetd",
            "wayfire",
            "grub",
            "linux",
            "networkmanager",
        ];
        for c in &catalogo {
            for p in &c.paquetes {
                assert!(
                    !imprescindibles.contains(&p.as_str()),
                    "«{p}» hace falta para arrancar y está en el complemento «{}»",
                    c.id
                );
            }
        }
    }

    /// Los servicios de usuario no se pueden habilitar desde acá.
    ///
    /// archinstall corre `systemctl enable` sin `--user`, así que no los
    /// encuentra y la instalación falla en el paso de servicios.
    #[test]
    fn los_servicios_son_de_sistema() {
        let catalogo = parsear(include_str!("../complementos.toml")).unwrap();
        for c in &catalogo {
            for s in &c.servicios {
                assert!(!s.contains("--user"), "«{s}» en «{}»", c.id);
                // Un servicio de usuario típico del escritorio: si aparece acá es
                // que alguien confundió los dos tipos.
                assert!(
                    !s.starts_with("vasak-idle") && !s.starts_with("vasak-nightlight"),
                    "«{s}» es un servicio de usuario"
                );
            }
        }
    }

    #[test]
    fn un_id_repetido_se_rechaza() {
        // Un duplicado hace que elegir un complemento instale los paquetes del
        // otro, y eso se descubre mirando qué quedó instalado.
        let error = parsear(
            r#"
[[complemento]]
id = "firefox"
categoria = "navegador"

[[complemento]]
id = "firefox"
categoria = "extras"
"#,
        )
        .unwrap_err();
        assert_eq!(error, ErrorCatalogo::IdRepetido("firefox".into()));
    }

    #[test]
    fn el_aporte_junta_paquetes_y_servicios() {
        let catalogo = catalogo_de_prueba();
        let aporte = aporte_de(&catalogo, &["firefox".into(), "impresoras".into()]);

        assert_eq!(aporte.paquetes, vec!["cups", "firefox", "hplip"]);
        assert_eq!(aporte.servicios, vec!["avahi-daemon", "cups.socket"]);
    }

    #[test]
    fn lo_que_no_se_eligio_no_se_instala() {
        let catalogo = catalogo_de_prueba();
        let aporte = aporte_de(&catalogo, &["firefox".into()]);
        assert!(!aporte.paquetes.contains(&"chromium".to_string()));
        assert!(!aporte.paquetes.contains(&"nvidia-dkms".to_string()));
        assert!(aporte.servicios.is_empty());
    }

    #[test]
    fn un_id_que_no_existe_se_ignora_sin_romper() {
        // El catálogo se puede editar entre que la interfaz lo leyó y que se arma
        // el plan. Un id viejo no puede impedir una instalación que por lo demás
        // está bien.
        let catalogo = catalogo_de_prueba();
        let aporte = aporte_de(&catalogo, &["firefox".into(), "ya-no-existe".into()]);
        assert_eq!(aporte.paquetes, vec!["firefox"]);
    }

    #[test]
    fn sin_nada_elegido_no_se_suma_nada() {
        assert_eq!(aporte_de(&catalogo_de_prueba(), &[]), Aporte::default());
    }

    #[test]
    fn el_aporte_no_repite_paquetes() {
        // Dos complementos pueden compartir un paquete: `avahi` lo quieren las
        // impresoras y el escaneo.
        let catalogo = parsear(
            r#"
[[complemento]]
id = "uno"
categoria = "extras"
paquetes = ["avahi", "cups"]

[[complemento]]
id = "dos"
categoria = "extras"
paquetes = ["avahi", "sane"]
"#,
        )
        .unwrap();
        let aporte = aporte_de(&catalogo, &["uno".into(), "dos".into()]);
        assert_eq!(aporte.paquetes, vec!["avahi", "cups", "sane"]);
    }

    #[test]
    fn el_hardware_detectado_propone_su_controlador() {
        let catalogo = catalogo_de_prueba();
        let hardware: BTreeSet<String> = ["gpu-nvidia".to_string()].into_iter().collect();
        let elegidos = preseleccion(&catalogo, &hardware);

        assert!(elegidos.contains(&"nvidia".to_string()), "{elegidos:?}");
        // Y no el de la placa que no está.
        assert!(!elegidos.contains(&"amd".to_string()), "{elegidos:?}");
    }

    #[test]
    fn sin_hardware_detectado_solo_van_los_de_por_defecto() {
        let elegidos = preseleccion(&catalogo_de_prueba(), &BTreeSet::new());
        assert_eq!(elegidos, vec!["firefox"]);
    }

    /// En un grupo exclusivo no puede quedar más de uno marcado: es un estado
    /// que la interfaz —un grupo de opciones de las que se elige una— no puede
    /// representar.
    #[test]
    fn entre_los_exclusivos_queda_uno_solo() {
        let catalogo = parsear(
            r#"
[[complemento]]
id = "firefox"
categoria = "navegador"
exclusivo = true
por_defecto = true

[[complemento]]
id = "chromium"
categoria = "navegador"
exclusivo = true
por_defecto = true
"#,
        )
        .unwrap();
        let elegidos = preseleccion(&catalogo, &BTreeSet::new());
        assert_eq!(elegidos, vec!["firefox"]);
    }

    /// El catálogo real tiene exactamente un navegador marcado de entrada.
    #[test]
    fn el_catalogo_real_propone_un_solo_navegador() {
        let catalogo = parsear(include_str!("../complementos.toml")).unwrap();
        let elegidos = preseleccion(&catalogo, &BTreeSet::new());

        let navegadores: Vec<&Complemento> = catalogo
            .iter()
            .filter(|c| c.categoria == Categoria::Navegador && elegidos.contains(&c.id))
            .collect();
        assert_eq!(navegadores.len(), 1, "{navegadores:?}");
    }

    /// Cada categoría del enum tiene que existir en el catálogo real, y cada
    /// categoría del catálogo tiene que estar en `TODAS`.
    ///
    /// Una categoría que no está en `TODAS` no se dibuja: sus complementos
    /// existen, se pueden elegir desde el plan, y no aparecen en ninguna
    /// pantalla.
    #[test]
    fn todas_las_categorias_se_dibujan() {
        let catalogo = parsear(include_str!("../complementos.toml")).unwrap();
        let usadas: BTreeSet<Categoria> = catalogo.iter().map(|c| c.categoria).collect();
        for categoria in &usadas {
            assert!(
                Categoria::TODAS.contains(categoria),
                "{categoria:?} no está en TODAS y no se dibujaría"
            );
        }
        for categoria in Categoria::TODAS {
            assert!(
                usadas.contains(categoria),
                "{categoria:?} está en TODAS y no la usa ningún complemento"
            );
        }
    }
}
