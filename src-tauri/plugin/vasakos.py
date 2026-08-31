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
import tomllib
from pathlib import Path

# La versión de archinstall contra la que está escrito este plugin.
#
# No es opcional, aunque su propio comentario diga que sí: `load_plugin` hace
#
#     if sys.modules[namespace].__archinstall__version__ < float(...)
#
# sin `hasattr` delante, así que un plugin que no la define no arranca — revienta
# con AttributeError antes de que se cargue nada, y la instalación termina en el
# primer segundo.
#
# Tiene que ser un número, no una cadena: se compara con `<` contra un `float`.
# Y el número con el que se compara es `get_version().rsplit(".", 1)[0]`, o sea
# «4» para archinstall 4.4. Si el nuestro es menor, archinstall anota un error y
# sigue; por eso acá va la versión real contra la que se probó.
__archinstall__version__ = 4.4

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

    # `critico` dice si un fallo de ese ajuste invalida la instalación entera.
    # Sólo la limpieza lo es: los otros dos dejan un sistema que arranca y
    # funciona, apenas con el teclado equivocado o sin poder actualizarse.
    ajustes = (
        ("teclado del escritorio", _configurar_teclado_xkb, False),
        ("limpieza del medio live", _limpiar_rastros_del_live, True),
        ("greeter del sistema instalado", _asegurar_greetd, True),
        ("espejos de VasakOS", _asegurar_mirrorlist, False),
    )
    for indice, (nombre, funcion, critico) in enumerate(ajustes):
        progreso(VASAKOS, "en_curso", indice / len(ajustes), nombre)
        try:
            funcion(destino)
            registrar(f"listo: {nombre}")
        except Exception as error:  # noqa: BLE001
            if critico:
                # Se propaga: archinstall aborta y el ayudante informa la
                # instalación como fallida. Es la única forma de que alguien se
                # entere — un aviso en un registro plegado no lo lee nadie, y lo
                # que quedó abierto es acceso de root sin contraseña.
                fallar(VASAKOS, error)
                raise
            # Los demás dejan un sistema que arranca y funciona: se anota y se
            # sigue. Abortar por ellos marcaría como fallida una instalación
            # perfectamente usable, que es peor información que un aviso.
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

    # Los dos entrecomillados, no sólo el nombre completo. Hoy `commands.rs`
    # revalida el usuario contra `[a-z0-9_-]` antes de llegar acá, así que no hay
    # forma de que un metacarácter pase — pero el valor también puede venir de
    # `os.environ` o de `user.username`, que este archivo no valida, y la
    # seguridad de esta línea no tiene por qué depender de una comprobación que
    # vive en otro proceso.
    installation.arch_chroot(
        f"chfn -f {_entrecomillar(nombre_completo)} {_entrecomillar(usuario)}"
    )


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
#
# Con calamares esta limpieza era el único control: instalaba **copiando el
# squashfs** de la ISO (`unpackfs`), así que el sistema instalado nacía con
# todo lo del medio live adentro. archinstall no copia nada —hace `pacstrap`
# desde los repositorios—, y hoy los cuatro archivos de acá existen sólo en
# `archiso/airootfs/`: ningún paquete los trae, así que en el destino no
# aparecen. Esto queda igual, como red: cuesta nada borrar lo que no está, y el
# día que un paquete empiece a enviar uno de estos archivos el control ya está
# puesto.
#
# Lo que **no** puede estar en esta lista es un archivo que el sistema instalado
# tenga por derecho propio. `etc/greetd/config.toml` estuvo, heredado de la
# época de calamares, y era el único de los cinco que de verdad existía en el
# destino: lo escribe el hook de `vasak-session-manager` durante el pacstrap.
# Borrarlo dejaba el equipo instalado sin greeter. Lo suyo se comprueba en
# `_asegurar_greetd`, que mira el contenido en vez de borrar el archivo.
# Los que **tienen que** desaparecer. Si alguno queda, el sistema instalado le da
# privilegios de root a cualquiera con una sesión abierta, sin pedir contraseña.
#
# Un fallo acá aborta la instalación en vez de anotarse como aviso: entregar un
# sistema diciendo «listo» cuando cualquiera del grupo `wheel` es root sin
# escribir nada es peor que entregar un fallo con su motivo. Un aviso en un
# registro que nadie lee no es una advertencia.
RASTROS_CRITICOS = [
    # sudo sin contraseña para el grupo wheel. En el medio live es lo que permite
    # instalar sin pedir nada; en el sistema instalado es una puerta abierta.
    "etc/sudoers.d/g_wheel",
    # Polkit sin autenticación, del mismo modo que el sudoers.
    "etc/polkit-1/rules.d/49-nopasswd_global.rules",
    # La regla que deja al instalador tomar root sin preguntar. En el medio live
    # es lo que evita un diálogo de contraseña que nadie puede contestar —el
    # usuario live no tiene una—; heredarla en el sistema instalado dejaría a
    # cualquiera del grupo wheel abrir un proceso root sin autenticarse.
    "etc/polkit-1/rules.d/49-vasak-installer.rules",
]

