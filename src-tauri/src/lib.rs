//! Punto de entrada del instalador de VasakOS.
//!
//! La aplicación son dos procesos. Éste es el de la ventana, que corre como el
//! usuario de la sesión live y no tiene privilegios; el otro es
//! `vasak-installer-helper`, que corre como root y hace todo lo que toca el
//! disco. La frontera entre los dos está en `sidecar.rs` (este lado) y
//! `helper.rs` (el otro), y hablan NDJSON.
//!
//! Módulos, de adentro hacia afuera:
//!
//! - `protocol` — el idioma entre los dos procesos.
//! - `layout` — qué particiones crear. Función pura, es el código cuyo error
//!   borra datos.
//! - `archconfig` — la única frontera con el esquema de archinstall.
//! - `probe`, `validar`, `teclado` — lo que se puede saber y comprobar antes.
//! - `sidecar`, `helper` — los dos lados del canal privilegiado.
//! - `commands` — lo que llama la interfaz.

pub mod archconfig;
pub mod commands;
pub mod helper;
pub mod layout;
mod locales;
pub mod probe;
pub mod protocol;
pub mod sidecar;
pub mod teclado;
pub mod validar;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // El diario del sistema, con el nombre de esta aplicación. Va **primero**
        // de todos los plugins: instala el gancho de pánico, y un pánico mientras
        // arranca otro plugin es de los más probables y de los que menos rastro
        // dejan. Y acá pesa el doble que en cualquier otra aplicación: cuando el
        // instalador falla, el equipo se reinicia y la única constancia que queda
        // de por qué es lo que se escribió en el diario.
        .plugin(tauri_plugin_vsk_journal::init())
        // El idioma de la sesión. **Con la ruta explícita de los catálogos**: el
        // plugin sólo prueba rutas relativas al ejecutable y al directorio de
        // trabajo, y ninguna existe cuando el binario está en /usr/bin.
        .plugin(tauri_plugin_i18n_vsk::init_with_path(
            Some(locales::idioma_del_sistema()),
            locales::directorio(),
        ))
        .plugin(tauri_plugin_vsk_contextual_menu::init())
        .plugin(tauri_plugin_config_manager::init())
        .plugin(tauri_plugin_vicons::init())
        .plugin(tauri_plugin_shell::init())
        .manage(commands::EstadoAyudante::default())
        .invoke_handler(tauri::generate_handler![
            commands::sondear_sistema,
            commands::sondear_discos,
            commands::catalogos,
            commands::pasos_de_instalacion,
            commands::validar_usuario,
            commands::validar_equipo,
            commands::sugerir_usuario,
            commands::fuerza_contrasena,
            commands::vista_previa_particionado,
            commands::preparar_ayudante,
            commands::ayudante_listo,
            commands::sondear_discos_con_sistemas,
            commands::instalar,
            commands::cancelar_instalacion,
            commands::reiniciar,
            commands::apagar,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación");
}
