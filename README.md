# vasak-installer — el instalador de VasakOS

Interfaz Tauri sobre **archinstall**. Todos los pasos se responden en la ventana;
archinstall no muestra ninguno de sus menús.

Reemplaza a `vasakos-calamares`. El cambio no es sólo de interfaz: calamares
instalaba **copiando el squashfs de la ISO** (`unpackfs`), sin red y en pocos
minutos; esto hace **`pacstrap` desde los repositorios**, así que la instalación
necesita conexión de punta a punta, tarda entre quince minutos y una hora, y a
cambio el sistema instalado queda con los paquetes al día en vez de con los que
tenía la ISO el día que se armó.

---

## Cómo está partido

Son **dos procesos**, y ésa es la decisión de la que cuelga todo lo demás.

```text
┌──────────────────────────┐        NDJSON        ┌──────────────────────────┐
│ vasak-installer          │  ←───────────────→   │ vasak-installer-helper   │
│ (usuario de la sesión)   │   stdin / stdout     │ (root, vía pkexec)       │
│                          │                      │                          │
│ Vue · WebView            │                      │ lsblk · openssl · parted │
│ sondeo sin privilegios   │                      │ archinstall              │
└──────────────────────────┘                      └───────────┬──────────────┘
                                                              │ --plugin
                                                  ┌───────────▼──────────────┐
                                                  │ plugin/vasakos.py        │
                                                  │ progreso + post-config   │
                                                  └──────────────────────────┘
```

**La ventana no corre como root.** Es un motor de navegador completo, con su JIT
y su pila de red; darle privilegios para poder llamar a `parted` cambia una
superficie de ataque de dos comandos por una de un navegador. Hay además una
razón visible: como root, el plugin de configuración leería `/root` en vez del
perfil de la sesión live, y el instalador aparecería con otro tema y otros iconos
que el escritorio que lo rodea.

**El ayudante se lanza al llegar al paso del disco**, no al abrir la ventana. Una
aplicación que arranca pidiendo autorización le pide a alguien que apruebe algo
que todavía no sabe qué es. Casi todo el sondeo —`lsblk`, `/proc`, los catálogos
de `/usr/share`— no necesita privilegios y corre en el proceso de la ventana; root
hace falta para dos cosas: correr `os-prober` (monta particiones ajenas para
mirar qué sistema tienen) y la instalación.

---

## Por qué archinstall se maneja por archivo y no como librería

archinstall se puede usar de dos formas: importándolo desde Python y manejando su
clase `Installer` paso a paso, o pasándole un archivo de configuración y dejando
que corra solo. Acá se hace lo segundo.

El archivo de configuración es su **entrada versionada y documentada**: tiene un
campo `version`, su esquema está publicado, y el propio archinstall sabe
generarlo desde sus menús. La API de Python, en cambio, se reacomoda entre
versiones mayores —los módulos se movieron entre la 2.x, la 3.x y la 4.x—, y un
instalador que la usa se rompe con cada actualización del paquete.

Todo lo que sabe del esquema de archinstall vive en **`archconfig.rs` y en ningún
otro lado**: los nombres de las claves, que `Grub` va con mayúscula, que los
tamaños son objetos con unidad y sector. Cuando archinstall cambie de versión
mayor, ése es el único archivo que hay que revisar.

### El precio de esa decisión, y cómo se paga

archinstall **no emite nada legible por máquina**: todo su progreso son líneas de
texto para humanos. Sacar la barra de progreso de adivinar la redacción de sus
mensajes es frágil de la peor manera —un cambio de una palabra deja la barra
quieta sin ningún error visible.

Por eso el instalador envía **su propio plugin de archinstall**
(`src-tauri/plugin/vasakos.py`). archinstall define ganchos `on_mirrors`,
`on_genfstab`, `on_mkinitcpio`, `on_add_bootloader`, `on_user_created`,
`on_install`… que se llaman en los puntos donde arranca cada etapa real. El
plugin escribe NDJSON en un archivo que el ayudante sigue como un `tail -f`, y de
ahí salen los pasos de la interfaz.

El mismo plugin es donde va **la post-configuración de VasakOS**. La alternativa
era `custom_commands` en el JSON: quince cadenas de shell sin tests, sin manejo de
errores y sin forma de saber cuál de las quince falló.

