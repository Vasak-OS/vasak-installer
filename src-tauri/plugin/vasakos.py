"""Plugin de archinstall para VasakOS.

Hace dos cosas, y la elección de hacerlas acá es deliberada:

1. **Emite progreso legible por máquina.** archinstall sólo escribe texto para
   humanos, así que sin esto la barra de la interfaz tendría que salir de
   adivinar la redacción de sus mensajes — y cualquier cambio de una palabra en
   archinstall dejaría la barra quieta sin ningún error visible. Los ganchos
   `on_*` que archinstall define son los mismos puntos donde arranca cada etapa
   real, así que el progreso sale del propio flujo del instalador y no de una
   heurística.

2. **Aplica los ajustes propios de VasakOS** sobre el sistema recién instalado.
   La alternativa era una lista de comandos de shell en `custom_commands` del
   JSON: quince cadenas sin tests, sin manejo de errores y sin forma de saber
   cuál de las quince falló. Acá cada ajuste es una función con su nombre, su
   error y su línea en el registro.

## El canal de eventos

Se escribe NDJSON —una línea de JSON por evento— en el archivo que indica
`VASAK_INSTALLER_EVENTOS`, y el ayudante lo sigue como un `tail -f`. No se usa la
salida estándar porque ahí escriben también pacman, `mkinitcpio`, `grub-install`
y todo lo que archinstall invoca; una línea de JSON partida por un `print` ajeno
es un evento perdido.

Cada escritura abre, escribe y cierra (`with open(..., 'a')`). Es más lento que
mantener el descriptor abierto y es a propósito: el proceso puede morir en
cualquier momento, y un búfer sin volcar se lleva el último evento — que es
precisamente el que dice dónde falló.

## Contrato con el lado Rust

Los nombres de los pasos son los de `Paso::clave()` en `protocol.rs` y los
nombres de los campos son los de `Progreso`. Agregar un paso es tocar los dos
lados; hay un test en Rust que verifica que los pasos y los catálogos de idioma
no se desincronicen.
"""

import json
import os
import re
import shutil
from pathlib import Path

# ── Canal de eventos ────────────────────────────────────────────────────────

RUTA_EVENTOS = os.environ.get("VASAK_INSTALLER_EVENTOS")

# Los pasos, en el orden en que ocurren. Tienen que coincidir con
# `Paso::clave()` del lado Rust.
PARTICIONAR = "particionar"
MONTAR = "montar"
ESPEJOS = "espejos"
SISTEMA_BASE = "sistemaBase"
ESCRITORIO = "escritorio"
ARRANQUE = "arranque"
USUARIOS = "usuarios"
CONFIGURACION = "configuracion"
VASAKOS = "vasakos"
CIERRE = "cierre"


def _escribir(objeto):
    """Agrega una línea al archivo de eventos.

    Se traga cualquier error a propósito: **el canal de progreso no puede
    matar la instalación**. Si el archivo no se puede escribir, la interfaz se
    queda sin barra pero el sistema se instala igual, y eso es mucho mejor que
    lo contrario. Es la única excepción a la regla de no tragarse errores en
    este archivo.
    """
    if not RUTA_EVENTOS:
        return
    try:
        with open(RUTA_EVENTOS, "a", encoding="utf-8") as archivo:
            archivo.write(json.dumps(objeto, ensure_ascii=False) + "\n")
    except Exception:  # noqa: BLE001
        pass


def progreso(paso, estado, fraccion=None, detalle=None):
    _escribir(
        {
            "type": "progress",
            "paso": paso,
            "estado": estado,
            "fraccion": fraccion,
            "detalle": detalle,
        }
    )


def registrar(linea, nivel="info"):
    _escribir({"type": "log", "nivel": nivel, "linea": linea})


def empezar(paso, detalle=None):
    progreso(paso, "en_curso", None, detalle)


def terminar(paso):
    progreso(paso, "hecho", 1.0, None)


def fallar(paso, error):
    progreso(paso, "fallado", None, str(error))
    registrar(f"falló {paso}: {error}", "error")


# ── Ganchos de archinstall ──────────────────────────────────────────────────
#
# archinstall los busca por nombre con `hasattr`, así que cada uno tiene que
# llamarse exactamente así. Todos devuelven `None` (o `False` donde corresponda)
# para no alterar lo que archinstall hace: son observadores, no reemplazos.
#
# Cuidado: un gancho que devuelve un valor "verdadero" le dice a archinstall que
# el plugin **se encargó** del paso y que no lo haga él. Nunca devolver algo
# distinto de `None` desde acá.


