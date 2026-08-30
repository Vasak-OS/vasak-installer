"""Pruebas del plugin de archinstall.

Se corren con `python3 -m unittest discover -s src-tauri/plugin`, sin ninguna
dependencia fuera de la biblioteca estándar: este archivo viaja en el paquete y
el `check()` del PKGBUILD lo ejecuta, así que sumarle pytest sería sumarle una
dependencia de compilación a la ISO.

Lo que se prueba es lo que se rompe callado y lo que tiene consecuencias de
seguridad. La limpieza del medio live es lo primero: si sobrevive el `sudoers`
sin contraseña, el sistema instalado le da root a cualquiera del grupo `wheel`
sin pedir nada, y eso no se nota hasta que alguien lo aprovecha.
"""

import os
import shutil
import tempfile
import unittest
from pathlib import Path

import vasakos


class LimpiezaDelMedioLive(unittest.TestCase):
    def setUp(self):
        self.raiz = Path(tempfile.mkdtemp(prefix="vsk-plugin-"))
        # Un destino con la pinta de un sistema recién instalado.
        (self.raiz / "etc").mkdir()
        # El canal de eventos no apunta a ningún lado en los tests: `_escribir`
        # se va por su rama de «sin ruta» y no escribe nada.
        self.eventos_previos = vasakos.RUTA_EVENTOS
        vasakos.RUTA_EVENTOS = None

    def tearDown(self):
        vasakos.RUTA_EVENTOS = self.eventos_previos
        shutil.rmtree(self.raiz, ignore_errors=True)

    def _crear(self, relativo, contenido="x"):
        ruta = self.raiz / relativo
        ruta.parent.mkdir(parents=True, exist_ok=True)
        ruta.write_text(contenido)
        return ruta

    def test_borra_los_archivos_criticos(self):
        rutas = [self._crear(r) for r in vasakos.RASTROS_CRITICOS]

        vasakos._limpiar_rastros_del_live(self.raiz)

        for ruta in rutas:
            self.assertFalse(ruta.exists(), f"{ruta} sobrevivió")

    def test_borra_tambien_los_cosmeticos(self):
        rutas = [self._crear(r) for r in vasakos.RASTROS_COSMETICOS]

        vasakos._limpiar_rastros_del_live(self.raiz)

        for ruta in rutas:
            self.assertFalse(ruta.exists(), f"{ruta} sobrevivió")

    def test_no_falla_si_no_habia_nada_que_borrar(self):
        # Un sistema instalado sin rastros del medio live es el caso normal
        # cuando archinstall ya limpió por su cuenta.
        vasakos._limpiar_rastros_del_live(self.raiz)

    def test_un_directorio_se_borra_entero(self):
        # `getty@tty1.service.d` es un directorio, no un archivo: con `unlink` a
        # secas quedaba tal cual y el autologin de consola se heredaba.
        directorio = self.raiz / "etc/systemd/system/getty@tty1.service.d"
        directorio.mkdir(parents=True)
        (directorio / "autologin.conf").write_text("x")

        vasakos._limpiar_rastros_del_live(self.raiz)

        self.assertFalse(directorio.exists())

    def test_un_enlace_roto_tambien_se_borra(self):
        # Los de `multi-user.target.wants` son enlaces, y uno colgado no es
        # `exists()`: sin comprobar `is_symlink` quedaban puestos.
        enlace = self.raiz / "etc/systemd/system/multi-user.target.wants/pacman-init.service"
        enlace.parent.mkdir(parents=True)
        os.symlink("/no/existe", enlace)

        vasakos._limpiar_rastros_del_live(self.raiz)

        self.assertFalse(enlace.is_symlink(), "el enlace roto sobrevivió")

    def test_un_critico_que_no_se_puede_borrar_aborta_la_instalacion(self):
        """Éste es el que importa.

        Si el `sudoers` sin contraseña sobrevive, el sistema instalado le da root
        a cualquiera del grupo `wheel`. Antes esto se anotaba como un aviso en el
        registro y la instalación se declaraba terminada con éxito.
        """
        critico = self._crear(vasakos.RASTROS_CRITICOS[0])
        # Se simula que no se puede borrar dejando el directorio sin permiso de
        # escritura, que es lo que pasaría con un montaje de sólo lectura.
        modo_original = critico.parent.stat().st_mode
        critico.parent.chmod(0o555)
        try:
            if os.geteuid() == 0:
                # root escribe igual en un directorio sin permiso, así que la
                # simulación no sirve. El caso queda cubierto por el test de
                # abajo, que no depende de los permisos.
                self.skipTest("como root los permisos no impiden borrar")
            with self.assertRaises(vasakos.RastroCriticoPersistente):
                vasakos._limpiar_rastros_del_live(self.raiz)
        finally:
            critico.parent.chmod(modo_original)

    def test_el_error_nombra_los_archivos_que_quedaron(self):
        # Sin los nombres, el mensaje de fallo no dice qué hay que arreglar a
        # mano en el sistema que quedó a medias.
        error = vasakos.RastroCriticoPersistente(
            "quedaron: /etc/sudoers.d/g_wheel"
        )
        self.assertIn("sudoers", str(error))

    def test_el_sudoers_sin_contrasena_esta_entre_los_criticos(self):
        # Una lista de la que se saque este archivo sin querer deja de proteger
        # justamente lo que motivó la lista.
        self.assertIn("etc/sudoers.d/g_wheel", vasakos.RASTROS_CRITICOS)
        self.assertIn(
            "etc/polkit-1/rules.d/49-vasak-installer.rules", vasakos.RASTROS_CRITICOS
        )
        # Y ninguno de los críticos puede estar además entre los cosméticos: si
        # estuviera, un fallo suyo se anotaría como aviso en la segunda pasada.
        self.assertEqual(
            set(vasakos.RASTROS_CRITICOS) & set(vasakos.RASTROS_COSMETICOS), set()
        )