Se escribe en un archivo y no en la salida estándar porque ahí escriben también
pacman, `mkinitcpio` y todo lo que archinstall invoca: una línea de JSON partida
por un `print` ajeno es un evento perdido.

---

## Las decisiones que hay que conocer antes de tocar algo

### El particionado se calcula acá, y es el código que puede borrar datos

archinstall **no propone ninguna distribución de particiones desde un archivo**:
su `disk_config` espera la lista completa con posiciones y tamaños exactos, y su
sugerencia automática vive en los menús interactivos que no usamos.

`layout.rs` es una función pura por eso: recibe un disco y devuelve particiones,
así que se puede probar con cien discos distintos sin tocar ninguno. Lo que fija:

- **1 MiB inicial** para el MBR protector y la cabecera GPT; alinea todo lo demás.
- **ESP de 1 GiB**, no de 512 MiB. En `/boot` viven el kernel y **los dos**
  initramfs, y cada actualización los reescribe: con 512 MiB, un sistema con dos
  kernels y microcódigo queda al borde, y `pacman` fallando por espacio en `/boot`
  deja un equipo que no arranca.
- **1 MiB reservado al final** para la cabecera GPT secundaria.
- **La raíz btrfs va sin punto de montaje propio.** Con `mountpoint` y
  subvolúmenes a la vez, archinstall monta la partición cruda en `/` y después los
  subvolúmenes encima: el sistema termina instalado **afuera** de `@`, y el primer
  arranque encuentra un `@` vacío. Hay un test que lo fija.
- **El ESP nunca va cifrado**: el firmware lo lee antes de que exista nada que
  pueda descifrarlo.
- Los subvolúmenes son los mismos que usaba calamares (`@`, `@home`, `@root`,
  `@srv`, `@cache`, `@tmp`, `@log`) y no los de archinstall, para que un respaldo
  de subvolúmenes hecho con la ISO anterior se restaure en ésta.

### Los dos teclados

Hay **dos** teclados que configurar y no se llaman igual. El de la consola
(`KEYMAP` en `/etc/vconsole.conf`, que es lo que consume archinstall) llama
`la-latin1` al latinoamericano; el del escritorio (el diseño de XKB, que usa
Wayland y por lo tanto Wayfire) lo llama `latam`.

Configurar sólo el primero es el error que se nota tarde y mal: alguien elige su
teclado, la instalación termina bien, y **en el primer arranque no puede escribir
su contraseña** porque el greeter quedó en `us`. La tabla de traducción está en
`teclado.rs`, con tests contra el `base.lst` real de `xkeyboard-config`, y el
plugin la aplica en `/etc/vasak/teclado.conf` y en `/etc/environment.d/`.

### Las contraseñas

Nunca van por `argv` y nunca se escriben en claro. `/proc/<pid>/cmdline` lo puede
leer cualquier usuario, así que una contraseña en la línea de comandos es una
contraseña publicada mientras el proceso vive.

El camino es: la ventana las tiene en memoria → viajan una vez al ayudante por el
canal NDJSON → el ayudante las pasa a `openssl passwd -6` **por entrada
estándar** → al archivo de credenciales va sólo el hash. La ventana las olvida en
cuanto la instalación arranca (`olvidarSecretos()`).

La única que sí va en claro es la frase de LUKS, porque `cryptsetup` necesita la
frase y no un hash. Por eso el archivo de credenciales se escribe en `/run` con
modo `0600` —el modo va en `OpenOptions`, no en un `chmod` posterior, que deja
una ventana abierta— y se borra al terminar.

Se rechazan los caracteres de control porque `openssl passwd -stdin` lee **una
sola línea**: un `\n` en el medio haría hashear algo distinto de lo tipeado.
Espacios y acentos sí se aceptan; rechazarlos sería empobrecer las contraseñas
por comodidad nuestra.

### La lista de paquetes no está compilada

`paquetes.txt` se lee en tiempo de ejecución desde
`/usr/share/vasak-installer/paquetes.txt`, así que sumar un paquete al escritorio
no obliga a recompilar el instalador ni a rehacer la ISO.

Y es **una sola línea**: el escritorio entero es el metapaquete
`vasakos-desktop`, que se arma en `PKGBUILDS/vasakos-desktop/` y arrastra por
dependencia los 255 paquetes que forman VasakOS. `paquetes.txt` lo nombra a él
y agrega el kernel; `archiso/packages.x86_64` lo nombra a él y agrega lo que
sólo tiene sentido en el medio live. Sumar un paquete al escritorio es editarle
las `depends` al metapaquete: ninguna de las dos listas se toca.

