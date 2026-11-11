//! Del mapa de teclado de consola al diseño de XKB.
//!
//! Hay **dos** teclados que configurar y no son el mismo nombre:
//!
//!  - El de la consola: `KEYMAP` en `/etc/vconsole.conf`, cargado por
//!    `loadkeys`. Es el que consume archinstall en su `kb_layout`. El
//!    latinoamericano acá se llama `la-latin1`.
//!  - El del escritorio: el diseño de XKB que usa Wayland, y por lo tanto
//!    Wayfire y todo lo que dibuja VasakOS. El latinoamericano acá se llama
//!    `latam`.
//!
//! Configurar sólo uno es el error que se nota tarde y mal: si sólo se configura
//! la consola, la persona elige su teclado en el instalador, la instalación
//! termina bien, y **el primer inicio de sesión no puede escribir su
//! contraseña**, porque el greeter corre en Wayland y quedó en `us`.
//!
//! La interfaz muestra un solo selector —el de consola, que es la lista que
//! archinstall entiende— y la traducción se hace acá.

use std::collections::BTreeSet;
use std::fs;

/// Excepciones: los casos donde el nombre de XKB no se puede derivar del de
/// consola sacándole sufijos.
///
/// Todo lo demás sale de la regla mecánica de `a_xkb`. Esta tabla existe porque
/// los dos proyectos nombraron distinto las mismas cosas, no porque haya lógica.
const EXCEPCIONES: &[(&str, (&str, &str))] = &[
    // Latinoamérica: `la` en consola, `latam` en XKB. Es el caso que más se
    // usa en VasakOS y el que motivó todo este módulo.
    ("la-latin1", ("latam", "")),
    ("la", ("latam", "")),
    // Reino Unido: `uk` en consola, `gb` en XKB (el código ISO del país).
    ("uk", ("gb", "")),
    // Canadá francés.
    ("cf", ("ca", "fr")),
    // Los diseños alternativos son variantes de `us`, no diseños propios.
    ("dvorak", ("us", "dvorak")),
    ("dvorak-l", ("us", "dvorak-l")),
    ("dvorak-r", ("us", "dvorak-r")),
    ("colemak", ("us", "colemak")),
    // Turco: `trq` es el Q turco.
    ("trq", ("tr", "")),
    ("trf", ("tr", "f")),
    // Japonés: el mapa de consola nombra el teclado de 106 teclas.
    ("jp106", ("jp", "")),
    // Griego, que en consola va con el código de idioma y en XKB con el del país.
    ("gr", ("gr", "")),
    // Brasil: ABNT2 es el diseño físico, no una variante de XKB.
    ("br-abnt2", ("br", "abnt2")),
    ("br-abnt", ("br", "abnt")),
    // Suiza: el mapa de consola distingue el idioma, XKB lo hace con variante.
    ("sg", ("ch", "de")),
    ("fr_CH", ("ch", "fr")),
    // Chino y coreano no tienen mapa de consola propio; si aparecen, van a `us`
    // porque el método de entrada hace el resto.
    ("us-acentos", ("us", "intl")),
];

/// Sufijos que en consola indican la codificación o una variante de teclas
/// muertas, y que en XKB no van en el nombre del diseño.
///
/// Se sacan en este orden y sólo uno: `de-latin1-nodeadkeys` tiene los dos, y
/// sacarlos de a uno desde el final da `de`, que es lo correcto.
const SUFIJOS: &[&str] = &[
    "-nodeadkeys",
    "-deadkeys",
    "-latin1",
    "-latin9",
    "-lat2",
    "-lat1",
    "-latin",
    "-abnt2",
    "-utf",
    "-ucw",
];

/// El diseño al que se cae cuando la traducción no da un diseño que XKB conozca.
///
/// `us` y no dejarlo vacío: un `xkb_layout` vacío hace que el compositor use su
/// propio predeterminado, que es `us` igual pero sin que quede registro de que
/// acá pasó algo. Con esto, `traducir` devuelve `Err` y el plugin lo anota en el
/// registro de la instalación.
pub const RESPALDO: &str = "us";

