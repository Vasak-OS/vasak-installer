//! El afinado del WebKit tiene que quedar cableado de punta a punta.
//!
//! El glue de GTK/WebKit (`commands/webkit.rs`) no se puede ejercitar sin un
//! display real, así que esta prueba lee el fuente y comprueba que las piezas
//! sigan conectadas: las palancas que bajan la memoria, el comando registrado en
//! `lib.rs` y la llamada del frontend al montar. Si alguien borra un eslabón, el
//! afinado deja de aplicarse en silencio y esto lo caza.

use std::path::Path;

fn leer(ruta: &str) -> String {
    let completa = Path::new(env!("CARGO_MANIFEST_DIR")).join(ruta);
    std::fs::read_to_string(&completa)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", completa.display()))
}

#[test]
fn el_glue_declara_las_palancas_de_memoria() {
    let webkit = leer("src/commands/webkit.rs");
    assert!(
        webkit.contains("pub fn tune_webview"),
        "falta el comando `tune_webview` en commands/webkit.rs"
    );
    assert!(
        webkit.contains("set_enable_developer_extras(false)"),
        "el afinado dejó de apagar los extras de desarrollo"
    );
    assert!(
        webkit.contains("set_enable_back_forward_navigation_gestures(false)"),
        "el afinado dejó de apagar los gestos de navegación atrás/adelante"
    );
    assert!(
        webkit.contains("CacheModel::DocumentViewer"),
        "el afinado dejó de usar el caché de visor de documento"
    );
}

#[test]
fn el_comando_esta_registrado() {
    let lib = leer("src/lib.rs");
    assert!(
        lib.contains("commands::webkit::tune_webview"),
        "`tune_webview` no está en el generate_handler de lib.rs"
    );
}

#[test]
fn el_frontend_lo_invoca_al_montar() {
    let app = leer("../src/App.vue");
    assert!(
        app.contains("invoke('tune_webview')"),
        "App.vue no invoca `tune_webview` al montar"
    );
}
