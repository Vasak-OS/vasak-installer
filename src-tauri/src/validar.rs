//! Validación de lo que se escribe a mano: nombre de equipo y de usuario.
//!
//! Está en Rust y no en el frontend a propósito. La misma comprobación se hace
//! dos veces —el frontend para poder decirlo mientras se escribe, y el backend
//! antes de arrancar— pero **la que decide es ésta**: un nombre de usuario
//! inválido no lo rechaza el instalador, lo rechaza `useradd` en medio de la
//! instalación, con el disco ya formateado y un mensaje que nadie va a ver
//! porque está en el registro.
//!
//! Los motivos vuelven como una variante y no como una cadena en español,
//! porque el texto lo pone el frontend desde su catálogo de idioma.

use serde::Serialize;

/// Largo máximo de un nombre de usuario.
///
/// 32 es el límite de `useradd` en glibc (`UT_NAMESIZE`), y no es negociable:
/// más largo se acepta al crearlo en algunos sistemas y después aparece truncado
/// en `who`, en los registros y en los nombres de archivo de las sesiones.
const MAX_USUARIO: usize = 32;

/// Largo máximo de una etiqueta de nombre de equipo, por RFC 1123.
const MAX_ETIQUETA: usize = 63;

/// Nombres que el sistema ya usa.
///
/// Crear un usuario con uno de éstos no falla siempre —`useradd` mira
/// `/etc/passwd`, y en el sistema nuevo todavía no están todos— pero el paquete
/// que después crea el usuario de sistema sí falla, y queda un sistema donde por
/// ejemplo el daemon de red no puede arrancar. Es más barato prohibirlos.
const RESERVADOS: &[&str] = &[
    "root", "bin", "daemon", "mail", "ftp", "http", "nobody", "dbus", "systemd-network",
    "systemd-resolve", "systemd-timesync", "systemd-coredump", "polkitd", "avahi", "colord",
    "rtkit", "usbmux", "geoclue", "nm-openvpn", "greeter", "greetd", "vasak", "vasakos",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "motivo", rename_all = "snake_case")]
pub enum ErrorNombre {
    Vacio,
    /// Más largo que el límite del sistema.
    Largo { maximo: usize },
    /// Tiene un carácter que no se admite. Se devuelve **cuál**: «caracteres
    /// inválidos» sin decir cuál obliga a adivinar, y el sospechoso más común es
    /// un espacio o un acento, que no se ven distintos.
    Caracter { cual: String },
    /// Empieza con algo que no puede ir primero.
    EmpiezaMal,
    /// Termina con algo que no puede ir último.
    TerminaMal,
    /// Es un nombre que el sistema ya usa.
    Reservado,
}

/// Valida un nombre de usuario contra lo que acepta `useradd`.
///
/// La regla es la de `shadow-utils`: empieza con minúscula o `_`, sigue con
/// minúsculas, dígitos, `_` o `-`. Sin mayúsculas: `useradd` las acepta con
/// `--badname` pero después medio ecosistema las baja a minúscula y el usuario
/// termina teniendo dos nombres.
pub fn nombre_de_usuario(nombre: &str) -> Result<(), ErrorNombre> {
    if nombre.is_empty() {
        return Err(ErrorNombre::Vacio);
    }
    if nombre.len() > MAX_USUARIO {
        return Err(ErrorNombre::Largo { maximo: MAX_USUARIO });
    }

    let primero = nombre.chars().next().expect("no está vacío");
    if !(primero.is_ascii_lowercase() || primero == '_') {
        return Err(ErrorNombre::EmpiezaMal);
    }

    if let Some(malo) = nombre
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-'))
    {
        return Err(ErrorNombre::Caracter {
            cual: malo.to_string(),
        });
    }

    if RESERVADOS.contains(&nombre) {
        return Err(ErrorNombre::Reservado);
    }

    Ok(())
}