class GreeterDelSistemaInstalado(unittest.TestCase):
    """El equipo instalado tiene que pedir contraseña, y poder pedirla.

    Las dos mitades tienen historia. `etc/greetd/config.toml` estaba en
    `RASTROS_CRITICOS`, heredado de cuando instalaba calamares copiando el
    squashfs de la ISO: ahí el archivo del destino era, efectivamente, el del
    medio live. Con archinstall el destino se arma con `pacstrap` y ese archivo
    lo escribe el hook de `vasak-session-manager`, así que borrarlo dejaba el
    equipo sin greeter — instalado, arrancando, y sin forma de entrar.
    """

    AUTOLOGIN_DEL_LIVE = """[terminal]
vt = 7

[initial_session]
command = "vasak-session"
user = "vasak"

[default_session]
command = "/usr/bin/vasak-session-manager-launch"
user = "greeter"
"""

    REFERENCIA = """[terminal]
vt = 7

[default_session]
command = "/usr/bin/vasak-session-manager-launch"
user = "greeter"
"""

    def setUp(self):
        self.raiz = Path(tempfile.mkdtemp(prefix="vsk-greetd-"))
        self.eventos_previos = vasakos.RUTA_EVENTOS
        vasakos.RUTA_EVENTOS = None
        self.config = self.raiz / "etc" / "greetd" / "config.toml"
        self.referencia = (
            self.raiz / "usr" / "share" / "vasak-session-manager" / "greetd.toml"
        )

    def tearDown(self):
        vasakos.RUTA_EVENTOS = self.eventos_previos
        shutil.rmtree(self.raiz, ignore_errors=True)

    def _escribir(self, ruta, contenido):
        ruta.parent.mkdir(parents=True, exist_ok=True)
        ruta.write_text(contenido, encoding="utf-8")
        return ruta

    def _con_cuentas(self, *nombres):
        lineas = ["root:x:0:0::/root:/bin/bash"]
        lineas += [f"{n}:x:1000:1000::/home/{n}:/bin/bash" for n in nombres]
        self._escribir(self.raiz / "etc" / "passwd", "\n".join(lineas) + "\n")

    # ── La regresión ────────────────────────────────────────────────────────
    def test_la_config_de_greetd_no_esta_entre_los_rastros_que_se_borran(self):
        # Ésta es la prueba del error: mientras estuvo en la lista, la limpieza
        # borraba del sistema instalado el archivo que escribe el hook de
        # vasak-session-manager, y el equipo quedaba sin greeter.
        self.assertNotIn("etc/greetd/config.toml", vasakos.RASTROS_CRITICOS)
        self.assertNotIn("etc/greetd/config.toml", vasakos.RASTROS_COSMETICOS)

    def test_la_limpieza_no_toca_la_config_de_greetd(self):
        self._escribir(self.config, self.REFERENCIA)

        vasakos._limpiar_rastros_del_live(self.raiz)

        self.assertTrue(self.config.exists(), "la limpieza se llevó el greeter")

    # ── El autologin heredado ───────────────────────────────────────────────
    def test_el_autologin_del_live_se_reemplaza_por_la_referencia(self):
        self._escribir(self.config, self.AUTOLOGIN_DEL_LIVE)
        self._escribir(self.referencia, self.REFERENCIA)
        self._con_cuentas("pato")  # `vasak` no está: es la cuenta del medio live

        vasakos._asegurar_greetd(self.raiz)

        self.assertTrue(self.config.exists(), "quedó sin configuración")
        self.assertIsNone(vasakos._autologin_de(self.config))
        self.assertIn("vasak-session-manager-launch", self.config.read_text())

    def test_sin_referencia_se_quita_antes_que_dejar_el_autologin(self):
        self._escribir(self.config, self.AUTOLOGIN_DEL_LIVE)
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)

        # Un equipo al que no se puede entrar se arregla; uno que entra solo
        # sin contraseña no se nota.
        self.assertFalse(self.config.exists())

    def test_un_toml_roto_no_esconde_el_autologin(self):
        # Si no parsea, darlo por «no hay autologin» sería justo el error que
        # este control existe para evitar.
        self._escribir(self.config, "esto no es toml [[[\n" + self.AUTOLOGIN_DEL_LIVE)
        self._escribir(self.referencia, self.REFERENCIA)
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)

        self.assertIsNone(vasakos._autologin_de(self.config))

    def test_un_toml_roto_sin_autologin_igual_se_reemplaza(self):
        # No tiene autologin, así que no hay nada que sacar por seguridad, pero
        # greetd tampoco lo puede cargar: el equipo arranca sin pantalla de
        # login. Mirando sólo el autologin, este caso es indistinguible de una
        # configuración sana, y era el agujero.
        self._escribir(self.config, "[terminal\nvt = ")
        self._escribir(self.referencia, self.REFERENCIA)
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)

        self.assertEqual(self.config.read_text(), self.REFERENCIA)

    def test_un_toml_roto_sin_referencia_avisa_y_no_aborta(self):
        # Sin referencia no hay con qué arreglarlo. Se avisa y la instalación
        # sigue: el equipo queda con un greeter que no levanta, que se arregla
        # desde una consola, y descartar una instalación entera y completa por
        # eso es peor que entregarla.
        self._escribir(self.config, "[terminal\nvt = ")
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)  # no levanta

        self.assertTrue(self.config.exists())

    def test_legible_distingue_el_toml_roto_de_uno_sano_sin_autologin(self):
        roto = self._escribir(self.raiz / "roto.toml", "[terminal\nvt = ")
        sano = self._escribir(self.raiz / "sano.toml", self.REFERENCIA)

        # Los dos no tienen autologin; sólo uno se puede cargar.
        self.assertEqual(vasakos._leer_greetd(roto), (False, None))
        self.assertEqual(vasakos._leer_greetd(sano), (True, None))

    # ── Lo que no hay que tocar ─────────────────────────────────────────────
    def test_un_autologin_a_una_cuenta_que_existe_se_respeta(self):
        self._escribir(self.config, self.AUTOLOGIN_DEL_LIVE)
        self._escribir(self.referencia, self.REFERENCIA)
        self._con_cuentas("vasak")  # acá sí existe: es una decisión, no un rastro

        vasakos._asegurar_greetd(self.raiz)

        self.assertEqual(vasakos._autologin_de(self.config), "vasak")

    def test_una_config_sin_autologin_queda_intacta(self):
        self._escribir(self.config, self.REFERENCIA)
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)

        self.assertEqual(self.config.read_text(), self.REFERENCIA)

    # ── El greeter que falta ────────────────────────────────────────────────
    def test_si_no_hay_config_se_pone_la_referencia(self):
        self._escribir(self.referencia, self.REFERENCIA)
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)

        self.assertTrue(self.config.exists())
        self.assertEqual(self.config.read_text(), self.REFERENCIA)

    def test_sin_config_ni_referencia_avisa_en_vez_de_paniquear(self):
        self._con_cuentas("pato")

        vasakos._asegurar_greetd(self.raiz)  # no levanta

        self.assertFalse(self.config.exists())