Antes cada lista estaba escrita entera, en dos repositorios distintos y
sincronizadas a mano. Divergían, y de la peor manera: sumar un paquete y
olvidarse de la otra lista daba una ISO en la que la función andaba y un sistema
instalado en el que no, diferencia que sólo aparece después de instalar.

### El repositorio de VasakOS va en la configuración de archinstall

En `mirror_config.custom_repositories`. Sin eso, `pacstrap` no encuentra ninguno
de los paquetes `vasak-*` y la instalación muere en el paso del escritorio —
**después** de haber formateado el disco. El plugin lo verifica de nuevo sobre el
sistema instalado en `_asegurar_mirrorlist`.

### La autorización

La acción de polkit (`ar.net.vasak.installer.run-helper`) trae `auth_admin` por
defecto: **pide autenticación**. La regla que la releva sin preguntar
(`49-vasak-installer.rules`) la envía **la ISO**, no el paquete, porque sólo tiene
sentido en el medio live, donde el usuario de autologin no tiene contraseña que
escribir. El plugin la borra del sistema instalado; si sobreviviera, cualquiera
del grupo `wheel` podría lanzar un proceso root sin autenticarse.

La anotación `exec.path` de la acción tiene que coincidir **exactamente** con
dónde el PKGBUILD instala el ayudante. Si no coinciden, pkexec cae en la acción
genérica y el diálogo dice «ejecutar un programa como otro usuario» en vez de
explicar que se va a instalar el sistema.

---

## Desarrollo

```bash
bun install
bun test                                          # frontend
cargo test --manifest-path src-tauri/Cargo.toml   # backend
bun run lint
bunx --bun tauri dev
```

**Compilá siempre con `tauri build`, nunca con `cargo build --release` a secas.**
Con `cargo` el binario queda apuntando al servidor de desarrollo: la página
«carga» vacía y todo parece roto por otra razón.

Para probar el ayudante sin instalar el paquete:

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

y apuntar `VASAK_INSTALLER_HELPER` al binario que quedó en
`src-tauri/target/debug/vasak-installer-helper`. Como esa ruta no coincide con la
anotación de la acción de polkit, pkexec va a pedir contraseña de administrador:
es lo esperado fuera del medio live.

### Probar la instalación sin arriesgar nada

En una máquina virtual con un disco vacío. `archinstall --dry-run` existe, pero
el instalador no lo expone todavía: es lo primero que conviene sumar si se va a
trabajar sobre el flujo de instalación.

---

## Tests

**Cada cambio va con tests, en el mismo commit.** Lo que conviene probar es lo
que se rompe callado. Dos ejemplos de este repo, los dos encontrados por sus
tests antes de llegar a ninguna máquina:

- **`lsblk --json` sin `--tree` devuelve una lista plana**, con las particiones
  como hermanas de su propio disco y no anidadas. Leer `children` daba todo disco
  sin particiones — y con eso se caía la comprobación de «está en uso», que es la
  que impide formatear el pendrive del que se arrancó, porque el disco no está
  montado: lo está su partición. Ahora se asocia por `PKNAME`.
- **`lsblk` informa `/dev/zram0` con tipo `disk`**, y VasakOS activa zram por
  defecto, así que aparece en todo equipo. Sin el filtro de pseudodispositivos se
  ofrecía como destino de instalación.

Verificá que un test sirve reintroduciendo el bug a propósito y viendo que falle.

---

## Lo que falta

- **Probarlo de verdad.** Nada de esto se ejecutó todavía contra un disco: hace
  falta una máquina virtual con la ISO armada.
- **`--dry-run` expuesto en la interfaz**, para poder recorrer el flujo entero
  sin escribir.
- **Usar particiones existentes** en vez de borrar el disco entero. La estructura
  ya lo contempla (`EsquemaDisco` tiene una sola variante y `wipe` está en una
  sola clave), pero no hay interfaz.
- **Progreso fino durante `pacstrap`**, que es el paso largo. Hoy la barra se
  mueve por etapas; el conteo `(12/1543)` de pacman está en el registro y se
  podría parsear.

## Licencia

GPL-3.0-or-later.