def on_mirrors(mirrors=None):
    """Los espejos ya están elegidos; el pacstrap está por empezar."""
    # El paso de particionado y montaje ya ocurrió cuando archinstall llega acá:
    # `mount_ordered_layout()` corre antes que `set_mirrors()`. No hay gancho
    # para ninguno de los dos, así que se cierran acá — es lo más temprano que
    # se puede afirmar con certeza que salieron bien.
    terminar(PARTICIONAR)
    terminar(MONTAR)
    empezar(ESPEJOS)
    terminar(ESPEJOS)
    empezar(SISTEMA_BASE)


def on_genfstab(installation=None):
    """El fstab está escrito: el sistema base terminó de instalarse."""
    terminar(SISTEMA_BASE)
    empezar(ESCRITORIO)


def on_mkinitcpio(installation=None):
    """Se está armando el initramfs."""
    # Los paquetes ya están: `add_additional_packages()` corre antes de que
    # `mkinitcpio` se regenere por última vez.
    terminar(ESCRITORIO)
    empezar(ARRANQUE, "initramfs")
    # `False` explícito, no `None`: éste es uno de los ganchos que archinstall
    # interpreta como «el plugin se encargó», y con un valor verdadero saltearía
    # la generación del initramfs y el sistema no arrancaría.
    return False


def on_add_bootloader(installation=None):
    empezar(ARRANQUE, "gestor de arranque")
    return False


def on_user_create(user=None):
    empezar(USUARIOS, getattr(user, "username", None))


def on_user_created(installation=None, user=None):
    """Un usuario quedó creado. Es el momento de lo que va en su `$HOME`."""
    terminar(ARRANQUE)
    terminar(USUARIOS)
    empezar(CONFIGURACION)
    try:
        _ajustar_usuario(installation, user)
    except Exception as error:  # noqa: BLE001
        # No es fatal: el sistema arranca igual, sólo que el nombre completo no
        # aparece en la pantalla de inicio de sesión. Se registra y se sigue.
        registrar(f"no se pudo ajustar la cuenta: {error}", "warn")


def on_service(service=None):
    registrar(f"habilitando {service}")


def on_timezone(timezone=None):
    registrar(f"zona horaria {timezone}")


def on_install(installation=None, *_args, **_kwargs):
    """Último gancho: el sistema está instalado y todavía está montado.

    Acá va todo lo propio de VasakOS. Es el único momento en que el destino
    existe completo y sigue montado.
    """
    terminar(CONFIGURACION)
    empezar(VASAKOS)

    destino = _destino(installation)
    if destino is None:
        fallar(VASAKOS, "no se pudo determinar el destino de la instalación")
        return

    ajustes = (
        ("teclado del escritorio", _configurar_teclado_xkb),
        ("limpieza del medio live", _limpiar_rastros_del_live),
        ("espejos de VasakOS", _asegurar_mirrorlist),
    )
    for indice, (nombre, funcion) in enumerate(ajustes):
        progreso(VASAKOS, "en_curso", indice / len(ajustes), nombre)
        try:
            funcion(destino)
            registrar(f"listo: {nombre}")
        except Exception as error:  # noqa: BLE001
            # Ninguno de estos ajustes es imprescindible para que el sistema
            # arranque, así que un fallo se anota y se sigue con el siguiente.
            # Abortar acá dejaría un sistema instalado y funcional marcado como
            # fallido, que es peor información que un aviso.
            registrar(f"no se pudo aplicar «{nombre}»: {error}", "warn")

    terminar(VASAKOS)
    empezar(CIERRE)
    terminar(CIERRE)


# ── Los ajustes ─────────────────────────────────────────────────────────────