# Andamiaje del arranque de la ISO. Que quede alguno ensucia, no abre nada, así
# que un fallo se anota y la instalación sigue.
RASTROS_COSMETICOS = [
    # Autologin de la consola y la inicialización de las claves de pacman.
    "etc/systemd/system/getty@tty1.service.d",
    "etc/systemd/system/multi-user.target.wants/pacman-init.service",
    "etc/systemd/system/pacman-init.service",
    "etc/systemd/system/etc-pacman.d-gnupg.mount",
    # Los scripts que arrancan la sesión live.
    "root/.automated_script.sh",
    "root/.zlogin",
    "root/.xinitrc",
]


def _quitar(destino, relativo):
    """Borra una ruta del sistema instalado. Devuelve si quedó efectivamente sin ella."""
    ruta = destino / relativo
    # `is_symlink` primero: un enlace roto no es `exists()` pero sí hay que
    # borrarlo, y sin esta comprobación los enlaces de
    # `multi-user.target.wants` quedaban.
    if not ruta.exists() and not ruta.is_symlink():
        return True
    try:
        if ruta.is_dir() and not ruta.is_symlink():
            shutil.rmtree(ruta)
        else:
            ruta.unlink()
    except OSError as error:
        registrar(f"no se pudo quitar /{relativo}: {error}", "error")
        return False

    registrar(f"quitado del sistema instalado: /{relativo}")
    # Se comprueba que de verdad no esté. `rmtree` puede vaciar un directorio y
    # dejarlo, y un `unlink` sobre un montaje puede no fallar y no borrar nada:
    # afirmar que se quitó sin mirar es exactamente lo que no se puede hacer con
    # un archivo que da root.
    return not ruta.exists() and not ruta.is_symlink()


class RastroCriticoPersistente(Exception):
    """Quedó en el sistema instalado un archivo que da privilegios sin autenticar."""


def _limpiar_rastros_del_live(destino):
    """Saca del sistema instalado lo que sólo tenía sentido en la ISO.

    Los de `RASTROS_CRITICOS` no son opcionales: `etc/sudoers.d/g_wheel` le da
    sudo sin contraseña a todo el grupo `wheel`, y las reglas de polkit hacen lo
    mismo por su lado. Si alguno sobrevive, **la instalación se marca como
    fallida**, porque un sistema que se entrega diciendo «listo» y en el que
    cualquiera es root sin escribir nada es peor que uno que falló y lo dice.
    """
    quedaron = [r for r in RASTROS_CRITICOS if not _quitar(destino, r)]

    # Los cosméticos se intentan igual aunque haya fallado un crítico: si al
    # final se aborta, cuanto menos quede del medio live, mejor.
    for relativo in RASTROS_COSMETICOS:
        _quitar(destino, relativo)

    if quedaron:
        raise RastroCriticoPersistente(
            "quedaron en el sistema instalado archivos que dan privilegios sin "
            "autenticación: " + ", ".join(f"/{r}" for r in quedaron)
        )


class AutologinHeredado(Exception):
    """El sistema instalado abre sesión sola con una cuenta que no está."""


def _usuarios_de(destino):
    """Las cuentas del sistema instalado, según su propio `/etc/passwd`.

    `None` si no se pudo leer, que no es lo mismo que «no hay ninguna» y por eso
    no se responde con un conjunto vacío: quien pregunta tiene que poder
    distinguir «esta cuenta no existe» de «no sé qué cuentas hay».
    """
    try:
        texto = (destino / "etc" / "passwd").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    return {linea.split(":", 1)[0] for linea in texto.splitlines() if ":" in linea}


