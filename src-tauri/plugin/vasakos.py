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

# El metapaquete del escritorio. Es lo que distingue el `pacstrap` de los
# paquetes elegidos del que arma el sistema base.
METAPAQUETE = "vasakos-desktop"


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


def on_genfstab(installation=None, *_args, **_kwargs):
    """El último gancho de archinstall, y el único con el destino completo.

    Acá van todos los ajustes propios de VasakOS. **No en `on_install`**, y esto
    costó dos instalaciones que terminaban bien y no arrancaban: archinstall
    llama a `on_install` al final de `minimal_installation()`, y el orden de
    `guided.py` es

        línea 103:  minimal_installation()      ← ahí corre on_install
        línea 118:  add_bootloader()
        línea 136:  create_users()              ← ahí nace la cuenta y su $HOME
        línea 146:  add_additional_packages()   ← ahí se instala vasakos-desktop
        línea 184:  genfstab()                  ← ahí corre esto

    En `on_install` no existe todavía ni la cuenta ni casi ninguno de los
    archivos que estos ajustes tocan: `/etc/skel` lo trae
    `vasak-desktop-settings`, el `grub.d` propio también, y la configuración de
    greetd la escribe el hook de `vasak-session-manager` — los tres llegan en la
    línea 146. Sembrar el skel antes de que exista el home, desde un `/etc/skel`
    que todavía está vacío, no hace nada **y no falla**: el síntoma aparece
    recién en el primer inicio de sesión, con una pantalla negra.

    `genfstab()` es la última cosa que hace `guided.py` antes de ofrecer el menú
    de post-instalación, y se llama una sola vez. Llega después de las cuatro:
    cuenta creada, paquetes instalados, gestor de arranque puesto, destino
    todavía montado.

    **No devuelve nada, a propósito.** archinstall corta el bucle de plugins si
    uno devuelve exactamente `True` —`if plugin.on_genfstab(self) is True:
    break`— y aunque hoy seamos el único, devolver algo verdadero por accidente
    dejaría a otro sin correr. El fstab ya está escrito antes de este llamado,
    así que no hay riesgo de quedarse sin él.
    """
    # Los pasos que la interfaz venía mostrando se cierran acá: a esta altura el
    # sistema base y el escritorio están instalados de verdad.
    terminar(SISTEMA_BASE)
    terminar(ESCRITORIO)
    _aplicar_ajustes(installation)


def on_mkinitcpio(installation=None):
    """Se está armando el initramfs."""
    # Ojo con lo que se deduce de acá: este gancho corre **dentro** de
    # `minimal_installation()` (installer.py:978), o sea antes de que existan las
    # cuentas y antes de `add_additional_packages()`. Los otros dos sitios que
    # regeneran el initramfs son de plymouth y de UKI, que acá no se usan. Sirve
    # para mover la barra, no para afirmar que algo ya está instalado — creer eso
    # es lo que dejó los ajustes de VasakOS corriendo sobre un destino vacío.
    #
    # Por eso mismo acá **no** se cierra el escritorio: cerrarlo era decirle a
    # quien mira que ya está instalado cuando todavía faltan tres pasos. Lo abre
    # `on_pacstrap` cuando empieza de verdad y lo cierra `on_genfstab`.
    empezar(ARRANQUE, "initramfs")
    # `False` explícito, no `None`: éste es uno de los ganchos que archinstall
    # interpreta como «el plugin se encargó», y con un valor verdadero saltearía
    # la generación del initramfs y el sistema no arrancaría.
    return False


def on_pacstrap(paquetes=None):
    """Arranca un `pacstrap`. Es lo que dice cuándo empieza el escritorio.

    Se llama en cada `pacstrap`, y son dos: el del sistema base, desde
    `minimal_installation()`, y el de los paquetes elegidos, desde
    `add_additional_packages()`. Se distinguen por la lista: el metapaquete del
    escritorio sólo viene en el segundo.

    **No devuelve nada, y no es un detalle de estilo.** archinstall usa el
    retorno para *reemplazar* la lista de paquetes:

        if result := plugin.on_pacstrap(packages):
            packages = result

    Un valor verdadero devuelto por descuido —un `True`, la propia lista— cambia
    lo que se instala. Con `None` la lista queda intacta.
    """
    if paquetes and METAPAQUETE in paquetes:
        empezar(ESCRITORIO)


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


