//! El binario del ayudante privilegiado.
//!
//! Es el que se lanza con pkexec y el único que corre como root. Todo lo que
//! hace está en `helper.rs`; acá sólo está el arranque, deliberadamente corto
//! para que se pueda leer entero de un vistazo qué corre con privilegios.

fn main() {
    // Se comprueba el uid antes que nada. Sin esto, alguien que ejecuta el
    // ayudante a mano obtiene un proceso que acepta peticiones de instalación,
    // falla en el primer `mkfs` por permisos, y deja un mensaje de error de
    // `parted` en lugar de decir lo que pasa.
    //
    // No es una medida de seguridad —un usuario sin privilegios no puede
    // formatear nada de todos modos— sino de diagnóstico.
    //
    // SEGURIDAD: `getuid` no toca memoria ni recibe punteros; es una llamada al
    // sistema sin argumentos.
    let uid = unsafe { libc::getuid() };
    if uid != 0 {
        eprintln!(
            "vasak-installer-helper tiene que correr como root.\n\
             No se ejecuta a mano: lo lanza el instalador con pkexec."
        );
        std::process::exit(1);
    }

    vasak_installer_lib::helper::run()
}