/// Valida un nombre de equipo.
///
/// RFC 1123: etiquetas separadas por puntos, cada una con letras, dígitos y
/// guiones, sin empezar ni terminar con guion. Se admiten mayúsculas porque los
/// nombres de host no distinguen, pero el guion al final sí importa: un
/// `hostnamectl` con eso falla y el sistema queda con el nombre por defecto.
pub fn nombre_de_equipo(nombre: &str) -> Result<(), ErrorNombre> {
    if nombre.is_empty() {
        return Err(ErrorNombre::Vacio);
    }
    // 253 es el largo máximo de un nombre de dominio completo. Un equipo no
    // suele acercarse, pero el límite existe y el error es más claro acá que en
    // `hostnamectl`.
    if nombre.len() > 253 {
        return Err(ErrorNombre::Largo { maximo: 253 });
    }

    // `localhost` apunta a 127.0.0.1 en `/etc/hosts`. Un equipo llamado así
    // rompe la resolución de su propio nombre, y con ella todo lo que abre un
    // socket contra sí mismo — que en un escritorio es D-Bus y el portal.
    if nombre.eq_ignore_ascii_case("localhost") {
        return Err(ErrorNombre::Reservado);
    }

    for etiqueta in nombre.split('.') {
        if etiqueta.is_empty() {
            // Un punto al principio, al final, o dos seguidos.
            return Err(ErrorNombre::Caracter { cual: ".".into() });
        }
        if etiqueta.len() > MAX_ETIQUETA {
            return Err(ErrorNombre::Largo { maximo: MAX_ETIQUETA });
        }
        if etiqueta.starts_with('-') {
            return Err(ErrorNombre::EmpiezaMal);
        }
        if etiqueta.ends_with('-') {
            return Err(ErrorNombre::TerminaMal);
        }
        if let Some(malo) = etiqueta
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
        {
            return Err(ErrorNombre::Caracter {
                cual: malo.to_string(),
            });
        }
    }

    Ok(())
}

/// Propone un nombre de usuario a partir del nombre completo.
///
/// Se queda con la primera palabra, le saca los acentos y baja a minúsculas.
/// Sin el paso de los acentos, «Joaquín» proponía `joaquín`, que `useradd`
/// rechaza — y el rechazo aparecía recién al apretar Instalar.
pub fn sugerir_usuario(nombre_completo: &str) -> String {
    let primera = nombre_completo.split_whitespace().next().unwrap_or("");
    let limpio: String = primera
        .chars()
        .map(sin_acento)
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .take(MAX_USUARIO)
        .collect();

    // Un nombre de usuario no puede empezar con un dígito, así que se descartan
    // los de adelante. Sin esto, «1998» sugería `1998` y el campo quedaba
    // autocompletado con algo que la validación rechaza — la persona ve un error
    // en un campo que nunca tocó.
    let sin_digitos_al_frente = limpio.trim_start_matches(|c: char| c.is_ascii_digit());

    // Y si no quedó nada usable, se devuelve vacío: un campo vacío se entiende
    // como «escribilo vos», un campo con basura se entiende como un error.
    if sin_digitos_al_frente.is_empty() || RESERVADOS.contains(&sin_digitos_al_frente) {
        return String::new();
    }
    sin_digitos_al_frente.to_string()
}

/// Quita el acento de las letras que lo llevan en español y portugués.
///
/// Una tabla y no normalización Unicode: traer una dependencia de normalización
/// para diez letras es desproporcionado, y lo que no está en la tabla lo filtra
/// el paso siguiente.
fn sin_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        otro => otro,
    }
}

// ── Región, idioma y teclado ────────────────────────────────────────────────
//
// Se pueden escribir a mano cuando el sistema no pudo dar los catálogos, así
// que hay que comprobarlos contra el sistema antes de instalar. Un valor
// inválido no falla al principio: falla dentro del chroot, cuando `localectl` o
// `loadkeys` lo rechazan, con el disco ya formateado.

/// Que la zona horaria exista en la base de datos de zonas.
///
/// Se comprueba que el archivo esté, y **que esté adentro** de
/// `/usr/share/zoneinfo`: el nombre viene del frontend, y un `../../etc/shadow`
/// se resolvería a un archivo que existe. archinstall lo usa para armar un
/// enlace a `/etc/localtime`.
pub fn zona_horaria(zona: &str) -> Result<(), String> {
    if zona.is_empty() {
        return Err("está vacía".into());
    }
    // Ni rutas absolutas ni tramos que suban: la zona es un nombre relativo
    // dentro de la base de datos, siempre.
    if zona.starts_with('/') || zona.split('/').any(|t| t == "." || t == "..") {
        return Err(format!("«{zona}» no es un nombre de zona"));
    }

    let base = std::path::Path::new("/usr/share/zoneinfo");
    if !base.is_dir() {
        // Sin base de datos no se puede comprobar nada, y rechazar todo dejaría
        // el instalador sin poder instalar. Se acepta lo que se pueda.
        return Ok(());
    }
    if base.join(zona).is_file() {
        Ok(())
    } else {
        Err(format!("«{zona}» no está en la base de zonas horarias"))
    }
}