def _destino(installation):
    """La raíz del sistema instalado, como `Path`.

    archinstall la expone como `.target`, pero el nombre del atributo cambió
    entre versiones mayores y el gancho a veces recibe otra cosa. Se prueban las
    variantes conocidas y, si ninguna sirve, se devuelve `None` en vez de
    adivinar `/mnt`: escribir en `/mnt` sin saber si es el destino es escribir
    en cualquier parte.
    """
    for atributo in ("target", "_target", "mountpoint", "_mountpoint"):
        valor = getattr(installation, atributo, None)
        if valor:
            ruta = Path(valor)
            # Que exista y tenga un `/etc` adentro es lo que confirma que es un
            # sistema instalado y no un punto de montaje vacío.
            if ruta.is_dir() and (ruta / "etc").is_dir():
                return ruta
    return None


def _ajustar_usuario(installation, user):
    """Pone el nombre completo en el campo GECOS de la cuenta.

    Es lo que hace que la pantalla de inicio de sesión diga «Joaquín Decima» y
    no «jdecima». archinstall crea la cuenta sin nombre completo porque su menú
    no lo pide; nuestra interfaz sí, y sería raro pedirlo y perderlo.
    """
    nombre_completo = os.environ.get("VASAK_INSTALLER_NOMBRE_COMPLETO", "").strip()
    usuario = getattr(user, "username", None) or os.environ.get("VASAK_INSTALLER_USUARIO", "")
    if not nombre_completo or not usuario:
        return

    # Los `:` parten los campos de `/etc/passwd`. Uno en el nombre completo
    # correría todo el resto del registro una posición y dejaría la cuenta con
    # el intérprete de comandos equivocado.
    if ":" in nombre_completo or "\n" in nombre_completo:
        registrar("el nombre completo tiene caracteres que no van en /etc/passwd", "warn")
        return

    installation.arch_chroot(f"chfn -f {_entrecomillar(nombre_completo)} {usuario}")


def _configurar_teclado_xkb(destino):
    """Deja el teclado del escritorio igual al que se eligió en el instalador.

    Éste es el ajuste que más se nota si falta, y de la peor manera: archinstall
    configura `KEYMAP` en `/etc/vconsole.conf`, que es el teclado **de la
    consola**. El escritorio corre en Wayland y usa el diseño de XKB, que se
    llama distinto —el latinoamericano es `la-latin1` en consola y `latam` en
    XKB—. Sin esto, alguien elige su teclado, la instalación termina bien, y en
    el primer arranque no puede escribir su contraseña en el greeter.

    El diseño ya viene traducido desde Rust (`teclado.rs`), que es donde está la
    tabla y sus tests.
    """
    diseño = os.environ.get("VASAK_INSTALLER_XKB", "").strip()
    variante = os.environ.get("VASAK_INSTALLER_XKB_VARIANTE", "").strip()
    if not diseño:
        return

    # `/etc/vasak/` es la configuración del escritorio a nivel sistema, que
    # vasak-desktop-settings lee como predeterminado para toda cuenta nueva. Va
    # ahí y no en el `$HOME` del primer usuario para que un segundo usuario
    # creado después también arranque con el teclado correcto.
    directorio = destino / "etc" / "vasak"
    directorio.mkdir(parents=True, exist_ok=True)

    lineas = [
        "# Escrito por el instalador de VasakOS.",
        "# Diseño de teclado de la sesión gráfica (XKB), derivado del mapa de",
        "# consola que se eligió al instalar. Ver /etc/vconsole.conf para la consola.",
        f"XKB_LAYOUT={diseño}",
    ]
    if variante:
        lineas.append(f"XKB_VARIANT={variante}")
    (directorio / "teclado.conf").write_text("\n".join(lineas) + "\n", encoding="utf-8")

    # Y para el greeter, que corre antes de que exista una sesión y por lo tanto
    # antes de que nadie lea la configuración de un usuario.
    entorno = destino / "etc" / "environment.d"
    entorno.mkdir(parents=True, exist_ok=True)
    contenido = [f"XKB_DEFAULT_LAYOUT={diseño}"]
    if variante:
        contenido.append(f"XKB_DEFAULT_VARIANT={variante}")
    (entorno / "90-vasak-teclado.conf").write_text("\n".join(contenido) + "\n", encoding="utf-8")