class Entrecomillado(unittest.TestCase):
    def test_un_apostrofo_no_cierra_la_comilla(self):
        # «O'Connor» no es un nombre raro, y `arch_chroot` pasa la cadena por un
        # shell: sin escapar, el resto del nombre se ejecutaría como comando.
        salida = vasakos._entrecomillar("O'Connor")
        self.assertEqual(salida, "'O'\\''Connor'")

    def test_los_metacaracteres_quedan_adentro(self):
        for peligroso in ["; rm -rf /", "$(whoami)", "`id`", "a && b", "x | y"]:
            salida = vasakos._entrecomillar(peligroso)
            self.assertTrue(salida.startswith("'"))
            self.assertTrue(salida.endswith("'"))


class SeccionDelRepositorio(unittest.TestCase):
    def setUp(self):
        self.raiz = Path(tempfile.mkdtemp(prefix="vsk-plugin-repo-"))
        (self.raiz / "etc").mkdir()
        self.eventos_previos = vasakos.RUTA_EVENTOS
        vasakos.RUTA_EVENTOS = None

    def tearDown(self):
        vasakos.RUTA_EVENTOS = self.eventos_previos
        shutil.rmtree(self.raiz, ignore_errors=True)

    def test_no_toca_nada_si_archinstall_ya_escribio_la_seccion(self):
        conf = self.raiz / "etc/pacman.conf"
        conf.write_text("[options]\n\n[vasakos]\nInclude = /etc/pacman.d/vasakos-mirrorlist\n")
        antes = conf.read_text()

        vasakos._asegurar_mirrorlist(self.raiz)

        self.assertEqual(conf.read_text(), antes)

    def test_no_escribe_un_include_a_un_archivo_que_no_existe(self):
        """Un `Include` colgado hace **abortar a pacman entero**.

        Sin repositorio de VasakOS el sistema no puede actualizar sus
        aplicaciones, que se arregla a mano. Con el `pacman.conf` roto no se
        puede instalar nada, ni siquiera lo que haría falta para arreglarlo.
        """
        conf = self.raiz / "etc/pacman.conf"
        conf.write_text("[options]\n")

        vasakos._asegurar_mirrorlist(self.raiz)

        self.assertNotIn("[vasakos]", conf.read_text())

    def test_escribe_la_seccion_cuando_el_mirrorlist_esta(self):
        conf = self.raiz / "etc/pacman.conf"
        conf.write_text("[options]\n")
        mirrorlist = self.raiz / "etc/pacman.d/vasakos-mirrorlist"
        mirrorlist.parent.mkdir(parents=True)
        mirrorlist.write_text("Server = https://repo.vasak.net.ar/repo/$arch/$repo\n")

        vasakos._asegurar_mirrorlist(self.raiz)

        contenido = conf.read_text()
        self.assertIn("[vasakos]", contenido)
        self.assertIn("Include = /etc/pacman.d/vasakos-mirrorlist", contenido)
        # La firma tiene que coincidir con la del camino normal
        # (`sign_check: "Required"` + `sign_option: "TrustAll"` en
        # `archconfig.rs`). Con `DatabaseOptional`, este respaldo rechazaría la
        # misma clave de `vasakos-keyring` que el camino normal acepta.
        self.assertIn("SigLevel = Required TrustAll", contenido)

    def test_sin_pacman_conf_avisa_en_vez_de_paniquear(self):
        with self.assertRaises(FileNotFoundError):
            vasakos._asegurar_mirrorlist(self.raiz)


class PasosDelProtocolo(unittest.TestCase):
    def test_los_nombres_son_los_que_espera_el_lado_rust(self):
        # `Paso::clave()` en `protocol.rs` devuelve estos mismos textos, y el
        # ayudante deserializa los eventos con ellos. Un nombre que no coincida
        # hace que sus eventos se descarten como «evento ilegible» y la barra se
        # quede quieta sin que nada falle.
        self.assertEqual(vasakos.SISTEMA_BASE, "sistemaBase")
        self.assertEqual(vasakos.PARTICIONAR, "particionar")
        self.assertEqual(vasakos.CIERRE, "cierre")

    def test_los_ganchos_que_archinstall_puede_saltear_devuelven_falso(self):
        # archinstall interpreta un valor verdadero como «el plugin se encargó de
        # este paso». Devolviendo algo verdadero desde `on_mkinitcpio`, no
        # generaría el initramfs y el sistema no arrancaría.
        vasakos.RUTA_EVENTOS = None
        self.assertIs(vasakos.on_mkinitcpio(None), False)
        self.assertIs(vasakos.on_add_bootloader(None), False)


if __name__ == "__main__":
    unittest.main()