def _leer_greetd(config):
    """Qué dice la configuración de greetd: si se entiende, y a quién le abre sola.

    Devuelve `(legible, autologin)`.

    Las dos cosas hacen falta por separado y por motivos distintos. `autologin`
    es la cuenta que entra sin escribir contraseña, y `None` ahí es lo que
    corresponde en un sistema instalado. `legible` es si greetd va a poder
    cargar el archivo: uno que no parsea deja el equipo sin pantalla de login
    igual que si faltara, y eso no se puede confundir con «está bien y no tiene
    autologin», que es lo mismo que devolvería mirando sólo la primera mitad.
    """
    try:
        texto = config.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False, None

    try:
        inicial = tomllib.loads(texto).get("initial_session")
    except tomllib.TOMLDecodeError:
        # Un archivo que no parsea puede tener el autologin adentro igual, y
        # darlo por «no hay» sería justo el error que este control existe para
        # evitar. Se busca la sección a mano, como hace con `sed` el hook de
        # vasak-session-manager.
        seccion = re.search(
            r"^\[initial_session\](.*?)(?=^\[|\Z)", texto, re.MULTILINE | re.DOTALL
        )
        if seccion is None:
            return False, None
        usuario = re.search(r'^\s*user\s*=\s*"([^"]*)"', seccion.group(1), re.MULTILINE)
        return False, (usuario.group(1) if usuario else None)

    if not isinstance(inicial, dict):
        return True, None
    usuario = inicial.get("user")
    return True, (usuario if isinstance(usuario, str) and usuario else None)


def _autologin_de(config):
    """La cuenta a la que greetd le abre sesión sin pedir nada, o `None`."""
    return _leer_greetd(config)[1]


def _asegurar_greetd(destino):
    """Que el equipo instalado pida quién sos, y que tenga con qué preguntarlo.

    Son dos cosas y las dos se rompen calladas.

    La primera es que no herede el **autologin** del medio live, que abre la
    sesión del usuario `vasak` sin contraseña. En el destino esa cuenta no
    existe, así que además de ser autologin no funciona: el arranque queda en un
    bucle de login que falla.

    La segunda es que quede una configuración. Antes esto se resolvía borrando
    `etc/greetd/config.toml`, y con calamares —que instalaba copiando el
    squashfs de la ISO— tenía sentido, porque el archivo que había ahí era el
    del live. Con archinstall el destino se arma con `pacstrap` y ese archivo lo
    escribe el hook de `vasak-session-manager`: borrarlo dejaba el equipo
    instalado sin greeter, es decir sin forma de entrar. Por eso ahora se mira
    el contenido, y el arreglo es reemplazar, no quitar.
    """
    config = destino / "etc" / "greetd" / "config.toml"
    referencia = destino / "usr" / "share" / "vasak-session-manager" / "greetd.toml"

    if not config.exists():
        # Sin configuración greetd arranca y no sabe qué levantar. Que falte es
        # raro —el hook la escribe—, pero si falta, ponerla es barato.
        if not referencia.exists():
            registrar("greetd quedó sin configuración y no hay referencia que copiar", "warn")
            return
        config.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(referencia, config)
        registrar("greetd no tenía configuración: se puso la de vasak-session-manager")
        return

    legible, usuario = _leer_greetd(config)

    if not legible:
        # Un archivo que greetd no puede cargar deja el equipo sin pantalla de
        # login, lo mismo que si faltara, así que se trata igual. Si hay
        # referencia se pone y no queda nada más que mirar; si no la hay, se
        # sigue: puede que la sección de autologin se haya encontrado a mano, y
        # sacarla importa más que dejar el archivo prolijo.
        if referencia.exists():
            shutil.copyfile(referencia, config)
            registrar(
                "la configuración de greetd no se entiende: se puso la de "
                "vasak-session-manager"
            )
            return
        registrar(
            "la configuración de greetd no se entiende y no hay referencia que "
            "poner en su lugar",
            "warn",
        )

    if usuario is None:
        return

    cuentas = _usuarios_de(destino)
    if cuentas is not None and usuario in cuentas:
        # La cuenta existe: no es el rastro del live, es una decisión. Cambiarla
        # sería pisar lo que alguien pidió.
        registrar(f"greetd abre sesión sola con «{usuario}», que existe: se deja como está")
        return

    if referencia.exists():
        shutil.copyfile(referencia, config)
        registrar(
            f"greetd abría sesión sola con «{usuario}», que no existe en el sistema "
            "instalado: se puso la configuración de vasak-session-manager"
        )
    else:
        # Sin referencia hay que elegir, y se elige que no abra sesión sola: un
        # equipo al que no se puede entrar se arregla; uno que entra solo, no se
        # nota.
        config.unlink()
        registrar(
            f"greetd abría sesión sola con «{usuario}», que no existe, y no hay "
            "referencia que poner en su lugar: se quitó la configuración",
            "warn",
        )

    quedo = _leer_greetd(config)[1] if config.exists() else None
    if quedo is not None:
        raise AutologinHeredado(
            f"el sistema instalado sigue abriendo sesión sola con «{quedo}», una "
            "cuenta que no existe"
        )


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

    # El `Include` sólo se escribe si el archivo al que apunta existe.
    #
    # `pacman` **aborta** cuando un `Include` apunta a un archivo que no está: no
    # es que ignore ese repositorio, es que no corre en absoluto. Y esta rama se
    # ejecuta justamente cuando algo salió distinto de lo previsto, que es la
    # misma situación en la que `vasakos-mirrorlist` puede no haberse instalado.
    #
    # Sin repositorio de VasakOS, el sistema no puede actualizar sus aplicaciones
    # —recuperable, se arregla agregando la sección a mano—. Con un `pacman.conf`
    # roto no se puede instalar nada, ni siquiera lo que haría falta para
    # arreglarlo.
    mirrorlist = destino / "etc" / "pacman.d" / "vasakos-mirrorlist"
    if not mirrorlist.is_file():
        registrar(
            "archinstall no escribió el repositorio [vasakos] y falta "
            "/etc/pacman.d/vasakos-mirrorlist: no se agrega la sección, porque un "
            "Include a un archivo inexistente hace abortar a pacman entero",
            "error",
        )
        return

    registrar(
        "archinstall no escribió el repositorio [vasakos] en pacman.conf; "
        "el instalador lo agrega",
        "warn",
    )
    with open(pacman_conf, "a", encoding="utf-8") as archivo:
        # `Required TrustAll` y no `Required DatabaseOptional`: tiene que coincidir
        # con lo que pone la configuración principal (`sign_check: "Required"` y
        # `sign_option: "TrustAll"` en `archconfig.rs`). Con `DatabaseOptional`,
        # este respaldo rechazaría la misma clave de `vasakos-keyring` que el
        # camino normal acepta, y los paquetes no se podrían verificar.
        archivo.write(
            "\n# Agregado por el instalador de VasakOS.\n"
            "[vasakos]\n"
            "SigLevel = Required TrustAll\n"
            "Include = /etc/pacman.d/vasakos-mirrorlist\n"
        )