/// Que el idioma esté entre los que soporta glibc.
///
/// Se compara sin la codificación, que es como lo maneja el resto del
/// instalador: `SUPPORTED` trae `es_AR.UTF-8 UTF-8` y acá se espera `es_AR`.
pub fn idioma(local: &str) -> Result<(), String> {
    if local.is_empty() {
        return Err("está vacío".into());
    }
    // El formato de glibc: `xx_YY`, con variantes como `ca_ES@valencia`. Nada de
    // espacios ni de barras, que es lo que se colaría de un campo libre.
    if local
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '-'))
    {
        return Err(format!("«{local}» tiene caracteres que no van en un local"));
    }

    let Ok(contenido) = std::fs::read_to_string("/usr/share/i18n/SUPPORTED") else {
        return Ok(()); // sin catálogo no se puede comprobar
    };
    let existe = contenido.lines().any(|linea| {
        linea
            .split_whitespace()
            .next()
            .is_some_and(|l| l.trim_end_matches(".UTF-8") == local)
    });
    if existe {
        Ok(())
    } else {
        Err(format!("«{local}» no está entre los idiomas del sistema"))
    }
}

/// Que el mapa de teclado exista entre los de consola.
pub fn teclado(nombre: &str) -> Result<(), String> {
    if nombre.is_empty() {
        return Err("está vacío".into());
    }
    if nombre.contains('/') || nombre.contains("..") {
        return Err(format!("«{nombre}» no es un nombre de mapa de teclado"));
    }

    let mapas = crate::probe::teclados();
    if mapas.is_empty() {
        return Ok(()); // sin `kbd` instalado no se puede comprobar
    }
    if mapas.iter().any(|m| m == nombre) {
        Ok(())
    } else {
        Err(format!("«{nombre}» no está entre los mapas de teclado del sistema"))
    }
}

/// Fuerza de una contraseña, para mostrarla — **no para bloquear**.
///
/// No se bloquea por la misma razón que no lo hacía la configuración de
/// calamares: quien instala su propio equipo tiene derecho a decidir. Lo que sí
/// se hace es decirlo, porque una barra que dice «débil» cambia más contraseñas
/// que un rechazo, que se sortea agregando un `1` al final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fuerza {
    Vacia,
    Debil,
    Aceptable,
    Buena,
}