/// Traduce un mapa de consola a un diseño de XKB.
///
/// Devuelve `(diseño, variante)`; la variante es `""` cuando no hay.
///
/// `Err` con el diseño de respaldo cuando el resultado no figura entre los
/// diseños que conoce el sistema. No es fatal —se instala igual, con `us`— pero
/// tiene que quedar dicho.
pub fn traducir(
    keymap_consola: &str,
    diseños_conocidos: &BTreeSet<String>,
) -> Result<(String, String), (String, String)> {
    let (diseño, variante) = a_xkb(keymap_consola);

    // Con la lista vacía no se puede validar nada, y rechazar todo sería peor
    // que aceptar: es lo que pasa si `xkeyboard-config` no está instalado en el
    // medio live, y la traducción mecánica acierta en la enorme mayoría de los
    // casos.
    if diseños_conocidos.is_empty() || diseños_conocidos.contains(&diseño) {
        return Ok((diseño, variante));
    }
    Err((RESPALDO.to_string(), String::new()))
}

/// La parte mecánica: excepción si hay, y si no, sacar sufijos.
fn a_xkb(keymap_consola: &str) -> (String, String) {
    let limpio = keymap_consola.trim();

    if let Some((_, (diseño, variante))) = EXCEPCIONES.iter().find(|(k, _)| *k == limpio) {
        return (diseño.to_string(), variante.to_string());
    }

    let mut base = limpio.to_string();
    // En bucle y no una vez: `de-latin1-nodeadkeys` tiene dos sufijos.
    while let Some(sufijo) = SUFIJOS.iter().find(|s| base.ends_with(**s)) {
        base.truncate(base.len() - sufijo.len());
    }

    // Los mapas de consola de algunos países llevan el país en mayúscula
    // (`fr_CH`); XKB los quiere todos en minúscula.
    (base.to_lowercase(), String::new())
}

/// Los diseños de XKB que conoce el sistema.
///
/// Se lee `base.lst` de `xkeyboard-config`, que es un archivo de texto legible
/// por cualquiera, en vez de invocar `localectl list-x11-keymap-layouts`:
/// `localectl` necesita que `systemd-localed` esté corriendo y en el medio live
/// puede no estar, y ahí devuelve una lista vacía sin decir por qué.
///
/// El archivo tiene secciones (`! layout`, `! variant`, `! model`) y dentro de
/// cada una, líneas `nombre    descripción`. Interesa sólo la de `layout`.
pub fn diseños_de_xkb() -> BTreeSet<String> {
    const RUTA: &str = "/usr/share/X11/xkb/rules/base.lst";
    let Ok(contenido) = fs::read_to_string(RUTA) else {
        return BTreeSet::new();
    };

    let mut salida = BTreeSet::new();
    let mut dentro = false;
    for linea in contenido.lines() {
        let t = linea.trim();
        if t.starts_with('!') {
            // Otra sección: si ya estábamos en `layout`, terminamos.
            dentro = t == "! layout";
            continue;
        }
        if !dentro || t.is_empty() {
            continue;
        }
        if let Some(nombre) = t.split_whitespace().next() {
            salida.insert(nombre.to_string());
        }
    }
    salida
}