def _aplicar_ajustes(installation):
    """Los ajustes propios, sobre un destino ya completo.

    Separado de [`on_genfstab`] para que el gancho quede en una línea y esto se
    pueda leer —y probar— sin el ruido del progreso.
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
        ("configuración de las cuentas", _sembrar_skel, False),
        # Con `installation` porque necesita el chroot; las demás sólo miran
        # archivos del destino.
        ("nombre en el menú de arranque", lambda d: _rehacer_grub(d, installation), False),
        # Va después de regenerar grub.cfg, que es lo que esta comprobación lee.
        ("AppArmor en la línea de arranque", _verificar_apparmor_en_el_arranque, False),
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


# El archivo sin el que la sesión gráfica no arranca.
#
# `wayfire.ini` trae `[autostart] 0_env = uwsm finalize`, y de ese `finalize`
# depende todo: la unidad `wayland-wm@` de uwsm es `Type=notify` con
# `TimeoutStartSec=30`, así que sin el aviso de «listo» systemd la da por fallada
# a los treinta segundos y `OnFailure=wayland-session-shutdown.target` se lleva la
# sesión entera. Lo que se ve es una pantalla negra y la vuelta al inicio de
# sesión, medio minuto después, sin un solo mensaje de error.
CONFIG_DE_SESION = ".config/wayfire.ini"

# El drop-in que le pone el nombre a VasakOS en el menú de arranque.
DROPIN_GRUB = "etc/default/grub.d/10-vasakos.cfg"


# ── AppArmor ────────────────────────────────────────────────────────────────

# Dónde deja cada gestor de arranque la línea de comandos del kernel.
#
# Se miran todos y no sólo el de GRUB, a propósito. Hoy el instalador fija
# `"bootloader": "Grub"` —es el único de los que ofrece archinstall que arranca
# en BIOS—, y el parámetro que enciende AppArmor viaja en un drop-in de
# `/etc/default/grub.d/`, que sólo GRUB lee. Si mañana se ofreciera systemd-boot,
# rEFInd, Limine o una UKI, ese drop-in dejaría de leerse y AppArmor se apagaría
# **sin que nadie se entere**.
#
# Un mecanismo de seguridad que desaparece callado es peor que no tenerlo: nadie
# lo va a extrañar y el sistema se va a seguir describiendo como si lo tuviera.
# Por eso esto no comprueba «el drop-in está», que sería comprobar el mecanismo,
# sino «algo va a pedir AppArmor al arrancar», que es el resultado.
CONFIGS_DE_ARRANQUE = (
    "boot/grub/grub.cfg",
    # systemd-boot: un archivo por entrada.
    "boot/loader/entries",
    "boot/refind_linux.conf",
    "boot/EFI/refind/refind.conf",
    "boot/limine.conf",
    # De acá sale la línea de comandos cuando se generan UKIs.
    "etc/kernel/cmdline",
)

MODULO_LSM = "apparmor"

# `lsm=` lleva la lista entera separada por comas. Se mira ese valor y no un
# `"apparmor" in linea` suelto porque el nombre aparece también en rutas y
# comentarios —`/etc/apparmor.d`, sin ir más lejos—, y ahí la comprobación diría
# que sí sin que nadie hubiera encendido nada.
_LSM = re.compile(r"\blsm=([A-Za-z0-9_,-]+)")


class ApparmorSinActivar(Exception):
    pass


def _pide_apparmor(texto):
    """Si algo en ese texto enciende AppArmor al arrancar."""
    return any(MODULO_LSM in c.group(1).split(",") for c in _LSM.finditer(texto))


def _verificar_apparmor_en_el_arranque(destino):
    """Comprueba que el sistema instalado vaya a arrancar con AppArmor.

    No repara nada: avisa. Un sistema sin AppArmor arranca y funciona igual, así
    que abortar la instalación por esto sería peor información que un aviso —y
    el aviso sale en el registro de la interfaz, no en un archivo que no lee
    nadie—.
    """
    for relativo in CONFIGS_DE_ARRANQUE:
        ruta = destino / relativo
        if ruta.is_dir():
            archivos = sorted(h for h in ruta.iterdir() if h.is_file())
        elif ruta.is_file():
            archivos = [ruta]
        else:
            continue
        for archivo in archivos:
            try:
                contenido = archivo.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if _pide_apparmor(contenido):
                registrar(f"AppArmor va a quedar activo al arrancar (según /{relativo})")
                return

    raise ApparmorSinActivar(
        "ningún gestor de arranque pide lsm=...,apparmor: el sistema instalado "
        "va a arrancar sin AppArmor"
    )


def _rehacer_grub(destino, installation):
    """Vuelve a generar `grub.cfg` para que el menú diga VasakOS.

    Hace falta por el orden de archinstall. En `guided.py`:

        line 118:  installation.add_bootloader(...)
        line 146:  installation.add_additional_packages(...)

    `grub.cfg` se genera en la línea 118, cuando todavía no está instalado
    `vasak-desktop-settings` —que es quien trae el drop-in con
    `GRUB_DISTRIBUTOR="VasakOS"`—. Así que el primer menú se arma leyendo el
    `/etc/default/grub` de Arch, que dice `GRUB_DISTRIBUTOR="Arch"`, y el arranque
    queda con el nombre equivocado hasta la próxima actualización de kernel.

    Regenerarlo acá es la única forma de que el primer arranque ya esté bien: no
    hay gancho entre el `pacman.strap('grub')` que hace archinstall y su
    `grub-mkconfig`.

    Si falla no se aborta: el sistema arranca igual, sólo que el menú dice Arch.
    """
    if not (destino / DROPIN_GRUB).is_file():
        # Sin el drop-in no hay nada que ganar regenerando, y decirlo importa:
        # significa que `vasak-desktop-settings` no llegó a instalarse.
        registrar(f"no está /{DROPIN_GRUB}: el menú de arranque va a decir Arch", "warn")
        return

    if not (destino / "boot" / "grub").is_dir():
        # Instalación sin GRUB —otro gestor de arranque— o el directorio no quedó
        # donde se espera. No es un error nuestro.
        registrar("no hay /boot/grub: no se regenera el menú")
        return

    installation.arch_chroot("grub-mkconfig -o /boot/grub/grub.cfg")
    registrar("menú de arranque regenerado con el nombre de VasakOS")


def _sembrar_skel(destino):
    """Copia a cada cuenta lo que le falte de `/etc/skel`.

    # Por qué hace falta

    `/etc/skel` se copia cuando **se crea** la cuenta, y archinstall crea los
    usuarios *antes* de instalar los paquetes. En `guided.py`:

        line 136:  installation.create_users(...)
        line 146:  installation.add_additional_packages(...)

    O sea que cuando nace la cuenta, `/etc/skel` tiene nada más que lo de `base`:
    `vasak-desktop-settings` —que es quien trae `wayfire.ini`, la configuración de
    GTK y el resto— se instala diez líneas después. El home queda vacío y nadie lo
    nota hasta el primer inicio de sesión, que termina en una pantalla negra.

    `vasak-config-migrate` no lo tapa, y está bien que no: **no crea archivos que
    falten** —«puede que no use ese componente, y crearle configuración que no
    pidió es justo lo contrario de esto»—. Migrar y sembrar son dos cosas
    distintas; sembrar es de acá, que es el único momento en que el destino está
    completo y montado.

    # Qué hace y qué no

    Copia sólo **lo que no está**. Nada se sobrescribe: si alguien ya tiene un
    `wayfire.ini` propio —imposible en una instalación nueva, pero esto también
    corre si se reinstala sobre un `/home` existente— se deja intacto.

    Los dueños salen del propio directorio del home, no de `/etc/passwd`: es el
    mismo dato y no hay que parsear nada.
    """
    skel = destino / "etc" / "skel"
    if not skel.is_dir():
        registrar("no hay /etc/skel que copiar", "warn")
        return

    hogares = destino / "home"
    if not hogares.is_dir():
        registrar("el sistema instalado no tiene /home", "warn")
        return

    for hogar in sorted(hogares.iterdir()):
        if not hogar.is_dir() or hogar.is_symlink():
            continue
        try:
            duenio = hogar.stat()
        except OSError as error:
            registrar(f"no se pudo mirar {hogar.name}: {error}", "warn")
            continue

        copiados = _copiar_lo_que_falte(skel, hogar, duenio.st_uid, duenio.st_gid)
        registrar(f"{hogar.name}: {copiados} archivo(s) de /etc/skel")

        # Y se comprueba lo que de verdad importa. Un fallo acá no aborta la
        # instalación —el sistema arranca y se entra por consola— pero tiene que
        # quedar dicho, porque el síntoma no se parece en nada a la causa.
        if (skel / CONFIG_DE_SESION).is_file() and not (hogar / CONFIG_DE_SESION).is_file():
            registrar(
                f"{hogar.name} quedó sin {CONFIG_DE_SESION}: la sesión gráfica no va a arrancar",
                "error",
            )


def _copiar_lo_que_falte(origen, destino_dir, uid, gid):
    """Copia recursivamente lo que no exista en el destino. Devuelve cuántos."""
    copiados = 0
    for entrada in sorted(origen.iterdir()):
        objetivo = destino_dir / entrada.name

        if entrada.is_dir() and not entrada.is_symlink():
            if not objetivo.exists():
                objetivo.mkdir(parents=True)
                os.chown(objetivo, uid, gid)
                shutil.copymode(entrada, objetivo)
            copiados += _copiar_lo_que_falte(entrada, objetivo, uid, gid)
            continue

        # Un enlace se recrea como enlace: seguirlo copiaría el contenido y
        # dejaría dos archivos donde el paquete quiso uno.
        if entrada.is_symlink():
            if not objetivo.exists() and not objetivo.is_symlink():
                os.symlink(os.readlink(entrada), objetivo)
                copiados += 1
            continue

        if objetivo.exists():
            continue
        shutil.copy2(entrada, objetivo)
        os.chown(objetivo, uid, gid)
        copiados += 1

    return copiados


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
        # Sin `return`: ver el porqué en `on_genfstab`.
        on_genfstab(installation)

    def on_mkinitcpio(self, installation=None):
        return on_mkinitcpio(installation)

    def on_pacstrap(self, packages=None):
        # Sin `return`: el retorno reemplaza la lista de paquetes. Ver
        # `on_pacstrap`.
        on_pacstrap(packages)

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