# Lo que el medio live deja y el sistema instalado no tiene que heredar.
#
# Sale de la lista que aplicaba `shellprocess-final.conf` de calamares, revisada:
# las entradas de sddm y plymouth de ahí ya no aplican —VasakOS usa greetd con
# vasak-session-manager— y estaban borrando archivos que no existen.
RASTROS_DEL_LIVE = [
    # sudo sin contraseña para el grupo wheel. En el medio live es lo que permite
    # instalar sin pedir nada; en el sistema instalado es una puerta abierta.
    "etc/sudoers.d/g_wheel",
    # Autologin de la consola y la inicialización de las claves de pacman: los
    # dos son andamiaje del arranque de la ISO.
    "etc/systemd/system/getty@tty1.service.d",
    "etc/systemd/system/multi-user.target.wants/pacman-init.service",
    "etc/systemd/system/pacman-init.service",
    "etc/systemd/system/etc-pacman.d-gnupg.mount",
    # Polkit sin autenticación, del mismo modo que el sudoers.
    "etc/polkit-1/rules.d/49-nopasswd_global.rules",
    # La regla que deja al instalador tomar root sin preguntar. En el medio live
    # es lo que evita un diálogo de contraseña que nadie puede contestar —el
    # usuario live no tiene una—; heredarla en el sistema instalado dejaría a
    # cualquiera del grupo wheel abrir un proceso root sin autenticarse.
    "etc/polkit-1/rules.d/49-vasak-installer.rules",
    # Los scripts que arrancan la sesión live.
    "root/.automated_script.sh",
    "root/.zlogin",
    "root/.xinitrc",
    # La configuración de greetd de la ISO, que hace autologin del usuario
    # `vasak`. El paquete greetd trae la suya, y ésta la pisaba.
    "etc/greetd/config.toml",
]


def _limpiar_rastros_del_live(destino):
    """Saca del sistema instalado lo que sólo tenía sentido en la ISO.

    El más importante es `etc/sudoers.d/g_wheel`: en el medio live le da sudo sin
    contraseña al usuario live, y heredarlo significa que cualquiera con una
    sesión abierta es root sin escribir nada.
    """
    for relativo in RASTROS_DEL_LIVE:
        ruta = destino / relativo
        # `is_symlink` primero: un enlace roto no es `exists()` pero sí hay que
        # borrarlo, y sin esta comprobación los enlaces de
        # `multi-user.target.wants` quedaban.
        if not ruta.exists() and not ruta.is_symlink():
            continue
        try:
            if ruta.is_dir() and not ruta.is_symlink():
                shutil.rmtree(ruta)
            else:
                ruta.unlink()
            registrar(f"quitado del sistema instalado: /{relativo}")
        except OSError as error:
            # Que uno no se pueda borrar no invalida los demás, y el que importa
            # de verdad —el sudoers— se reporta con su nombre en el registro.
            nivel = "error" if "sudoers" in relativo else "warn"
            registrar(f"no se pudo quitar /{relativo}: {error}", nivel)


def _asegurar_mirrorlist(destino):
    """Comprueba que el repositorio de VasakOS quedó en el `pacman.conf`.

    archinstall escribe la sección desde `custom_repositories`, así que esto no
    la agrega: **verifica**. Si por un cambio de su esquema la sección no
    quedara, el sistema instalado no podría actualizar ningún paquete de
    VasakOS, y eso se descubriría semanas después con un `pacman -Syu` que no
    trae nada.
    """
    pacman_conf = destino / "etc" / "pacman.conf"
    if not pacman_conf.is_file():
        raise FileNotFoundError("no hay /etc/pacman.conf en el sistema instalado")

    contenido = pacman_conf.read_text(encoding="utf-8")
    if re.search(r"^\s*\[vasakos\]", contenido, re.MULTILINE):
        return

    registrar(
        "archinstall no escribió el repositorio [vasakos] en pacman.conf; "
        "el instalador lo agrega",
        "warn",
    )
    with open(pacman_conf, "a", encoding="utf-8") as archivo:
        archivo.write(
            "\n# Agregado por el instalador de VasakOS.\n"
            "[vasakos]\n"
            "SigLevel = Required DatabaseOptional\n"
            "Include = /etc/pacman.d/vasakos-mirrorlist\n"
        )


def _entrecomillar(valor):
    """Entrecomilla para pasar por `arch_chroot`, que va a un shell.

    Comillas simples y los `'` internos escapados como `'\\''`. Sin esto, un
    nombre completo con un apóstrofo —«O'Connor», que no es raro— cerraría la
    comilla y el resto del nombre se ejecutaría como comando.
    """
    return "'" + valor.replace("'", "'\\''") + "'"