/// Fija `xkb_layout` y `xkb_variant` en la sección `[input]` de un wayfire.ini,
/// conservando comentarios, las demás claves (como `xkb_options`) y el resto de
/// las secciones. Si faltan las claves las agrega dentro de `[input]`; si falta
/// la sección entera, la agrega al final.
///
/// Es lo que hay que escribir para que un Wayfire ya corriendo tome el teclado:
/// recarga el archivo solo, y un valor acá gana sobre el `XKB_DEFAULT_LAYOUT`
/// que la sesión exporta al arrancar.
pub fn fijar_xkb_en_input(contenido: &str, diseño: &str, variante: &str) -> String {
    let linea_layout = format!("xkb_layout = {diseño}");
    let linea_variante = format!("xkb_variant = {variante}");

    // La clave de una línea `clave = valor`, salvo comentarios.
    let clave_de = |linea: &str| -> Option<String> {
        let t = linea.trim_start();
        if t.starts_with('#') || t.starts_with(';') {
            return None;
        }
        let (izq, _) = t.split_once('=')?;
        Some(izq.trim().to_string())
    };

    let mut salida: Vec<String> = Vec::new();
    let mut en_input = false;
    let mut puso_layout = false;
    let mut puso_variante = false;
    let mut vio_input = false;

    let completar = |salida: &mut Vec<String>, puso_l: &mut bool, puso_v: &mut bool| {
        if !*puso_l {
            salida.push(linea_layout.clone());
            *puso_l = true;
        }
        if !*puso_v {
            salida.push(linea_variante.clone());
            *puso_v = true;
        }
    };

    for linea in contenido.lines() {
        let t = linea.trim();
        if t.starts_with('[') && t.ends_with(']') {
            // Antes de cambiar de sección, completar lo que falte en `[input]`.
            if en_input {
                completar(&mut salida, &mut puso_layout, &mut puso_variante);
            }
            en_input = t == "[input]";
            if en_input {
                vio_input = true;
            }
            salida.push(linea.to_string());
            continue;
        }
        if en_input {
            match clave_de(linea).as_deref() {
                Some("xkb_layout") => {
                    salida.push(linea_layout.clone());
                    puso_layout = true;
                    continue;
                }
                Some("xkb_variant") => {
                    salida.push(linea_variante.clone());
                    puso_variante = true;
                    continue;
                }
                _ => {}
            }
        }
        salida.push(linea.to_string());
    }

    if en_input {
        completar(&mut salida, &mut puso_layout, &mut puso_variante);
    }
    if !vio_input {
        salida.push("[input]".to_string());
        salida.push(linea_layout.clone());
        salida.push(linea_variante.clone());
    }

    let mut texto = salida.join("\n");
    if contenido.ends_with('\n') {
        texto.push('\n');
    }
    texto
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conocidos() -> BTreeSet<String> {
        [
            "us", "es", "latam", "de", "fr", "gb", "ca", "br", "tr", "jp", "ch", "gr", "it", "pt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// El caso que motiva el módulo: sin esta traducción, alguien que elige el
    /// teclado latinoamericano termina con un greeter en `us` y no puede
    /// escribir su propia contraseña en el primer arranque.
    #[test]
    fn el_latinoamericano_se_traduce_a_latam() {
        assert_eq!(
            traducir("la-latin1", &conocidos()),
            Ok(("latam".to_string(), String::new()))
        );
    }

    #[test]
    fn los_sufijos_de_codificacion_se_van() {
        let c = conocidos();
        assert_eq!(traducir("de-latin1", &c), Ok(("de".into(), String::new())));
        // Dos sufijos en el mismo nombre: si sólo se sacara uno quedaría
        // `de-latin1`, que XKB no conoce, y caería al respaldo.
        assert_eq!(
            traducir("de-latin1-nodeadkeys", &c),
            Ok(("de".into(), String::new()))
        );
        assert_eq!(traducir("fr-latin1", &c), Ok(("fr".into(), String::new())));
    }

    #[test]
    fn las_excepciones_de_nombre_se_respetan() {
        let c = conocidos();
        // `uk` en consola es `gb` en XKB: es el código ISO del país, no el TLD.
        assert_eq!(traducir("uk", &c), Ok(("gb".into(), String::new())));
        assert_eq!(traducir("cf", &c), Ok(("ca".into(), "fr".into())));
        assert_eq!(traducir("trq", &c), Ok(("tr".into(), String::new())));
        assert_eq!(traducir("jp106", &c), Ok(("jp".into(), String::new())));
    }

    #[test]
    fn los_disenos_alternativos_son_variantes_de_us() {
        let c = conocidos();
        // Dvorak y Colemak no son diseños de XKB: son variantes de `us`.
        // Mandarlos como diseño hace que el compositor los ignore en silencio.
        assert_eq!(traducir("dvorak", &c), Ok(("us".into(), "dvorak".into())));
        assert_eq!(traducir("colemak", &c), Ok(("us".into(), "colemak".into())));
    }

    #[test]
    fn el_abnt2_brasileno_es_variante_y_no_diseno() {
        assert_eq!(
            traducir("br-abnt2", &conocidos()),
            Ok(("br".into(), "abnt2".into()))
        );
    }

    #[test]
    fn un_keymap_que_xkb_no_conoce_cae_al_respaldo_avisando() {
        // `Err` y no `Ok`: se instala igual con `us`, pero el plugin tiene que
        // poder anotarlo en el registro. Un respaldo silencioso deja a alguien
        // preguntándose por qué su teclado no es el que eligió.
        assert_eq!(
            traducir("no-existe-este", &conocidos()),
            Err(("us".to_string(), String::new()))
        );
    }

    #[test]
    fn sin_lista_de_disenos_se_acepta_la_traduccion_mecanica() {
        // Es lo que pasa si `xkeyboard-config` no está en el medio live.
        // Rechazar todo dejaría a todo el mundo en `us`; la traducción mecánica
        // acierta en la enorme mayoría.
        let vacia = BTreeSet::new();
        assert_eq!(traducir("es", &vacia), Ok(("es".into(), String::new())));
        assert_eq!(
            traducir("la-latin1", &vacia),
            Ok(("latam".into(), String::new()))
        );
    }

    #[test]
    fn el_mayusculas_del_pais_se_normaliza() {
        // `fr_CH` está en las excepciones, pero la regla general también tiene
        // que bajar a minúscula: XKB no conoce ningún diseño con mayúsculas.
        let c = conocidos();
        assert_eq!(traducir("fr_CH", &c), Ok(("ch".into(), "fr".into())));
        assert_eq!(traducir("IT", &c), Ok(("it".into(), String::new())));
    }

    /// Contra el `base.lst` real: las excepciones de la tabla tienen que existir
    /// de verdad como diseños de XKB. Un typo acá manda a alguien al respaldo.
    #[test]
    fn las_excepciones_apuntan_a_disenos_que_xkb_conoce() {
        let reales = diseños_de_xkb();
        if reales.is_empty() {
            return; // sin xkeyboard-config instalado
        }
        for (consola, (diseño, _)) in EXCEPCIONES {
            assert!(
                reales.contains(*diseño),
                "la excepción {consola} apunta a «{diseño}», que XKB no conoce"
            );
        }
    }

    /// Los mapas de consola más usados tienen que traducir a algo válido: si
    /// alguno cae al respaldo, alguien va a terminar con el teclado equivocado.
    #[test]
    fn los_teclados_mas_usados_no_caen_al_respaldo() {
        let reales = diseños_de_xkb();
        if reales.is_empty() {
            return;
        }
        for keymap in [
            "us",
            "la-latin1",
            "es",
            "de-latin1",
            "fr-latin1",
            "it",
            "pt-latin1",
            "uk",
            "br-abnt2",
            "ru",
            "jp106",
            "trq",
            "pl",
            "cz-lat2",
            "hu",
            "se-latin1",
            "no-latin1",
            "dk-latin1",
            "fi",
            "nl",
        ] {
            assert!(
                traducir(keymap, &reales).is_ok(),
                "«{keymap}» cae al respaldo `us`"
            );
        }
    }

    /// Lo que trae el skel real: claves vacías, `xkb_options`, comentarios y
    /// otras secciones que no se pueden tocar.
    const SKEL: &str = "\
[core]
plugins = foo bar

[input]
# Distribución del teclado
xkb_layout =
xkb_variant =
xkb_options = caps:escape
; otro comentario

[idle]
toggle = <super> KEY_L
";

    #[test]
    fn el_skel_real_queda_con_el_diseno_y_conserva_todo_lo_demas() {
        let esperado = "\
[core]
plugins = foo bar

[input]
# Distribución del teclado
xkb_layout = latam
xkb_variant = nodeadkeys
xkb_options = caps:escape
; otro comentario

[idle]
toggle = <super> KEY_L
";
        assert_eq!(fijar_xkb_en_input(SKEL, "latam", "nodeadkeys"), esperado);
    }

    #[test]
    fn reaplicar_otro_diseno_reemplaza_y_no_duplica() {
        let una_vez = fijar_xkb_en_input(SKEL, "es", "");
        let dos_veces = fijar_xkb_en_input(&una_vez, "latam", "");
        assert_eq!(dos_veces.matches("xkb_layout").count(), 1);
        assert_eq!(dos_veces.matches("xkb_variant").count(), 1);
        assert!(dos_veces.contains("xkb_layout = latam\n"));
        assert!(!dos_veces.contains("xkb_layout = es"));
        assert!(dos_veces.contains("xkb_options = caps:escape\n"));
    }

    #[test]
    fn la_variante_vacia_deja_la_clave_vacia() {
        let con_variante = "[input]\nxkb_layout = us\nxkb_variant = dvorak\n";
        let r = fijar_xkb_en_input(con_variante, "es", "");
        assert_eq!(r, "[input]\nxkb_layout = es\nxkb_variant = \n");
    }

    #[test]
    fn si_faltan_las_claves_se_agregan_dentro_de_input() {
        let r = fijar_xkb_en_input(
            "[input]\nxkb_options = caps:escape\n\n[idle]\nx = 1\n",
            "de",
            "",
        );
        let input = r.find("[input]").unwrap();
        let idle = r.find("[idle]").unwrap();
        let layout = r.find("xkb_layout = de").unwrap();
        let variante = r.find("xkb_variant = ").unwrap();
        assert!(input < layout && layout < idle);
        assert!(input < variante && variante < idle);
        assert!(r.contains("xkb_options = caps:escape\n"));
    }

    #[test]
    fn sin_seccion_input_se_agrega_al_final_sin_romper_lo_demas() {
        let original = "[core]\nplugins = foo\n\n[idle]\nx = 1\n";
        let r = fijar_xkb_en_input(original, "fr", "azerty");
        assert!(r.starts_with(original.trim_end_matches('\n')));
        assert_eq!(
            r,
            "[core]\nplugins = foo\n\n[idle]\nx = 1\n[input]\nxkb_layout = fr\nxkb_variant = azerty\n"
        );
    }

    #[test]
    fn un_archivo_vacio_queda_con_la_seccion_input() {
        assert_eq!(
            fijar_xkb_en_input("", "es", ""),
            "[input]\nxkb_layout = es\nxkb_variant = "
        );
    }

    #[test]
    fn el_salto_de_linea_final_se_conserva_o_no_se_agrega() {
        let con = fijar_xkb_en_input("[input]\nxkb_layout = us\n", "es", "");
        assert!(con.ends_with('\n'));
        let sin = fijar_xkb_en_input("[input]\nxkb_layout = us", "es", "");
        assert!(!sin.ends_with('\n'));
        assert!(sin.starts_with("[input]\nxkb_layout = es\n"));
    }

    #[test]
    fn un_comentario_con_la_clave_no_se_toma_por_la_clave() {
        let r = fijar_xkb_en_input("[input]\n# xkb_layout = us\n", "es", "");
        assert!(r.contains("# xkb_layout = us\n"));
        assert!(r.contains("\nxkb_layout = es\n"));
    }
}