pub fn fuerza(contrasena: &str) -> Fuerza {
    if contrasena.is_empty() {
        return Fuerza::Vacia;
    }

    // Se cuentan **clases** de caracteres y no reglas de composición. Obligar a
    // «una mayúscula, un número y un símbolo» produce `Password1!`, que es la
    // contraseña más común del mundo; el largo es lo que importa de verdad.
    let clases = [
        contrasena.chars().any(|c| c.is_lowercase()),
        contrasena.chars().any(|c| c.is_uppercase()),
        contrasena.chars().any(|c| c.is_numeric()),
        contrasena.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|x| **x)
    .count();

    let largo = contrasena.chars().count();

    // Una frase larga de una sola clase es mejor que ocho caracteres de cuatro
    // clases, y por eso el largo puede alcanzar «buena» solo.
    match (largo, clases) {
        (l, _) if l >= 16 => Fuerza::Buena,
        (l, c) if l >= 12 && c >= 2 => Fuerza::Buena,
        (l, c) if l >= 8 && c >= 2 => Fuerza::Aceptable,
        (l, _) if l >= 12 => Fuerza::Aceptable,
        _ => Fuerza::Debil,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_nombres_de_usuario_normales_pasan() {
        for bueno in ["pato", "jdecima", "ana_maria", "usuario-1", "_servicio", "a"] {
            assert_eq!(nombre_de_usuario(bueno), Ok(()), "rechazó «{bueno}»");
        }
    }

    #[test]
    fn el_acento_en_el_usuario_se_rechaza_diciendo_cual() {
        // Éste es el caso real: «Joaquín» sugiere `joaquín`, y el rechazo de
        // `useradd` llegaría en medio de la instalación. Y el error dice qué
        // carácter molesta, porque una `í` y una `i` no se distinguen de un
        // vistazo en un campo de texto.
        assert_eq!(
            nombre_de_usuario("joaquín"),
            Err(ErrorNombre::Caracter { cual: "í".into() })
        );
        assert_eq!(
            nombre_de_usuario("ana maria"),
            Err(ErrorNombre::Caracter { cual: " ".into() })
        );
    }

    #[test]
    fn las_mayusculas_en_el_usuario_se_rechazan() {
        // `useradd` las acepta con `--badname`, pero después buena parte del
        // ecosistema las baja a minúscula y el usuario termina con dos nombres.
        assert_eq!(nombre_de_usuario("Pato"), Err(ErrorNombre::EmpiezaMal));
        assert_eq!(
            nombre_de_usuario("paTo"),
            Err(ErrorNombre::Caracter { cual: "T".into() })
        );
    }

    #[test]
    fn un_usuario_que_empieza_con_numero_o_guion_se_rechaza() {
        assert_eq!(nombre_de_usuario("1pato"), Err(ErrorNombre::EmpiezaMal));
        assert_eq!(nombre_de_usuario("-pato"), Err(ErrorNombre::EmpiezaMal));
    }

    #[test]
    fn los_usuarios_del_sistema_estan_prohibidos() {
        for reservado in ["root", "nobody", "greeter", "vasak", "dbus"] {
            assert_eq!(
                nombre_de_usuario(reservado),
                Err(ErrorNombre::Reservado),
                "dejó pasar «{reservado}»"
            );
        }
    }

    #[test]
    fn el_limite_de_32_del_sistema_se_respeta() {
        assert_eq!(nombre_de_usuario(&"a".repeat(32)), Ok(()));
        // 33 se acepta al crearlo en algunos sistemas y después aparece truncado
        // en `who` y en los registros.
        assert_eq!(
            nombre_de_usuario(&"a".repeat(33)),
            Err(ErrorNombre::Largo { maximo: 32 })
        );
    }

    #[test]
    fn los_nombres_de_equipo_normales_pasan() {
        for bueno in ["vasak", "note-de-pato", "PC1", "equipo.casa.lan", "a"] {
            assert_eq!(nombre_de_equipo(bueno), Ok(()), "rechazó «{bueno}»");
        }
    }

    #[test]
    fn localhost_esta_prohibido_como_nombre_de_equipo() {
        // Apunta a 127.0.0.1 en /etc/hosts: un equipo llamado así no resuelve su
        // propio nombre, y con eso se cae D-Bus y el portal.
        assert_eq!(nombre_de_equipo("localhost"), Err(ErrorNombre::Reservado));
        assert_eq!(nombre_de_equipo("LocalHost"), Err(ErrorNombre::Reservado));
    }

    #[test]
    fn el_guion_al_borde_del_nombre_de_equipo_se_rechaza() {
        assert_eq!(nombre_de_equipo("-equipo"), Err(ErrorNombre::EmpiezaMal));
        assert_eq!(nombre_de_equipo("equipo-"), Err(ErrorNombre::TerminaMal));
        // Y dentro de una etiqueta del medio, que es donde es más fácil que se
        // cuele sin que nadie lo vea.
        assert_eq!(nombre_de_equipo("casa.-red.lan"), Err(ErrorNombre::EmpiezaMal));
    }

    #[test]
    fn los_puntos_sueltos_se_rechazan() {
        for malo in [".equipo", "equipo.", "casa..lan"] {
            assert_eq!(
                nombre_de_equipo(malo),
                Err(ErrorNombre::Caracter { cual: ".".into() }),
                "dejó pasar «{malo}»"
            );
        }
    }

    #[test]
    fn el_espacio_y_el_subrayado_no_van_en_el_nombre_de_equipo() {
        // El subrayado es válido en DNS moderno pero no en un nombre de host, y
        // `hostnamectl` lo rechaza.
        assert_eq!(
            nombre_de_equipo("mi equipo"),
            Err(ErrorNombre::Caracter { cual: " ".into() })
        );
        assert_eq!(
            nombre_de_equipo("mi_equipo"),
            Err(ErrorNombre::Caracter { cual: "_".into() })
        );
    }

    #[test]
    fn la_sugerencia_de_usuario_sale_usable() {
        assert_eq!(sugerir_usuario("Joaquín Decima"), "joaquin");
        assert_eq!(sugerir_usuario("Ana María"), "ana");
        assert_eq!(sugerir_usuario("João"), "joao");
        assert_eq!(sugerir_usuario("Ñandú"), "nandu");
        // Y lo que sugiere tiene que pasar la validación: si no, el campo se
        // autocompleta con algo que después se rechaza.
        for entrada in ["Joaquín Decima", "Ana María", "João", "Ñandú", "Pato"] {
            let sugerido = sugerir_usuario(entrada);
            assert_eq!(
                nombre_de_usuario(&sugerido),
                Ok(()),
                "«{entrada}» sugirió «{sugerido}», que no valida"
            );
        }
    }

    #[test]
    fn la_sugerencia_no_devuelve_algo_invalido_ante_entradas_raras() {
        // Con puros dígitos la primera letra sería un número, que no vale como
        // primer carácter. Antes se devolvía tal cual y el campo quedaba
        // autocompletado con algo inválido.
        for entrada in ["", "   ", "123", "!!!", "1998"] {
            let sugerido = sugerir_usuario(entrada);
            assert!(
                sugerido.is_empty() || nombre_de_usuario(&sugerido).is_ok(),
                "«{entrada}» sugirió «{sugerido}»"
            );
        }
    }

    #[test]
    fn la_zona_horaria_se_comprueba_contra_la_base_del_sistema() {
        assert_eq!(zona_horaria("America/Argentina/Buenos_Aires"), Ok(()));
        assert_eq!(zona_horaria("Europe/Madrid"), Ok(()));
        assert!(zona_horaria("America/No_Existe").is_err());
        assert!(zona_horaria("").is_err());
    }

    /// Una zona con `..` no puede resolver fuera de la base de datos.
    ///
    /// El nombre viene de un campo de texto libre —el que aparece cuando el
    /// sistema no pudo dar la lista— y termina en un enlace a `/etc/localtime`.
    #[test]
    fn una_zona_con_rutas_relativas_se_rechaza() {
        for malo in [
            "../../etc/shadow",
            "/etc/shadow",
            "America/../../../etc/passwd",
            "./America/Bogota",
        ] {
            assert!(zona_horaria(malo).is_err(), "dejó pasar «{malo}»");
        }
    }

    #[test]
    fn el_idioma_se_comprueba_contra_los_del_sistema() {
        // Si glibc no está, la comprobación se saltea y todo pasa; el test se
        // adapta en vez de fallar en una máquina sin catálogo.
        if std::fs::read_to_string("/usr/share/i18n/SUPPORTED").is_err() {
            return;
        }
        assert_eq!(idioma("es_AR"), Ok(()));
        assert_eq!(idioma("en_US"), Ok(()));
        assert!(idioma("xx_YY").is_err());
        assert!(idioma("").is_err());
        // Con la codificación pegada no valida: el resto del instalador maneja
        // el local sin ella, y `es_AR.UTF-8` en `sys_lang` duplica la línea en
        // `locale.gen`.
        assert!(idioma("es_AR.UTF-8").is_err());
    }

    #[test]
    fn un_idioma_con_espacios_o_barras_se_rechaza() {
        assert!(idioma("es AR").is_err());
        assert!(idioma("../es_AR").is_err());
    }

    #[test]
    fn el_teclado_se_comprueba_contra_los_del_sistema() {
        if crate::probe::teclados().is_empty() {
            return; // sin `kbd` instalado
        }
        assert_eq!(teclado("us"), Ok(()));
        assert!(teclado("no-existe-este-mapa").is_err());
        assert!(teclado("").is_err());
        assert!(teclado("../../etc/shadow").is_err());
    }

    #[test]
    fn la_fuerza_premia_el_largo_por_encima_de_las_clases() {
        assert_eq!(fuerza(""), Fuerza::Vacia);
        assert_eq!(fuerza("1234"), Fuerza::Debil);
        assert_eq!(fuerza("Pass1!"), Fuerza::Debil);
        // Cuatro clases en ocho caracteres es lo que piden las reglas de
        // composición, y produce exactamente esto.
        assert_eq!(fuerza("Passw0r!"), Fuerza::Aceptable);
        // Una frase larga de una sola clase es mejor, y el resultado lo refleja.
        assert_eq!(fuerza("correcto caballo bateria"), Fuerza::Buena);
        assert_eq!(fuerza("caballo bater1"), Fuerza::Buena);
    }
}