def _entrecomillar(valor):
    """Entrecomilla para pasar por `arch_chroot`, que va a un shell.

    Comillas simples y los `'` internos escapados como `'\\''`. Sin esto, un
    nombre completo con un apóstrofo —«O'Connor», que no es raro— cerraría la
    comilla y el resto del nombre se ejecutaría como comando.
    """
    return "'" + valor.replace("'", "'\\''") + "'"


# ── El punto de entrada ─────────────────────────────────────────────────────
#
# archinstall no llama a las funciones del módulo. Importa el archivo, busca una
# clase llamada `Plugin`, la instancia, y guarda **el objeto**:
#
#     plugins[namespace] = sys.modules[namespace].Plugin()
#
# y después cada gancho lo invoca sobre ese objeto:
#
#     for plugin in plugins.values():
#         if hasattr(plugin, "on_install"):
#             plugin.on_install(self)
#
# Sin esta clase, `load_plugin` avisa «missing a valid entry-point», la
# instalación sigue como si no hubiera plugin, y **ningún gancho corre**: sin
# progreso en la interfaz, sin limpieza del medio live, sin verificar el
# repositorio. Nada de eso falla de forma visible, que es lo peor que podía
# pasar.
#
# Los métodos no hacen el trabajo, delegan. Las funciones de arriba son las que
# están probadas y las que se pueden llamar sin archinstall instalado.
#
# Las firmas son las de los sitios donde archinstall llama, no las que uno
# supondría. `on_user_create` es el caso que engaña: se invoca
# `plugin.on_user_create(self, user)` —dos argumentos, la instalación y el
# usuario—, y no sólo el usuario.
class Plugin:
    """Lo que archinstall instancia para hablar con nosotros."""

    def on_mirrors(self, mirrors=None):
        return on_mirrors(mirrors)

    def on_genfstab(self, installation=None):
        return on_genfstab(installation)

    def on_mkinitcpio(self, installation=None):
        return on_mkinitcpio(installation)

    def on_add_bootloader(self, installation=None):
        return on_add_bootloader(installation)

    # `_installation` con guion bajo: archinstall lo pasa —este gancho recibe dos
    # argumentos— y acá no se usa. El nombre lo deja dicho y calla a ARG002.
    def on_user_create(self, _installation=None, user=None):
        return on_user_create(user)

    def on_user_created(self, installation=None, user=None):
        return on_user_created(installation, user)

    def on_service(self, service=None):
        return on_service(service)

    def on_timezone(self, timezone=None):
        return on_timezone(timezone)

    def on_install(self, installation=None):
        return on_install(installation)
