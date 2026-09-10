//! Planificación de la tabla de particiones.
//!
//! archinstall **no propone nada** desde un archivo de configuración: su
//! `disk_config` con `config_type: "default_layout"` igual espera la lista
//! completa de particiones con sus posiciones y tamaños exactos. La sugerencia
//! automática vive en sus menús interactivos, que es justamente lo que no
//! usamos. Así que el plan lo armamos acá.
//!
//! Que sea una función pura es a propósito: **es el único código del instalador
//! cuyo error borra datos.** Una función que recibe un disco y devuelve una
//! lista de particiones se puede probar con cien discos distintos sin tocar
//! ninguno; el mismo cálculo hecho al vuelo mientras se escribe la tabla, no.
//!
//! Todo el aritmético va en MiB enteros. Con bytes o con flotantes los inicios
//! terminan sin alinear a 1 MiB, y una partición desalineada en un SSD escribe
//! de a dos bloques donde debería escribir uno.

use serde::{Deserialize, Serialize};

use crate::protocol::{AsignacionManual, EsquemaDisco, SistemaArchivos};

/// Un MiB en bytes.
const MIB: u64 = 1024 * 1024;

/// Dónde empieza la primera partición.
///
/// El MiB inicial no es desperdicio: ahí van el MBR protector y la cabecera GPT
/// primaria con su tabla de entradas, y arrancar en 1 MiB alinea todo lo que
/// sigue.
const INICIO_MIB: u64 = 1;

/// La partición de arranque, montada en `/boot`.
///
/// La misma medida en UEFI y en BIOS: cambia el sistema de archivos y la bandera,
/// no para qué sirve.
///
/// 1 GiB y no 512 MiB porque en `/boot` viven el kernel y **los dos** initramfs
/// (el normal y el de respaldo), y cada actualización de kernel los reescribe.
/// Con 512 MiB un sistema con dos kernels y microcódigo queda al borde, y
/// `pacman` fallando por espacio en `/boot` deja un sistema que no arranca.
const ARRANQUE_MIB: u64 = 1024;


/// Lo que se deja libre al final del disco.
///
/// La cabecera GPT **secundaria** y su copia de la tabla van en los últimos
/// sectores, así que una raíz que llega hasta el final del disco no cabe. 1 MiB
/// las cubre con margen y mantiene la alineación.
const RESERVA_FINAL_MIB: u64 = 1;

/// El disco más chico que aceptamos.
///
/// Con menos no entra el escritorio: el sistema base más los paquetes de
/// VasakOS pasan holgadamente los 10 GiB, y dejar instalar en 12 GiB produce un
/// `pacstrap` que muere por espacio a los veinte minutos. Es mejor decirlo
/// antes de formatear.
pub const MINIMO_GIB: u64 = 20;

/// Cuánto tiene que medir un ESP que ya existe para poder reusarlo.
///
/// No es una preferencia: es el mismo cálculo que fija `ARRANQUE_MIB`. En
/// VasakOS el ESP se monta en `/boot`, así que adentro viven el kernel, **los
/// dos** initramfs —el normal y el de respaldo— y el microcódigo, y cada
/// actualización de kernel los reescribe. Ahí está anotado por qué 512 MiB deja
/// al sistema al borde y por qué se eligió 1 GiB.
///
/// 512 es entonces el piso absoluto, no lo recomendable. Se acepta porque un
/// ESP ajeno es lo que es y no se puede agrandar sin mover particiones.
///
/// **Un Windows recién instalado no llega**: su ESP es de 100 MiB, porque ahí
/// adentro guarda sólo su cargador. Ese caso no se rechaza: se le crea uno
/// propio al lado y el de Windows no se toca. Ver `esp_del_plan`.
const MINIMO_ESP_REUSABLE_MIB: u64 = 512;

/// Un disco tal como lo ve el sondeo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disco {
    /// `/dev/nvme0n1`, `/dev/sda`.
    pub ruta: String,
    /// Lo que muestra la interfaz: `Samsung SSD 980 1TB`.
    pub modelo: String,
    pub tamano_bytes: u64,
    /// Tamaño de sector **lógico**. archinstall lo quiere dentro de cada
    /// tamaño, y en los discos de 4Kn es 4096, no 512.
    pub sector_logico: u64,
    /// `true` en discos mecánicos. Cambia las opciones de montaje de btrfs.
    pub rotacional: bool,
    /// `true` si es NVMe. También cambia las opciones de montaje.
    pub nvme: bool,
    /// `true` si el disco o alguna de sus particiones está montada ahora mismo.
    /// El disco del que arrancó la ISO cae acá, y ofrecerlo sería ofrecer
    /// borrar el instalador en marcha.
    pub en_uso: bool,
    /// Lo que hay hoy, para que el resumen pueda decir qué se va a perder.
    pub particiones: Vec<ParticionExistente>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticionExistente {
    pub ruta: String,
    /// Dónde empieza en el disco. Hace falta para dos cosas que antes no se
    /// hacían: encontrar los huecos libres, y decirle a archinstall que no
    /// toque lo que ya está.
    #[serde(default)]
    pub inicio_bytes: u64,
    pub tamano_bytes: u64,
    pub sistema_archivos: Option<String>,
    pub etiqueta: Option<String>,
    /// El número en la tabla de particiones. Sólo para los mensajes.
    #[serde(default)]
    pub numero: Option<u32>,
    /// El GUID del tipo de partición en GPT.
    #[serde(default)]
    pub tipo_particion: Option<String>,
    /// Lo que se pudo averiguar del sistema operativo que vive ahí, si hay uno.
    pub sistema_operativo: Option<String>,
}

/// El GUID que GPT le da a la partición de sistema EFI.
///
/// Se compara contra esto y no contra `fstype == "vfat"`: un equipo con Windows
/// suele tener además una partición FAT de recuperación o de datos, y
/// formatearla creyendo que era el ESP borra justo lo que se venía a conservar.
const GUID_ESP: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

impl ParticionExistente {
    /// Si es la partición de sistema EFI.
    pub fn es_esp(&self) -> bool {
        self.tipo_particion
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case(GUID_ESP))
    }

    /// El byte siguiente al último que ocupa.
    pub fn fin_bytes(&self) -> u64 {
        self.inicio_bytes.saturating_add(self.tamano_bytes)
    }
}

/// El firmware del equipo. Decide si hay ESP o si no hay partición de arranque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Firmware {
    Uefi,
    Bios,
}

/// Una partición del plan. Todavía no es JSON de archinstall: eso lo hace
/// `archconfig.rs`. Acá están los números y las decisiones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticionPlaneada {
    pub inicio_mib: u64,
    pub tamano_mib: u64,
    /// Siempre presente: archinstall lo exige para toda partición que crea.
    pub sistema_archivos: Option<&'static str>,
    /// `None` en la raíz btrfs con
    /// subvolúmenes: ahí el punto de montaje lo lleva el subvolumen `@`, y
    /// poner los dos hace que archinstall monte la partición encima de sus
    /// propios subvolúmenes.
    pub punto_montaje: Option<&'static str>,
    pub opciones_montaje: Vec<String>,
    pub banderas: Vec<&'static str>,
    /// Subvolúmenes btrfs, con el nombre tal cual va al disco.
    pub subvolumenes: Vec<(&'static str, &'static str)>,
    /// Si esta partición va cifrada con LUKS.
    pub cifrada: bool,
    /// Para los mensajes de la interfaz: qué es esta partición.
    pub rol: Rol,
    /// Qué se le hace: crearla, formatearla o dejarla como está.
    pub accion: Accion,
    /// La ruta en `/dev`. `None` en las que se crean, porque todavía no
    /// existen. **Obligatoria** en las otras dos: archinstall rechaza una
    /// partición `existing` o `modify` sin `dev_path`
    /// (`models/device.py:904`).
    pub ruta: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rol {
    /// La partición de arranque: el ESP en UEFI, `/boot` a secas en BIOS.
    Esp,
    Raiz,
    /// Una partición que se monta en otro lado: `/home`, `/var`, `/srv`. Sólo
    /// aparece en el modo manual, que es el único donde se pueden elegir.
    ///
    /// Existe para que la interfaz no la llame «Sistema». Un `/home` de 200
    /// GiB rotulado como el sistema es exactamente el tipo de detalle que hace
    /// dudar de si uno entendió bien la pantalla.
    Datos,
}

/// Qué se le hace a una partición del plan.
///
/// Hasta ahora esta distinción no existía porque el instalador sabía hacer una
/// sola cosa con un disco: borrarlo entero. Para instalar al lado de otro
/// sistema hay que poder decir «esto lo creo», «esto ya está y lo formateo» y
/// «esto ya está y no lo toco».
///
/// Los tres nombres se eligieron por lo que significan **en el disco**, y cada
/// uno se traduce a un `status` de archinstall en `archconfig.rs`. Lo que
/// archinstall hace con cada uno está medido leyendo `device_handler.py` y
/// `filesystem.py`, no supuesto:
///
/// | acá | archinstall | tabla de particiones | formateo |
/// |---|---|---|---|
/// | `Crear` | `create` | se crea | sí |
/// | `Formatear` | `modify` | se borra y se recrea igual | sí |
/// | `Conservar` | `existing` | no se toca | **no** |
///
/// `existing` es la única que no formatea: `_format_partitions` filtra por
/// `is_create_or_modify()`, y `partition()` filtra por `not p.exists()` antes
/// de tocar la tabla. Es lo que permite reusar el ESP de Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accion {
    /// No existe: se crea en la geometría del plan y se formatea.
    Crear,
    /// Existe: se rehace con la misma geometría y se formatea. Los datos que
    /// tuviera se pierden.
    Formatear,
    /// Existe y **no se toca**. Sólo se monta donde diga el plan.
    Conservar,
}

/// Los subvolúmenes, con los mismos nombres y puntos de montaje que venía
/// usando la configuración de calamares.
///
/// Se conservan tal cual y no se adopta el juego de archinstall (`@`, `@home`,
/// `@log`, `@pkg`, `.snapshots`) porque un sistema instalado con la ISO anterior
/// y uno instalado con ésta tienen que verse igual: si los nombres cambian, un
/// respaldo de subvolúmenes hecho con la ISO vieja no se restaura en la nueva, y
/// nadie se enteraría hasta necesitarlo.
///
/// `/var/cache` como subvolumen aparte es lo que permite excluir la caché de
/// pacman de una instantánea sin excluir `/var` entero.
const SUBVOLUMENES: &[(&str, &str)] = &[
    ("@", "/"),
    ("@home", "/home"),
    ("@root", "/root"),
    ("@srv", "/srv"),
    ("@cache", "/var/cache"),
    ("@tmp", "/var/tmp"),
    ("@log", "/var/log"),
];

/// Por qué un disco no se puede usar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorPlan {
    /// Más chico que `MINIMO_GIB`.
    Chico { tiene_gib: u64, minimo_gib: u64 },
    /// Está montado. Casi siempre es el pendrive de la ISO.
    EnUso,
    /// Tamaño cero o sector lógico absurdo: el disco desapareció entre el
    /// sondeo y ahora, o `lsblk` devolvió algo que no se puede usar.
    Invalido,
    /// No hay un hueco libre lo bastante grande para instalar al lado.
    SinEspacioLibre { mayor_hueco_gib: u64 },
    /// Hay un ESP pero no entra lo que VasakOS pone adentro. El caso típico es
    /// un Windows de fábrica, que hace uno de 100 MiB.
    EspChico { tiene_mib: u64, minimo_mib: u64 },
    /// Instalar al lado de otro sistema sólo está resuelto en UEFI. En BIOS la
    /// tabla es MBR, que admite cuatro particiones primarias y ya suele
    /// tenerlas ocupadas, y no hay ESP que reusar.
    SoloUefi,
    /// Se pidió instalar sobre una partición que el disco no tiene. El sondeo
    /// es de antes: alguien pudo haber cambiado el disco en el medio.
    ParticionNoEsta { ruta: String },
    /// La partición elegida no empieza ni termina en un MiB entero.
    ///
    /// archinstall la borra y la rehace con `optimalAlignedConstraint`, así que
    /// no la puede reproducir tal cual: la correría, y correrla es meterse en la
    /// de al lado. Se rechaza en vez de mover nada.
    ParticionDesalineada { ruta: String },
    /// Se pidió instalar sobre el ESP. Formatearlo como raíz deja al equipo sin
    /// partición de arranque, y de paso borra el cargador del otro sistema.
    ParticionEsElEsp { ruta: String },
    /// En el modo manual: un punto de montaje que no está en
    /// `PUNTOS_MANUALES`.
    PuntoDeMontajeDesconocido { punto: String },
    /// En el modo manual: el mismo punto de montaje dos veces, o la misma
    /// partición asignada dos veces.
    AsignacionRepetida { que: String },
    /// En el modo manual: no se eligió ninguna partición para `/`.
    SinRaiz,
    /// En el modo manual: no se eligió ninguna para `/boot`.
    SinArranque,
    /// En el modo manual: la raíz sin formatear. Instalar sobre un sistema de
    /// archivos que ya tiene cosas deja dos sistemas mezclados en el mismo
    /// árbol de directorios.
    LaRaizSeFormatea,
    /// En el modo manual y UEFI: se asignó `/boot` a una partición que no es
    /// el ESP. La instalación no arrancaría, y se descubriría al reiniciar.
    ArranqueNoEsEsp { ruta: String },
    /// En el modo manual: se quiso conservar y montar una partición cifrada.
    /// Abrir volúmenes LUKS que no son la raíz todavía no se hace, así que
    /// montarla fallaría a mitad de la instalación.
    ParticionCifrada { ruta: String },
}

impl std::fmt::Display for ErrorPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorPlan::Chico {
                tiene_gib,
                minimo_gib,
            } => write!(
                f,
                "el disco tiene {tiene_gib} GiB y hacen falta al menos {minimo_gib} GiB"
            ),
            ErrorPlan::EnUso => write!(f, "el disco está en uso"),
            ErrorPlan::Invalido => write!(f, "el disco informa un tamaño o un sector inválidos"),
            ErrorPlan::SinEspacioLibre { mayor_hueco_gib } => write!(
                f,
                "no hay espacio libre suficiente: el hueco más grande es de {mayor_hueco_gib} GiB y hacen falta {MINIMO_GIB}"
            ),
            ErrorPlan::EspChico {
                tiene_mib,
                minimo_mib,
            } => write!(
                f,
                "la partición EFI que ya existe tiene {tiene_mib} MiB y hacen falta al menos {minimo_mib}"
            ),
            ErrorPlan::SoloUefi => write!(
                f,
                "instalar junto a otro sistema sólo está disponible en equipos UEFI"
            ),
            ErrorPlan::ParticionNoEsta { ruta } => {
                write!(f, "el disco ya no tiene la partición {ruta}")
            }
            ErrorPlan::ParticionDesalineada { ruta } => write!(
                f,
                "la partición {ruta} no empieza y termina en un MiB entero, y rehacerla la movería"
            ),
            ErrorPlan::ParticionEsElEsp { ruta } => write!(
                f,
                "{ruta} es la partición de arranque EFI: usarla como raíz dejaría el equipo sin arrancar"
            ),
            ErrorPlan::PuntoDeMontajeDesconocido { punto } => write!(
                f,
                "«{punto}» no es un punto de montaje que se pueda elegir: {}",
                PUNTOS_MANUALES.join(", ")
            ),
            ErrorPlan::AsignacionRepetida { que } => {
                write!(f, "{que} está asignado dos veces")
            }
            ErrorPlan::SinRaiz => write!(f, "falta elegir en qué partición va el sistema (/)"),
            ErrorPlan::SinArranque => write!(
                f,
                "falta elegir la partición de arranque EFI (/boot)"
            ),
            ErrorPlan::LaRaizSeFormatea => write!(
                f,
                "la partición del sistema tiene que formatearse: instalar encima de lo que ya hay deja dos sistemas mezclados"
            ),
            ErrorPlan::ArranqueNoEsEsp { ruta } => write!(
                f,
                "{ruta} no es una partición de sistema EFI, y en UEFI el arranque tiene que serlo"
            ),
            ErrorPlan::ParticionCifrada { ruta } => write!(
                f,
                "{ruta} está cifrada: todavía no se puede conservar y montar una partición cifrada que no sea la raíz"
            ),
        }
    }
}

/// Arma el plan de particionado para borrar el disco entero.
///
/// Devuelve las particiones en orden de posición en el disco. El orden de
/// **montaje** no es éste y no se calcula acá: archinstall ordena los puntos de
/// montaje por profundidad antes de montar (`mount_ordered_layout`), que es lo
/// que garantiza que `/` se monte antes que `/boot`.
pub fn planificar(
    disco: &Disco,
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Vec<ParticionPlaneada>, ErrorPlan> {
    if disco.tamano_bytes == 0 || disco.sector_logico == 0 {
        return Err(ErrorPlan::Invalido);
    }
    if disco.en_uso {
        return Err(ErrorPlan::EnUso);
    }

    let total_mib = disco.tamano_bytes / MIB;
    let tiene_gib = disco.tamano_bytes / (1024 * MIB);
    if tiene_gib < MINIMO_GIB {
        return Err(ErrorPlan::Chico {
            tiene_gib,
            minimo_gib: MINIMO_GIB,
        });
    }

    let mut plan = Vec::with_capacity(2);
    let mut cursor = INICIO_MIB;

    // Las dos ramas crean una partición de arranque de 1 GiB montada en `/boot`.
    // Lo que cambia es el sistema de archivos y la bandera.
    match firmware {
        Firmware::Uefi => {
            plan.push(ParticionPlaneada {
                inicio_mib: cursor,
                tamano_mib: ARRANQUE_MIB,
                sistema_archivos: Some("fat32"),
                punto_montaje: Some("/boot"),
                // El ESP es FAT y FAT no tiene permisos: sin `umask` queda
                // legible por cualquiera, y ahí están el kernel y el initramfs.
                // `0077` lo deja sólo para root, que es quien lo escribe.
                opciones_montaje: vec!["umask=0077".into()],
                // Las dos: `esp` es la que marca la partición como Sistema EFI
                // en GPT, y `boot` es como la nombra parted —que es la
                // herramienta que archinstall usa por debajo.
                banderas: vec!["boot", "esp"],
                subvolumenes: Vec::new(),
                // El ESP **nunca** va cifrado: el firmware tiene que poder
                // leerlo para arrancar.
                cifrada: false,
                rol: Rol::Esp,
                // Borrar el disco entero: todo se crea de cero.
                accion: Accion::Crear,
                ruta: None,
            });
            cursor += ARRANQUE_MIB;
        }
        // En BIOS va un `/boot` de verdad: ext4 y con la bandera `boot`.
        //
        // No es simetría por prolijidad, es lo que archinstall **exige**.
        // `add_bootloader` empieza con
        //
        //     boot_partition = self._get_boot_partition()
        //     if boot_partition is None:
        //         raise ValueError(f'Could not detect boot at mountpoint {self.target}')
        //
        // y `get_boot_partition` filtra por `x.is_boot() and x.mountpoint`, o sea
        // bandera `boot` **y** punto de montaje. Sin una partición así la
        // instalación muere al llegar al gestor de arranque, con el disco ya
        // formateado y los paquetes ya instalados — el peor momento posible.
        //
        // En BIOS la usa nada más que para deducir el disco:
        //
        //     parent_dev_path = get_parent_device_path(boot_partition.safe_dev_path)
        //     ['--target=i386-pc', '--recheck', str(parent_dev_path)]
        //
        // Hubo dos intentos antes de éste, y ninguno servía:
        //
        // 1. Una `bios_grub` de 2 MiB, que es lo que pide GPT. Pero archinstall
        //    elige la tabla por el firmware —`PartitionTable.default()` devuelve
        //    MBR sin UEFI— y en MBR no va; además, sin sistema de archivos
        //    `_setup_partition` moría en `safe_fs_type`, y `PartitionFlag` no
        //    conoce `bios_grub` (sólo boot, xbootldr, esp, linux-home y swap), así
        //    que la bandera se descartaba en silencio.
        // 2. Ninguna partición de arranque. Eso arregló el particionado y dejó
        //    este error, que es el que aparece ahora en el registro.
        //
        // Poner la bandera en la raíz tampoco alcanza: con btrfs la raíz no lleva
        // punto de montaje —lo lleva el subvolumen `@`— y el filtro pide los dos.
        // Y btrfs es el sistema por defecto, así que fallaría justo en el camino
        // más usado.
        //
        // GRUB sigue escribiendo su segunda etapa en el hueco entre el MBR y la
        // primera partición, que existe porque `INICIO_MIB` arranca en el MiB 1.
        Firmware::Bios => {
            plan.push(ParticionPlaneada {
                inicio_mib: cursor,
                tamano_mib: ARRANQUE_MIB,
                // ext4 y no fat32: acá no hay firmware que tenga que leerlo, y
                // ext4 conserva permisos y no se corrompe con un apagón a medio
                // escribir el initramfs.
                sistema_archivos: Some("ext4"),
                punto_montaje: Some("/boot"),
                opciones_montaje: Vec::new(),
                // Sólo `boot`: `esp` marcaría una partición de sistema EFI en un
                // disco que arranca por BIOS.
                banderas: vec!["boot"],
                subvolumenes: Vec::new(),
                cifrada: false,
                rol: Rol::Esp,
                // Borrar el disco entero: todo se crea de cero.
                accion: Accion::Crear,
                ruta: None,
            });
            cursor += ARRANQUE_MIB;
        }
    }

    // Lo que sobra, menos la reserva del final. La resta se hace con
    // `saturating_sub` y después se verifica: en un disco justo al límite un
    // desbordamiento daría una partición gigante y `parted` fallaría con un
    // error sobre sectores que no le dice nada a nadie.
    let tamano_raiz = total_mib
        .saturating_sub(cursor)
        .saturating_sub(RESERVA_FINAL_MIB);
    if tamano_raiz == 0 {
        return Err(ErrorPlan::Chico {
            tiene_gib,
            minimo_gib: MINIMO_GIB,
        });
    }

    let usa_subvolumenes = fs == SistemaArchivos::Btrfs;
    plan.push(ParticionPlaneada {
        inicio_mib: cursor,
        tamano_mib: tamano_raiz,
        sistema_archivos: Some(fs.como_archinstall()),
        // Con subvolúmenes el punto de montaje lo lleva `@`. Poner los dos hace
        // que archinstall monte la partición cruda en `/` y después los
        // subvolúmenes encima, y el sistema termina instalado fuera de `@`.
        punto_montaje: if usa_subvolumenes { None } else { Some("/") },
        opciones_montaje: opciones_de_montaje(fs, disco),
        banderas: Vec::new(),
        subvolumenes: if usa_subvolumenes {
            SUBVOLUMENES.to_vec()
        } else {
            Vec::new()
        },
        cifrada: cifrar,
        rol: Rol::Raiz,
        accion: Accion::Crear,
        ruta: None,
    });

    Ok(plan)
}

/// El plan completo: qué particiones quedan y si la tabla se rehace de cero.
///
/// Antes alcanzaba con la lista de particiones porque el instalador hacía una
/// sola cosa. Instalando al lado de otro sistema hay dos datos más que el resto
/// del programa necesita y que no se pueden deducir mirando la lista: si se
/// borra la tabla entera, y **qué se va a destruir**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Si se rehace la tabla de particiones desde cero. Es el `wipe` de
    /// archinstall, y es el punto sin retorno.
    pub borrar_disco: bool,
    pub particiones: Vec<ParticionPlaneada>,
}

impl Plan {
    /// Qué particiones que hoy existen van a dejar de existir.
    ///
    /// Es la red de seguridad: esta lista es la que se le muestra a la persona
    /// para confirmar, nombrando el sistema operativo detectado en cada una, y
    /// **nada fuera de esta lista se puede tocar**.
    ///
    /// Se calcula desde el plan y no se va anotando mientras se arma, a
    /// propósito: así no puede pasar que alguien agregue un modo nuevo y se
    /// olvide de agregar lo que destruye. Si el plan borra el disco, la lista
    /// es todo; si no, es lo que alguna partición del plan pisa.
    pub fn a_destruir<'a>(&self, disco: &'a Disco) -> Vec<&'a ParticionExistente> {
        if self.borrar_disco {
            return disco.particiones.iter().collect();
        }
        disco
            .particiones
            .iter()
            .filter(|existente| {
                self.particiones.iter().any(|p| {
                    // Conservar es justamente no destruir.
                    if p.accion == Accion::Conservar {
                        return false;
                    }
                    // Formatear nombra su víctima por la ruta.
                    if p.ruta.as_deref() == Some(existente.ruta.as_str()) {
                        return true;
                    }
                    // Y crear destruye lo que se solape con su geometría, que
                    // no debería pasar nunca — pero si el cálculo de huecos se
                    // equivoca, es acá donde tiene que verse.
                    let inicio = p.inicio_mib.saturating_mul(MIB);
                    let fin = inicio.saturating_add(p.tamano_mib.saturating_mul(MIB));
                    inicio < existente.fin_bytes() && existente.inicio_bytes < fin
                })
            })
            .collect()
    }
}

/// Un tramo de disco sin usar, en MiB y ya alineado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hueco {
    inicio_mib: u64,
    tamano_mib: u64,
}

/// Los tramos libres de un disco, de mayor a menor.
///
/// Alineados a 1 MiB por los dos lados: el inicio se redondea hacia arriba y el
/// final hacia abajo, porque archinstall rechaza una partición que no esté
/// alineada (`models/device.py:213`) y porque una partición desalineada en un
/// SSD escribe dos bloques cada vez que toca uno.
///
/// El último MiB del disco no se ofrece: ahí va la copia de la tabla GPT.
fn huecos_libres(disco: &Disco) -> Vec<Hueco> {
    let total_mib = disco.tamano_bytes / MIB;
    let fin_utilizable = total_mib.saturating_sub(RESERVA_FINAL_MIB);

    let mut ocupados: Vec<(u64, u64)> = disco
        .particiones
        .iter()
        .map(|p| (p.inicio_bytes, p.fin_bytes()))
        .collect();
    ocupados.sort_unstable();

    let mut huecos = Vec::new();
    // Arranca en el MiB 1 y no en 0: el principio del disco es la tabla de
    // particiones.
    let mut cursor = INICIO_MIB;

    for (inicio_b, fin_b) in ocupados {
        // Hacia abajo: un hueco que termina a mitad de un MiB termina antes.
        let inicio_mib = inicio_b / MIB;
        if inicio_mib > cursor {
            huecos.push(Hueco {
                inicio_mib: cursor,
                tamano_mib: inicio_mib - cursor,
            });
        }
        // Hacia arriba: si una partición termina a mitad de un MiB, el hueco
        // empieza en el siguiente. Redondear hacia abajo pondría el borde
        // adentro de la partición ajena.
        let fin_mib = fin_b.div_ceil(MIB);
        cursor = cursor.max(fin_mib);
    }

    if fin_utilizable > cursor {
        huecos.push(Hueco {
            inicio_mib: cursor,
            tamano_mib: fin_utilizable - cursor,
        });
    }

    // De mayor a menor: `Reverse` en vez de invertir el `cmp`, que es lo que
    // clippy pide para poder usar la variante por clave.
    huecos.sort_unstable_by_key(|h| std::cmp::Reverse(h.tamano_mib));
    huecos
}

/// Lo que se comprueba del disco antes de cualquier plan.
///
/// El tamaño mínimo no está acá: en los modos no destructivos lo que tiene que
/// entrar es la raíz, que es más chica que el disco.
fn comprobar_disco(disco: &Disco) -> Result<(), ErrorPlan> {
    if disco.tamano_bytes == 0 || disco.sector_logico == 0 {
        return Err(ErrorPlan::Invalido);
    }
    if disco.en_uso {
        return Err(ErrorPlan::EnUso);
    }
    Ok(())
}

/// El ESP del plan: el que ya está si sirve, o uno nuevo al principio del hueco.
///
/// Está separado porque los dos modos no destructivos lo necesitan igual, y
/// porque es la decisión de la que depende que el otro sistema siga arrancando.
/// Duplicarla sería tener dos lugares donde equivocarse en eso.
///
/// Devuelve además cuántos MiB del hueco se usaron: cero si se reusó uno que ya
/// existe, y `ARRANQUE_MIB` si hubo que crearlo.
fn esp_del_plan(disco: &Disco, hueco: Hueco) -> Result<(ParticionPlaneada, u64), ErrorPlan> {
    let comun = |accion, ruta, inicio_mib, tamano_mib| ParticionPlaneada {
        inicio_mib,
        tamano_mib,
        sistema_archivos: Some("fat32"),
        punto_montaje: Some("/boot"),
        // El ESP es FAT y FAT no tiene permisos: sin `umask` queda legible por
        // cualquiera, y ahí están el kernel y el initramfs.
        opciones_montaje: vec!["umask=0077".into()],
        banderas: vec!["boot", "esp"],
        subvolumenes: Vec::new(),
        // El ESP **nunca** va cifrado: el firmware tiene que poder leerlo.
        cifrada: false,
        rol: Rol::Esp,
        accion,
        ruta,
    };

    match disco.particiones.iter().find(|p| p.es_esp()) {
        Some(esp) => {
            // Igual que en los otros dos modos. Acá archinstall hoy ni mira la
            // geometría de una `existing` —`partition()` filtra por
            // `not p.exists()`— así que truncar a MiB no rompería nada todavía.
            // Pero mandar números que no son los de la partición es apoyarse en
            // que eso siga siendo cierto, y la regla vale igual: lo que se
            // reusa se describe tal como está o no se reusa.
            if esp.inicio_bytes % MIB != 0 || esp.tamano_bytes % MIB != 0 {
                return Err(ErrorPlan::ParticionDesalineada {
                    ruta: esp.ruta.clone(),
                });
            }
            let mib = esp.tamano_bytes / MIB;
            if mib < MINIMO_ESP_REUSABLE_MIB {
                // Demasiado chico para lo que ponemos adentro, así que **no se
                // usa** — y tampoco se rechaza la instalación. Se hace uno
                // propio en el hueco libre y el de Windows queda intacto:
                // sigue teniendo su cargador y sigue arrancando.
                //
                // Es el caso normal, no el raro: Windows hace su ESP de 100
                // MiB porque ahí guarda sólo el cargador. Nosotros guardamos
                // además el kernel y los dos initramfs.
                //
                // La otra salida sería la de CachyOS con GRUB —ESP en
                // `/boot/efi` y el kernel en la raíz, que entra en 100 MiB—,
                // pero con archinstall no sale limpia: `_add_grub_bootloader`
                // le pasa `--boot-directory` a la partición con bandera `boot`
                // en cuanto no se monta en `/boot`, así que GRUB terminaría
                // adentro del ESP ajeno; y con cifrado haría falta
                // `GRUB_ENABLE_CRYPTODISK`, que archinstall no escribe nunca.
                // Ver Vasak-OS/vasak-installer#31.
                return crear_esp(hueco);
            }
            // `Conservar` es lo único que archinstall no formatea, y formatear
            // el ESP ajeno es borrarle el cargador al otro sistema. La
            // geometría va tal como está porque no se la va a tocar; se manda
            // igual porque archinstall la pide.
            Ok((
                comun(
                    Accion::Conservar,
                    Some(esp.ruta.clone()),
                    esp.inicio_bytes / MIB,
                    mib,
                ),
                0,
            ))
        }
        // Sin ESP no hay nada que reusar: un disco con un Linux viejo en MBR, o
        // uno con datos y nada más.
        None => crear_esp(hueco),
    }
}

/// Uno propio, al principio del hueco libre.
///
/// Sale aparte porque los dos caminos que llevan acá —que no haya ESP, o que
/// el que hay sea demasiado chico— tienen que producir exactamente lo mismo.
fn crear_esp(hueco: Hueco) -> Result<(ParticionPlaneada, u64), ErrorPlan> {
    if hueco.tamano_mib < ARRANQUE_MIB {
        return Err(ErrorPlan::SinEspacioLibre {
            mayor_hueco_gib: hueco.tamano_mib / 1024,
        });
    }
    Ok((
        ParticionPlaneada {
            inicio_mib: hueco.inicio_mib,
            tamano_mib: ARRANQUE_MIB,
            sistema_archivos: Some("fat32"),
            punto_montaje: Some("/boot"),
            opciones_montaje: vec!["umask=0077".into()],
            banderas: vec!["boot", "esp"],
            subvolumenes: Vec::new(),
            cifrada: false,
            rol: Rol::Esp,
            accion: Accion::Crear,
            ruta: None,
        },
        ARRANQUE_MIB,
    ))
}

/// La partición raíz del plan, con lo que cambia según el sistema de archivos.
fn raiz_del_plan(
    disco: &Disco,
    fs: SistemaArchivos,
    cifrar: bool,
    inicio_mib: u64,
    tamano_mib: u64,
    accion: Accion,
    ruta: Option<String>,
) -> ParticionPlaneada {
    let usa_subvolumenes = fs == SistemaArchivos::Btrfs;
    ParticionPlaneada {
        inicio_mib,
        tamano_mib,
        sistema_archivos: Some(fs.como_archinstall()),
        // Con subvolúmenes el punto de montaje lo lleva `@`. Poner los dos hace
        // que archinstall monte la partición cruda en `/` y después los
        // subvolúmenes encima, y el sistema termina instalado fuera de `@`.
        punto_montaje: if usa_subvolumenes { None } else { Some("/") },
        opciones_montaje: opciones_de_montaje(fs, disco),
        banderas: Vec::new(),
        subvolumenes: if usa_subvolumenes {
            SUBVOLUMENES.to_vec()
        } else {
            Vec::new()
        },
        cifrada: cifrar,
        rol: Rol::Raiz,
        accion,
        ruta,
    }
}

/// Arma el plan para instalar **en el espacio libre**, sin tocar lo que ya está.
///
/// Es lo contrario de `planificar`: ahí el disco queda vacío y el plan lo llena;
/// acá el disco tiene dueño y el plan se acomoda en el hueco más grande que
/// encuentre.
///
/// Sólo UEFI. En BIOS la tabla es MBR —cuatro particiones primarias, que un
/// equipo con otro sistema ya suele tener ocupadas— y no hay ESP que reusar. Es
/// una limitación declarada y no un olvido: `ErrorPlan::SoloUefi`.
///
/// El ESP que ya existe **se reusa sin formatearlo**, que es lo único que deja
/// a Windows arrancando. Si no hay ninguno se crea uno; si hay uno pero es más
/// chico que `MINIMO_ESP_REUSABLE_MIB`, se falla y se dice por qué en vez de
/// crear un segundo ESP, que muchos firmwares no manejan bien.
pub fn planificar_junto_a(
    disco: &Disco,
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Plan, ErrorPlan> {
    comprobar_disco(disco)?;
    if firmware != Firmware::Uefi {
        return Err(ErrorPlan::SoloUefi);
    }

    let mayor = huecos_libres(disco).first().copied().unwrap_or(Hueco {
        inicio_mib: INICIO_MIB,
        tamano_mib: 0,
    });

    let (esp, gastado) = esp_del_plan(disco, mayor)?;

    // Lo que queda del hueco después del ESP tiene que alcanzar para la raíz.
    let para_la_raiz = mayor.tamano_mib.saturating_sub(gastado);
    if para_la_raiz < MINIMO_GIB * 1024 {
        return Err(ErrorPlan::SinEspacioLibre {
            mayor_hueco_gib: mayor.tamano_mib / 1024,
        });
    }

    let mut plan = vec![
        esp,
        raiz_del_plan(
            disco,
            fs,
            cifrar,
            mayor.inicio_mib + gastado,
            para_la_raiz,
            Accion::Crear,
            None,
        ),
    ];

    // El plan queda en orden de posición en el disco. Con el ESP reusado eso no
    // es el orden en que se armó: un ESP de Windows está al principio y el
    // hueco libre casi siempre al final.
    plan.sort_by_key(|p| p.inicio_mib);

    Ok(Plan {
        // Lo que hace que nada de lo que ya está se pierda. Con `true` acá,
        // reusar el ESP no significaría nada: archinstall borraría la tabla
        // antes de mirar el plan.
        borrar_disco: false,
        particiones: plan,
    })
}

/// Arma el plan para instalar **sobre una partición que ya existe**, formateando
/// **sólo esa**.
///
/// Es el caso de quien tiene un disco repartido y quiere entregarle una de las
/// particiones a VasakOS: no hay hueco libre que buscar, hay una partición
/// elegida a mano y todo lo demás se queda como está.
///
/// Sólo UEFI, por lo mismo que `planificar_junto_a`: en BIOS haría falta además
/// formatear otra partición como `/boot`, o sea elegir dos, y eso ya es
/// particionado manual.
///
/// La partición se marca `Formatear`, que en archinstall es `modify`: la borra,
/// la rehace **con la misma geometría** y la formatea. De ahí sale la única
/// condición rara de esta función — que la partición esté alineada a 1 MiB—,
/// porque para rehacerla archinstall usa `optimalAlignedConstraint` y una
/// partición desalineada no la puede reproducir donde estaba: la correría, y
/// correrla es meterse en la de al lado.
pub fn planificar_sobre(
    disco: &Disco,
    particion: &str,
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Plan, ErrorPlan> {
    comprobar_disco(disco)?;
    if firmware != Firmware::Uefi {
        return Err(ErrorPlan::SoloUefi);
    }

    let destino = disco
        .particiones
        .iter()
        .find(|p| p.ruta == particion)
        .ok_or_else(|| ErrorPlan::ParticionNoEsta {
            ruta: particion.to_string(),
        })?;

    // El ESP no: formatearlo como raíz deja el equipo sin partición de arranque
    // y de paso le borra el cargador al otro sistema. Es un error de la interfaz
    // si llega acá, y se corta igual.
    if destino.es_esp() {
        return Err(ErrorPlan::ParticionEsElEsp {
            ruta: particion.to_string(),
        });
    }

    if destino.inicio_bytes % MIB != 0 || destino.tamano_bytes % MIB != 0 {
        return Err(ErrorPlan::ParticionDesalineada {
            ruta: particion.to_string(),
        });
    }

    let tiene_gib = destino.tamano_bytes / (1024 * MIB);
    if tiene_gib < MINIMO_GIB {
        return Err(ErrorPlan::Chico {
            tiene_gib,
            minimo_gib: MINIMO_GIB,
        });
    }

    // El ESP puede necesitar hueco, si el disco no tiene ninguno. El hueco que
    // se le ofrece es el mayor **que no sea la partición elegida**: ésa ya está
    // ocupada por la raíz.
    let mayor = huecos_libres(disco).first().copied().unwrap_or(Hueco {
        inicio_mib: INICIO_MIB,
        tamano_mib: 0,
    });
    let (esp, _) = esp_del_plan(disco, mayor)?;

    let mut plan = vec![
        esp,
        raiz_del_plan(
            disco,
            fs,
            cifrar,
            destino.inicio_bytes / MIB,
            destino.tamano_bytes / MIB,
            Accion::Formatear,
            Some(destino.ruta.clone()),
        ),
    ];
    plan.sort_by_key(|p| p.inicio_mib);

    Ok(Plan {
        borrar_disco: false,
        particiones: plan,
    })
}

/// Los puntos de montaje que se pueden elegir a mano.
///
/// Una lista cerrada y no un campo de texto libre. No es una limitación
/// técnica: un punto de montaje escrito a mano es un lugar donde un error de
/// tipeo —`/hone`, `/boott`— deja el sistema instalado en una carpeta que nadie
/// mira, y el instalador no tiene forma de darse cuenta. Todo lo que se
/// escribiría en un instalador de escritorio está acá.
///
/// `/boot` es el ESP en UEFI, y por eso `planificar_manual` exige que la
/// partición que se le asigne tenga el GUID de sistema EFI.
pub const PUNTOS_MANUALES: &[&str] = &["/", "/boot", "/home", "/opt", "/srv", "/var"];

/// El nombre que archinstall le da a un sistema de archivos que informó
/// `lsblk`, o `None` si no lo conoce.
///
/// Los nombres no coinciden: `lsblk` dice `vfat` donde archinstall dice
/// `fat32`, y `swap` donde dice `linux-swap`. Un nombre que no esté en su
/// `FilesystemType` hace que el JSON entero se rechace con un `ValueError`, así
/// que lo que no se reconoce va como `null` — que archinstall acepta en una
/// partición que se conserva, porque no la va a formatear.
fn fs_para_archinstall(fs: &str) -> Option<&'static str> {
    Some(match fs {
        "vfat" | "fat32" => "fat32",
        "fat16" => "fat16",
        "fat12" => "fat12",
        "swap" => "linux-swap",
        "btrfs" => "btrfs",
        "ext2" => "ext2",
        "ext3" => "ext3",
        "ext4" => "ext4",
        "f2fs" => "f2fs",
        "ntfs" => "ntfs",
        "xfs" => "xfs",
        // `crypto_LUKS` **no** va, aunque archinstall lo tenga en su
        // enumeración: su propia validación lo rechaza —«Crypto luks cannot be
        // set as a filesystem type», `filesystem.py:116`— y además no es algo
        // que se pueda montar. Lo de adentro se monta después de abrir el
        // volumen, y abrir volúmenes que no son la raíz todavía no se hace.
        _ => return None,
    })
}

/// Arma el plan a partir de lo que se eligió partición por partición.
///
/// Es el modo sin barandas, así que las barandas son las comprobaciones de
/// acá. Todo lo que no esté asignado **no se toca**: no aparece en el plan, y
/// por lo tanto tampoco en lo que archinstall ejecuta.
///
/// Lo que se exige, y por qué cada cosa:
///
///   - **Una raíz y una sola.** Sin raíz no hay dónde instalar; con dos,
///     archinstall monta una encima de la otra y el sistema queda repartido.
///   - **La raíz se formatea.** Instalar sobre un sistema de archivos que ya
///     tiene cosas deja dos sistemas mezclados en el mismo árbol.
///   - **`/boot` tiene que ser el ESP**, en UEFI. Asignar una partición común
///     ahí da una instalación que no arranca, y se descubre al reiniciar.
///   - **Todo lo asignado tiene que estar alineado**, por lo mismo que en
///     `planificar_sobre`: para formatear hay que rehacer, y rehacer una
///     partición desalineada la corre encima de la de al lado.
///   - **Nada repetido**: ni dos puntos de montaje iguales ni dos veces la
///     misma partición.
pub fn planificar_manual(
    disco: &Disco,
    asignaciones: &[AsignacionManual],
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Plan, ErrorPlan> {
    comprobar_disco(disco)?;
    if firmware != Firmware::Uefi {
        return Err(ErrorPlan::SoloUefi);
    }

    // Sólo las que se usan. Una asignación sin punto de montaje es «dejala como
    // está», que es lo mismo que no nombrarla.
    let usadas: Vec<&AsignacionManual> = asignaciones
        .iter()
        .filter(|a| a.punto_montaje.is_some())
        .collect();

    let mut vistas_particion: Vec<&str> = Vec::new();
    let mut vistos_punto: Vec<&str> = Vec::new();
    let mut plan: Vec<ParticionPlaneada> = Vec::new();
    let mut hay_raiz = false;

    for a in &usadas {
        let punto_pedido = a.punto_montaje.as_deref().unwrap_or_default();
        let punto = *PUNTOS_MANUALES
            .iter()
            .find(|p| **p == punto_pedido)
            .ok_or_else(|| ErrorPlan::PuntoDeMontajeDesconocido {
                punto: punto_pedido.to_string(),
            })?;

        if vistos_punto.contains(&punto) {
            return Err(ErrorPlan::AsignacionRepetida {
                que: punto.to_string(),
            });
        }
        vistos_punto.push(punto);

        let existente = disco
            .particiones
            .iter()
            .find(|p| p.ruta == a.particion)
            .ok_or_else(|| ErrorPlan::ParticionNoEsta {
                ruta: a.particion.clone(),
            })?;

        if vistas_particion.contains(&existente.ruta.as_str()) {
            return Err(ErrorPlan::AsignacionRepetida {
                que: existente.ruta.clone(),
            });
        }
        vistas_particion.push(&existente.ruta);

        if existente.inicio_bytes % MIB != 0 || existente.tamano_bytes % MIB != 0 {
            return Err(ErrorPlan::ParticionDesalineada {
                ruta: existente.ruta.clone(),
            });
        }

        let es_raiz = punto == "/";
        let es_arranque = punto == "/boot";

        if es_arranque {
            if !existente.es_esp() {
                return Err(ErrorPlan::ArranqueNoEsEsp {
                    ruta: existente.ruta.clone(),
                });
            }
            // El mismo mínimo que en los otros dos modos, y por lo mismo: acá
            // adentro van el kernel y los dos initramfs. Sin esto, el ESP de
            // 100 MiB de un Windows pasaba —tiene el GUID correcto— y la
            // primera actualización de kernel se quedaba sin espacio, que deja
            // un sistema que no arranca.
            let mib = existente.tamano_bytes / MIB;
            if mib < MINIMO_ESP_REUSABLE_MIB {
                return Err(ErrorPlan::EspChico {
                    tiene_mib: mib,
                    minimo_mib: MINIMO_ESP_REUSABLE_MIB,
                });
            }
        }

        // Una partición cifrada que se quiere conservar y montar: no se puede
        // todavía. Se corta con el motivo en vez de mandarla y que archinstall
        // muera al montarla, a mitad de la instalación.
        if !a.formatear && existente.sistema_archivos.as_deref() == Some("crypto_LUKS") {
            return Err(ErrorPlan::ParticionCifrada {
                ruta: existente.ruta.clone(),
            });
        }

        if es_raiz {
            hay_raiz = true;
            if !a.formatear {
                return Err(ErrorPlan::LaRaizSeFormatea);
            }
            let tiene_gib = existente.tamano_bytes / (1024 * MIB);
            if tiene_gib < MINIMO_GIB {
                return Err(ErrorPlan::Chico {
                    tiene_gib,
                    minimo_gib: MINIMO_GIB,
                });
            }
        }

        let accion = if a.formatear {
            Accion::Formatear
        } else {
            Accion::Conservar
        };

        if es_raiz {
            // La raíz lleva los subvolúmenes, las opciones de montaje y el
            // cifrado, igual que en los otros modos: sale de la misma función
            // para que un cambio ahí valga también acá.
            plan.push(raiz_del_plan(
                disco,
                fs,
                cifrar,
                existente.inicio_bytes / MIB,
                existente.tamano_bytes / MIB,
                accion,
                Some(existente.ruta.clone()),
            ));
            continue;
        }

        plan.push(ParticionPlaneada {
            inicio_mib: existente.inicio_bytes / MIB,
            tamano_mib: existente.tamano_bytes / MIB,
            // Si se formatea, con el sistema de archivos elegido; si se
            // conserva, con el que ya tiene —y `null` si archinstall no lo
            // conoce, que en una partición que no va a formatear le da igual.
            sistema_archivos: if a.formatear {
                Some(if es_arranque { "fat32" } else { fs.como_archinstall() })
            } else {
                existente
                    .sistema_archivos
                    .as_deref()
                    .and_then(fs_para_archinstall)
            },
            punto_montaje: Some(punto),
            opciones_montaje: if es_arranque {
                vec!["umask=0077".into()]
            } else {
                Vec::new()
            },
            banderas: if es_arranque {
                vec!["boot", "esp"]
            } else {
                Vec::new()
            },
            subvolumenes: Vec::new(),
            // Sólo la raíz va cifrada. El ESP no puede —el firmware tiene que
            // leerlo— y el resto queda para cuando haya dónde pedir su frase.
            cifrada: false,
            rol: if es_arranque { Rol::Esp } else { Rol::Datos },
            accion,
            ruta: Some(existente.ruta.clone()),
        });
    }

    if !hay_raiz {
        return Err(ErrorPlan::SinRaiz);
    }
    // El arranque lo exige archinstall: `add_bootloader` busca una partición
    // con bandera `boot` y punto de montaje, y sin ella la instalación muere al
    // llegar al cargador, con el disco ya formateado.
    if !vistos_punto.contains(&"/boot") {
        return Err(ErrorPlan::SinArranque);
    }

    plan.sort_by_key(|p| p.inicio_mib);
    Ok(Plan {
        borrar_disco: false,
        particiones: plan,
    })
}

/// El plan que corresponde al esquema elegido.
///
/// **El único despacho.** Existía en dos lados —la vista previa y el ayudante—
/// y eso es lo único que podría hacer que la pantalla muestre un plan y se
/// ejecute otro. Con una sola función, mostrar y hacer no se pueden separar.
///
/// `particion` sólo se mira con `SobreUnaParticion`, y ahí es obligatoria.
pub fn planificar_con(
    disco: &Disco,
    esquema: EsquemaDisco,
    particion: Option<&str>,
    asignaciones: &[AsignacionManual],
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Plan, ErrorPlan> {
    match esquema {
        EsquemaDisco::BorrarTodo => planificar_borrando(disco, firmware, fs, cifrar),
        EsquemaDisco::JuntoAOtroSistema => planificar_junto_a(disco, firmware, fs, cifrar),
        EsquemaDisco::Manual => planificar_manual(disco, asignaciones, firmware, fs, cifrar),
        EsquemaDisco::SobreUnaParticion => {
            // Sin partición no hay nada que decidir, y adivinar cuál sería lo
            // peor que se puede hacer acá.
            let ruta = particion.ok_or_else(|| ErrorPlan::ParticionNoEsta {
                ruta: String::new(),
            })?;
            planificar_sobre(disco, ruta, firmware, fs, cifrar)
        }
    }
}

/// El plan para borrar el disco entero, con la misma forma que el otro.
pub fn planificar_borrando(
    disco: &Disco,
    firmware: Firmware,
    fs: SistemaArchivos,
    cifrar: bool,
) -> Result<Plan, ErrorPlan> {
    Ok(Plan {
        borrar_disco: true,
        particiones: planificar(disco, firmware, fs, cifrar)?,
    })
}

/// Las opciones de montaje de la raíz, según el sistema de archivos y el medio.
///
/// Son las mismas que traía la configuración de calamares, con un cambio: el
/// `defaults` no se pone. `defaults` es la lista implícita de mount y ponerlo
/// junto a `noatime` no agrega nada, pero sí ensucia el fstab.
fn opciones_de_montaje(fs: SistemaArchivos, disco: &Disco) -> Vec<String> {
    match fs {
        SistemaArchivos::Btrfs => {
            // `zstd:1` en NVMe y `zstd` (nivel 3) en el resto. En un NVMe el
            // disco es tan rápido que comprimir más fuerte lo frena en vez de
            // ayudarlo; en un disco mecánico es al revés.
            let compresion = if disco.nvme {
                "compress=zstd:1"
            } else {
                "compress=zstd"
            };
            vec![compresion.into(), "noatime".into()]
        }
        SistemaArchivos::Ext4 => vec!["noatime".into()],
        SistemaArchivos::Xfs => vec![
            "noatime".into(),
            "lazytime".into(),
            "inode64".into(),
            "logbsize=256k".into(),
            "noquota".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::{any, Just, Strategy};

    fn disco_de(tamano_gib: u64) -> Disco {
        Disco {
            ruta: "/dev/sda".into(),
            modelo: "Disco de prueba".into(),
            tamano_bytes: tamano_gib * 1024 * MIB,
            sector_logico: 512,
            rotacional: false,
            nvme: false,
            en_uso: false,
            particiones: Vec::new(),
        }
    }

    /// Un disco con Windows: ESP, la reservada de Microsoft, `C:` y un hueco
    /// libre al final.
    ///
    /// Las medidas son las que deja un Windows 11 de fábrica, con el ESP de
    /// 100 MiB que hace su instalador — que es justamente el caso que no se
    /// puede reusar. `esp_mib` está parametrizado para poder probar los dos.
    fn disco_con_windows(tamano_gib: u64, esp_mib: u64) -> Disco {
        let mut d = disco_de(tamano_gib);
        let esp_fin = (1 + esp_mib) * MIB;
        let msr_fin = esp_fin + 16 * MIB;
        // `C:` ocupa la mitad del disco.
        let c_fin = msr_fin + (tamano_gib * 1024 / 2) * MIB;
        d.particiones = vec![
            ParticionExistente {
                ruta: "/dev/sda1".into(),
                inicio_bytes: MIB,
                tamano_bytes: esp_mib * MIB,
                sistema_archivos: Some("vfat".into()),
                etiqueta: Some("SYSTEM".into()),
                numero: Some(1),
                tipo_particion: Some(GUID_ESP.into()),
                sistema_operativo: None,
            },
            ParticionExistente {
                ruta: "/dev/sda2".into(),
                inicio_bytes: esp_fin,
                tamano_bytes: 16 * MIB,
                sistema_archivos: None,
                etiqueta: None,
                numero: Some(2),
                tipo_particion: Some("e3c9e316-0b5c-4db8-817d-f92df00215ae".into()),
                sistema_operativo: None,
            },
            ParticionExistente {
                ruta: "/dev/sda3".into(),
                inicio_bytes: msr_fin,
                tamano_bytes: c_fin - msr_fin,
                sistema_archivos: Some("ntfs".into()),
                etiqueta: Some("Windows".into()),
                numero: Some(3),
                tipo_particion: Some("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7".into()),
                sistema_operativo: Some("Windows 11".into()),
            },
        ];
        d
    }

    /// **El ESP que ya existe se conserva, no se formatea.**
    ///
    /// Es la diferencia entera entre un dual boot que anda y un Windows que ya
    /// no arranca: el ESP guarda su cargador, y formatearlo lo borra. En el
    /// JSON esto se traduce a `existing`, que es el único estado que
    /// archinstall no formatea.
    #[test]
    fn el_esp_ajeno_se_reusa_sin_tocarlo() {
        for fs in [
            SistemaArchivos::Btrfs,
            SistemaArchivos::Ext4,
            SistemaArchivos::Xfs,
        ] {
            for cifrar in [false, true] {
                let disco = disco_con_windows(500, 512);
                let plan =
                    planificar_junto_a(&disco, Firmware::Uefi, fs, cifrar).unwrap();

                let esp = plan
                    .particiones
                    .iter()
                    .find(|p| p.rol == Rol::Esp)
                    .unwrap_or_else(|| panic!("{fs:?}/{cifrar}: el plan quedó sin ESP"));
                assert_eq!(esp.accion, Accion::Conservar, "{fs:?}/{cifrar}");
                assert_eq!(esp.ruta.as_deref(), Some("/dev/sda1"), "{fs:?}/{cifrar}");
                assert_eq!(esp.punto_montaje, Some("/boot"), "{fs:?}/{cifrar}");
                assert!(!esp.cifrada, "{fs:?}/{cifrar}");
            }
        }
    }

    /// **Nada de lo que ya está se destruye.**
    ///
    /// La red de seguridad del modo no destructivo, dicha como corresponde: no
    /// «el plan parece correcto», sino «la lista de lo que se pierde está
    /// vacía». Cubre también el solapamiento, porque `a_destruir` compara
    /// geometrías y no sólo rutas.
    #[test]
    fn instalar_al_lado_no_destruye_nada() {
        for fs in [
            SistemaArchivos::Btrfs,
            SistemaArchivos::Ext4,
            SistemaArchivos::Xfs,
        ] {
            for cifrar in [false, true] {
                let disco = disco_con_windows(500, 512);
                let plan =
                    planificar_junto_a(&disco, Firmware::Uefi, fs, cifrar).unwrap();
                let victimas: Vec<&str> = plan
                    .a_destruir(&disco)
                    .iter()
                    .map(|p| p.ruta.as_str())
                    .collect();
                assert!(
                    victimas.is_empty(),
                    "{fs:?}/{cifrar}: se destruiría {victimas:?}"
                );
                assert!(!plan.borrar_disco, "{fs:?}/{cifrar}");
            }
        }
    }

    /// **Borrar el disco sí declara todo lo que se pierde.**
    ///
    /// El contrapunto del anterior: si la lista de víctimas volviera vacía
    /// también acá, la pantalla de confirmación diría que no se pierde nada
    /// mientras se borra un Windows.
    #[test]
    fn borrar_el_disco_declara_todas_las_particiones() {
        let disco = disco_con_windows(500, 512);
        let plan =
            planificar_borrando(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        assert!(plan.borrar_disco);
        let victimas: Vec<&str> = plan
            .a_destruir(&disco)
            .iter()
            .map(|p| p.ruta.as_str())
            .collect();
        assert_eq!(victimas, ["/dev/sda1", "/dev/sda2", "/dev/sda3"]);
        assert!(plan
            .a_destruir(&disco)
            .iter()
            .any(|p| p.sistema_operativo.as_deref() == Some("Windows 11")));
    }

    /// **El plan nuevo no se pisa con lo que ya está.**
    ///
    /// archinstall valida esto y aborta con «Partitions overlap», pero para
    /// entonces ya borró la tabla si el modo fuera destructivo. Acá se
    /// comprueba antes, y sobre el hueco de verdad.
    #[test]
    fn lo_que_se_crea_cae_adentro_del_hueco() {
        for esp_mib in [512, 1024] {
            for tamano in [200, 500, 2000] {
                let disco = disco_con_windows(tamano, esp_mib);
                let plan =
                    planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false)
                        .unwrap();

                for p in plan.particiones.iter().filter(|p| p.accion == Accion::Crear) {
                    let inicio = p.inicio_mib * MIB;
                    let fin = inicio + p.tamano_mib * MIB;
                    for e in &disco.particiones {
                        assert!(
                            fin <= e.inicio_bytes || inicio >= e.fin_bytes(),
                            "{tamano} GiB: lo nuevo [{inicio}, {fin}) pisa {} [{}, {})",
                            e.ruta,
                            e.inicio_bytes,
                            e.fin_bytes()
                        );
                    }
                    assert!(
                        fin <= disco.tamano_bytes - MIB,
                        "{tamano} GiB: se mete en la copia de la tabla GPT"
                    );
                    // Alineación: archinstall rechaza lo que no esté en MiB
                    // enteros, y un SSD escribe dos bloques por cada uno.
                    assert_eq!(inicio % MIB, 0);
                    assert_eq!(p.tamano_mib * MIB % MIB, 0);
                }
            }
        }
    }

    /// **Con el ESP de 100 MiB de Windows se hace uno propio, y el de Windows
    /// no se toca.**
    ///
    /// Windows hace su ESP de 100 MiB porque ahí guarda sólo el cargador.
    /// Nosotros guardamos además el kernel y los dos initramfs, así que no
    /// entra. Rechazar la instalación dejaría el caso más común de dual boot
    /// sin salida; formatearlo o agrandarlo le borraría el arranque a Windows.
    ///
    /// La tercera es la que se toma: uno propio en el hueco libre. Windows
    /// conserva el suyo con su cargador adentro y sigue arrancando.
    #[test]
    fn con_el_esp_de_cien_mib_de_windows_se_hace_uno_propio() {
        let disco = disco_con_windows(500, 100);
        let plan =
            planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();

        let esp = plan.particiones.iter().find(|p| p.rol == Rol::Esp).unwrap();
        assert_eq!(esp.accion, Accion::Crear, "se reusó el de 100 MiB");
        assert_eq!(esp.ruta, None);
        assert_eq!(esp.tamano_mib, ARRANQUE_MIB);

        // Y el de Windows no aparece en el plan ni en lo que se pierde: ni
        // montado, ni formateado, ni tocado.
        assert!(
            !plan
                .particiones
                .iter()
                .any(|p| p.ruta.as_deref() == Some("/dev/sda1")),
            "el ESP de Windows entró en el plan: {:?}",
            plan.particiones
        );
        assert!(plan.a_destruir(&disco).is_empty());

        // El nuevo va en el hueco, no encima de nada.
        let inicio = esp.inicio_mib * MIB;
        let fin = inicio + esp.tamano_mib * MIB;
        for e in &disco.particiones {
            assert!(
                fin <= e.inicio_bytes || inicio >= e.fin_bytes(),
                "el ESP nuevo pisa {}",
                e.ruta
            );
        }
    }

    /// **Y si además no hay hueco, ahí sí se falla.**
    ///
    /// Es la única salida que queda: no se puede usar el que hay, no hay lugar
    /// para uno nuevo, y agrandar el de Windows querría decir mover sus
    /// particiones.
    #[test]
    fn con_el_esp_chico_y_sin_hueco_no_hay_salida() {
        // `C:` ocupa la mitad de 42 GiB, así que quedan ~20,9 libres: alcanzan
        // para la raíz sola pero no para la raíz **más** el GiB del ESP nuevo.
        // Con el mismo disco y un ESP reusable, el plan sale.
        let disco = disco_con_windows(42, 100);
        assert!(matches!(
            planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false),
            Err(ErrorPlan::SinEspacioLibre { .. })
        ));
        let mismo_pero_reusable = disco_con_windows(42, 512);
        assert!(
            planificar_junto_a(
                &mismo_pero_reusable,
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .is_ok(),
            "el que falla es el GiB del ESP nuevo, no el disco"
        );
    }

    /// **Sin hueco suficiente no se instala, y no se toca nada.**
    #[test]
    fn sin_espacio_libre_falla_en_vez_de_apretar() {
        // Un disco de 60 GiB con `C:` ocupando la mitad deja ~30 GiB, que
        // alcanza. Con 40 GiB quedan ~20 justos menos el ESP: no alcanza.
        let disco = disco_con_windows(40, 512);
        match planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false) {
            Err(ErrorPlan::SinEspacioLibre { mayor_hueco_gib }) => {
                assert!(mayor_hueco_gib < MINIMO_GIB, "{mayor_hueco_gib}");
            }
            otro => panic!("se esperaba SinEspacioLibre y salió {otro:?}"),
        }
    }

    /// **En BIOS se dice que no, en vez de hacer algo a medias.**
    #[test]
    fn junto_a_otro_sistema_es_solo_uefi() {
        let disco = disco_con_windows(500, 512);
        assert_eq!(
            planificar_junto_a(&disco, Firmware::Bios, SistemaArchivos::Btrfs, false).unwrap_err(),
            ErrorPlan::SoloUefi
        );
    }

    /// **Sin ESP en el disco se crea uno.**
    ///
    /// Un disco con un Linux viejo en MBR, o uno con datos y nada más. Ahí no
    /// hay nada que reusar y el ESP hay que hacerlo.
    #[test]
    fn sin_esp_en_el_disco_se_crea_uno() {
        let mut disco = disco_de(500);
        disco.particiones = vec![ParticionExistente {
            ruta: "/dev/sda1".into(),
            inicio_bytes: MIB,
            tamano_bytes: 100 * 1024 * MIB,
            sistema_archivos: Some("ext4".into()),
            etiqueta: Some("datos".into()),
            numero: Some(1),
            tipo_particion: Some("0fc63daf-8483-4772-8e79-3d69d8477de4".into()),
            sistema_operativo: None,
        }];
        let plan =
            planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let esp = plan.particiones.iter().find(|p| p.rol == Rol::Esp).unwrap();
        assert_eq!(esp.accion, Accion::Crear);
        assert_eq!(esp.ruta, None);
        assert_eq!(esp.tamano_mib, ARRANQUE_MIB);
        assert!(plan.a_destruir(&disco).is_empty());
    }

    /// **Instalar sobre una partición formatea ésa y sólo ésa.**
    ///
    /// La red de seguridad del modo, dicha como corresponde: no «el plan parece
    /// correcto», sino «la lista de lo que se pierde es exactamente la
    /// partición que se eligió». Una de más es un dato ajeno borrado; una de
    /// menos es un cartel que miente.
    #[test]
    fn sobre_una_particion_se_pierde_esa_y_nada_mas() {
        for fs in [
            SistemaArchivos::Btrfs,
            SistemaArchivos::Ext4,
            SistemaArchivos::Xfs,
        ] {
            for cifrar in [false, true] {
                let disco = disco_con_windows(500, 512);
                let plan =
                    planificar_sobre(&disco, "/dev/sda3", Firmware::Uefi, fs, cifrar).unwrap();

                let victimas: Vec<&str> = plan
                    .a_destruir(&disco)
                    .iter()
                    .map(|p| p.ruta.as_str())
                    .collect();
                assert_eq!(victimas, ["/dev/sda3"], "{fs:?}/{cifrar}");
                assert!(!plan.borrar_disco, "{fs:?}/{cifrar}");

                let raiz = plan
                    .particiones
                    .iter()
                    .find(|p| p.rol == Rol::Raiz)
                    .unwrap_or_else(|| panic!("{fs:?}/{cifrar}: el plan quedó sin raíz"));
                assert_eq!(raiz.accion, Accion::Formatear, "{fs:?}/{cifrar}");
                assert_eq!(raiz.ruta.as_deref(), Some("/dev/sda3"), "{fs:?}/{cifrar}");
                assert_eq!(raiz.cifrada, cifrar, "{fs:?}/{cifrar}");

                // Y la geometría, tal cual. `modify` en archinstall borra la
                // partición y la rehace con lo que diga el plan: un número
                // distinto acá la mueve, y moverla es meterse en la de al lado.
                let destino = disco
                    .particiones
                    .iter()
                    .find(|p| p.ruta == "/dev/sda3")
                    .unwrap();
                assert_eq!(raiz.inicio_mib * MIB, destino.inicio_bytes, "{fs:?}/{cifrar}");
                assert_eq!(raiz.tamano_mib * MIB, destino.tamano_bytes, "{fs:?}/{cifrar}");

                // El ESP de Windows se conserva, igual que en el otro modo.
                let esp = plan.particiones.iter().find(|p| p.rol == Rol::Esp).unwrap();
                assert_eq!(esp.accion, Accion::Conservar, "{fs:?}/{cifrar}");
            }
        }
    }

    /// **No se puede instalar sobre el ESP.**
    ///
    /// Formatearlo como raíz deja el equipo sin partición de arranque y de paso
    /// le borra el cargador al otro sistema. La interfaz no debería ofrecerlo,
    /// y por eso mismo el plan tiene que rechazarlo: lo que impide un desastre
    /// no puede depender de que la pantalla esté bien.
    #[test]
    fn no_se_puede_instalar_sobre_el_esp() {
        let disco = disco_con_windows(500, 512);
        assert_eq!(
            planificar_sobre(
                &disco,
                "/dev/sda1",
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::ParticionEsElEsp {
                ruta: "/dev/sda1".into()
            }
        );
    }

    /// **Una partición desalineada se rechaza en vez de moverse.**
    ///
    /// archinstall la borra y la rehace con `optimalAlignedConstraint`, así que
    /// no la puede poner donde estaba: la correría. Y correr una partición es
    /// escribir encima de la de al lado. Además el plan trabaja en MiB enteros,
    /// así que ni siquiera podría representarla.
    #[test]
    fn una_particion_desalineada_no_se_toca() {
        let mut disco = disco_con_windows(500, 512);
        // Como la deja una tabla vieja hecha por otra herramienta: empieza a
        // mitad de un MiB.
        disco.particiones[2].inicio_bytes += 512;
        assert_eq!(
            planificar_sobre(
                &disco,
                "/dev/sda3",
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::ParticionDesalineada {
                ruta: "/dev/sda3".into()
            }
        );
    }

    /// **Una partición que ya no está, una demasiado chica, y BIOS.**
    #[test]
    fn sobre_una_particion_comprueba_lo_que_le_dan() {
        let disco = disco_con_windows(500, 512);
        assert_eq!(
            planificar_sobre(
                &disco,
                "/dev/sda9",
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::ParticionNoEsta {
                ruta: "/dev/sda9".into()
            }
        );

        // `C:` ocupa la mitad del disco: en uno de 500 GiB son 250 y entra; en
        // uno de 30, quince, y no.
        let chico = disco_con_windows(30, 512);
        match planificar_sobre(
            &chico,
            "/dev/sda3",
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            false,
        ) {
            Err(ErrorPlan::Chico { tiene_gib, .. }) => assert!(tiene_gib < MINIMO_GIB),
            otro => panic!("se esperaba Chico y salió {otro:?}"),
        }

        assert_eq!(
            planificar_sobre(
                &disco,
                "/dev/sda3",
                Firmware::Bios,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::SoloUefi
        );
    }

    /// **De varios huecos libres se usa el más grande.**
    ///
    /// Elegir cualquiera da un plan válido —no pisa nada, no se sale del
    /// disco— así que ninguna de las propiedades lo agarra. Pero instalar en
    /// 21 GiB teniendo 400 libres al lado es de las cosas que se descubren
    /// meses después, cuando ya no entra nada y mover la partición es un
    /// problema.
    #[test]
    fn de_varios_huecos_se_usa_el_mas_grande() {
        let mut disco = disco_de(500);
        let mib = MIB;
        // ESP, un hueco de 30 GiB, una partición, y el resto libre: ~440 GiB.
        disco.particiones = vec![
            ParticionExistente {
                ruta: "/dev/sda1".into(),
                inicio_bytes: mib,
                tamano_bytes: 512 * mib,
                sistema_archivos: Some("vfat".into()),
                etiqueta: None,
                numero: Some(1),
                tipo_particion: Some(GUID_ESP.into()),
                sistema_operativo: None,
            },
            ParticionExistente {
                ruta: "/dev/sda2".into(),
                // Deja 30 GiB de hueco entre el ESP y ésta.
                inicio_bytes: (513 + 30 * 1024) * mib,
                tamano_bytes: 20 * 1024 * mib,
                sistema_archivos: Some("ntfs".into()),
                etiqueta: None,
                numero: Some(2),
                tipo_particion: Some("ebd0a0a2-b9e5-4433-87c0-68b6b72699c7".into()),
                sistema_operativo: Some("Windows 11".into()),
            },
        ];

        let plan =
            planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let raiz = plan.particiones.iter().find(|p| p.rol == Rol::Raiz).unwrap();

        // El hueco de atrás son ~449 GiB; el de adelante, 30. Elegir el chico
        // daría una raíz de 30 GiB, que entra en el mínimo y por eso ninguna
        // propiedad se queja.
        assert!(
            raiz.tamano_mib > 400 * 1024,
            "la raíz quedó de {} MiB: se eligió el hueco chico",
            raiz.tamano_mib
        );
        assert!(plan.a_destruir(&disco).is_empty());
    }

    /// **Sobre una partición, en un disco que no tiene ESP.**
    ///
    /// Es la rama de `planificar_sobre` que **crea** el ESP, y no la tocaba
    /// ningún test: todos los discos de prueba ya traían uno. Un disco con un
    /// Linux viejo en MBR es exactamente este caso.
    ///
    /// Lo que importa es que el ESP nuevo caiga en espacio libre: si cayera
    /// encima de una partición ajena, archinstall lo aceptaría —sólo valida
    /// solapamientos entre las que crea— y el estropicio se vería en el disco.
    #[test]
    fn sobre_una_particion_sin_esp_crea_uno_en_el_hueco() {
        let mut disco = disco_de(500);
        let mib = MIB;
        disco.particiones = vec![
            // Un `/boot` viejo de 1 GiB al principio.
            ParticionExistente {
                ruta: "/dev/sda1".into(),
                inicio_bytes: mib,
                tamano_bytes: 1024 * mib,
                sistema_archivos: Some("ext4".into()),
                etiqueta: Some("boot".into()),
                numero: Some(1),
                tipo_particion: Some("0fc63daf-8483-4772-8e79-3d69d8477de4".into()),
                sistema_operativo: None,
            },
            // Y la raíz del Linux viejo, que es la que se va a reusar.
            ParticionExistente {
                ruta: "/dev/sda2".into(),
                inicio_bytes: 1025 * mib,
                tamano_bytes: 100 * 1024 * mib,
                sistema_archivos: Some("ext4".into()),
                etiqueta: Some("raiz vieja".into()),
                numero: Some(2),
                tipo_particion: Some("0fc63daf-8483-4772-8e79-3d69d8477de4".into()),
                sistema_operativo: Some("Debian 12".into()),
            },
        ];

        let plan =
            planificar_sobre(&disco, "/dev/sda2", Firmware::Uefi, SistemaArchivos::Btrfs, false)
                .unwrap();

        let esp = plan.particiones.iter().find(|p| p.rol == Rol::Esp).unwrap();
        assert_eq!(esp.accion, Accion::Crear);
        assert_eq!(esp.ruta, None);
        assert_eq!(esp.tamano_mib, ARRANQUE_MIB);

        // El ESP nuevo no puede caer encima de nada de lo que ya está.
        let inicio = esp.inicio_mib * mib;
        let fin = inicio + esp.tamano_mib * mib;
        for e in &disco.particiones {
            assert!(
                fin <= e.inicio_bytes || inicio >= e.fin_bytes(),
                "el ESP nuevo [{inicio}, {fin}) pisa {} [{}, {})",
                e.ruta,
                e.inicio_bytes,
                e.fin_bytes()
            );
        }
        assert!(fin <= disco.tamano_bytes - mib, "se mete en la copia del GPT");

        // Y se pierde la raíz vieja, que es lo que se eligió, y sólo eso: el
        // `/boot` viejo queda intacto aunque ya no sirva para nada.
        let victimas: Vec<&str> = plan
            .a_destruir(&disco)
            .iter()
            .map(|p| p.ruta.as_str())
            .collect();
        assert_eq!(victimas, ["/dev/sda2"]);
    }

    /// **Los dos modos no destructivos tratan el ESP igual.**
    ///
    /// Es lo que hace que reusar el ESP ajeno sea una decisión y no dos. Se
    /// comprueba comparando los planes, no leyendo el código: si alguien
    /// separa las dos ramas, acá se ve.
    #[test]
    fn los_modos_no_destructivos_tratan_el_esp_igual() {
        let disco = disco_con_windows(500, 512);
        let a = planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let b = planificar_sobre(
            &disco,
            "/dev/sda3",
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            false,
        )
        .unwrap();

        let esp_de = |p: &Plan| {
            p.particiones
                .iter()
                .find(|x| x.rol == Rol::Esp)
                .cloned()
                .unwrap()
        };
        assert_eq!(esp_de(&a), esp_de(&b));

        // Y con uno de 100 MiB los dos hacen lo mismo: uno propio, sin tocar
        // el ajeno.
        let flaco = disco_con_windows(500, 100);
        let a = planificar_junto_a(&flaco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let b = planificar_sobre(
            &flaco,
            "/dev/sda3",
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            false,
        )
        .unwrap();
        assert_eq!(esp_de(&a).accion, Accion::Crear);
        assert_eq!(esp_de(&a), esp_de(&b));
    }

    /// **Los huecos se calculan alineados y sin morder lo ajeno.**
    ///
    /// El redondeo tiene que ir para el lado seguro en los dos extremos: el
    /// final de un hueco hacia abajo y el principio hacia arriba. Al revés, el
    /// borde caería adentro de la partición de al lado.
    #[test]
    fn los_huecos_no_muerden_a_los_vecinos() {
        let mut disco = disco_de(100);
        // Una partición desalineada a propósito: empieza y termina a mitad de
        // un MiB, que es lo que deja una tabla vieja hecha por otra
        // herramienta.
        disco.particiones = vec![ParticionExistente {
            ruta: "/dev/sda1".into(),
            inicio_bytes: 10 * MIB + 512,
            tamano_bytes: 20 * MIB + 123,
            sistema_archivos: Some("ntfs".into()),
            etiqueta: None,
            numero: Some(1),
            tipo_particion: None,
            sistema_operativo: None,
        }];
        let existente = &disco.particiones[0];
        for h in huecos_libres(&disco) {
            let inicio = h.inicio_mib * MIB;
            let fin = inicio + h.tamano_mib * MIB;
            assert_eq!(inicio % MIB, 0, "hueco desalineado: {h:?}");
            assert_eq!(fin % MIB, 0, "hueco desalineado: {h:?}");
            assert!(
                fin <= existente.inicio_bytes || inicio >= existente.fin_bytes(),
                "el hueco {h:?} pisa la partición [{}, {})",
                existente.inicio_bytes,
                existente.fin_bytes()
            );
            assert!(fin <= disco.tamano_bytes - MIB, "se come la copia del GPT");
        }
    }

    #[test]
    fn uefi_arma_esp_y_raiz_sin_huecos() {
        let disco = disco_de(100);
        let plan = planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].rol, Rol::Esp);
        assert_eq!(plan[0].inicio_mib, 1);
        assert_eq!(plan[0].tamano_mib, 1024);
        assert_eq!(plan[1].rol, Rol::Raiz);

        // Sin hueco entre las dos: la raíz empieza exactamente donde termina el
        // ESP. Un hueco de un MiB no rompe nada, pero es la clase de descuido
        // que después aparece como «el disco tiene 3 particiones».
        assert_eq!(plan[1].inicio_mib, plan[0].inicio_mib + plan[0].tamano_mib);
    }

    #[test]
    fn el_plan_no_se_pasa_del_disco() {
        // Varios tamaños, incluidos los que no son múltiplos redondos de MiB:
        // ahí es donde una división entera de más deja la última partición un
        // MiB fuera del disco.
        for gib in [20u64, 21, 64, 100, 250, 500, 931, 1863] {
            let mut disco = disco_de(gib);
            // Unos bytes de más para que el tamaño no caiga justo en un MiB.
            disco.tamano_bytes += 12345;
            let total_mib = disco.tamano_bytes / MIB;

            for firmware in [Firmware::Uefi, Firmware::Bios] {
                let plan = planificar(&disco, firmware, SistemaArchivos::Ext4, false).unwrap();
                let ultima = plan.last().unwrap();
                let fin = ultima.inicio_mib + ultima.tamano_mib;
                assert!(
                    fin <= total_mib - RESERVA_FINAL_MIB,
                    "{gib} GiB / {firmware:?}: el plan termina en {fin} MiB y el disco tiene {total_mib} MiB"
                );
            }
        }
    }

    /// **Los dos firmwares tienen que dar una partición de arranque montada.**
    ///
    /// Éste es el test del error que apareció instalando en BIOS:
    ///
    /// ```text
    /// ValueError: Could not detect boot at mountpoint /mnt
    /// ```
    ///
    /// `add_bootloader` de archinstall arranca con `_get_boot_partition()`, y
    /// `get_boot_partition` filtra por `x.is_boot() and x.mountpoint` — bandera
    /// `boot` **y** punto de montaje. Sin una partición así la instalación muere
    /// al llegar al gestor de arranque, con el disco ya formateado y los paquetes
    /// ya instalados. El peor momento para fallar.
    ///
    /// Se comprueba para los dos firmwares y para los dos sistemas de archivos:
    /// con btrfs la raíz no lleva punto de montaje —lo lleva el subvolumen `@`—,
    /// así que poner la bandera en la raíz no habría alcanzado justo en el camino
    /// por defecto.
    #[test]
    fn siempre_hay_una_particion_de_arranque_que_archinstall_reconoce() {
        for firmware in [Firmware::Uefi, Firmware::Bios] {
            for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
                let plan = planificar(&disco_de(50), firmware, fs, false).unwrap();

                let arranque = plan
                    .iter()
                    .find(|p| p.banderas.contains(&"boot") && p.punto_montaje.is_some())
                    .unwrap_or_else(|| {
                        panic!("{firmware:?}/{fs:?}: ninguna partición con bandera boot y punto de montaje: {plan:?}")
                    });

                assert_eq!(arranque.punto_montaje, Some("/boot"), "{firmware:?}/{fs:?}");
                assert_eq!(arranque.rol, Rol::Esp, "{firmware:?}/{fs:?}");
                // Con sistema de archivos: sin él, archinstall muere antes en
                // `safe_fs_type` con «File system type is not set».
                assert!(
                    arranque.sistema_archivos.is_some(),
                    "{firmware:?}/{fs:?}: la partición de arranque sin sistema de archivos"
                );
            }
        }
    }

    #[test]
    fn en_bios_el_arranque_es_ext4_y_sin_esp() {
        // `esp` marcaría una partición de sistema EFI en un disco que arranca por
        // BIOS, y fat32 no hace falta porque acá no hay firmware que lo lea.
        let plan = planificar(&disco_de(50), Firmware::Bios, SistemaArchivos::Btrfs, false).unwrap();
        let arranque = &plan[0];

        assert_eq!(arranque.sistema_archivos, Some("ext4"));
        assert_eq!(arranque.banderas, vec!["boot"]);
        assert!(!arranque.banderas.contains(&"esp"), "{arranque:?}");
    }

    #[test]
    fn en_uefi_el_arranque_sigue_siendo_el_esp() {
        // Que el arreglo de BIOS no haya cambiado el camino que ya funcionaba.
        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let esp = &plan[0];

        assert_eq!(esp.sistema_archivos, Some("fat32"));
        assert_eq!(esp.banderas, vec!["boot", "esp"]);
        assert!(esp.opciones_montaje.iter().any(|o| o.contains("umask")));
    }

    #[test]
    fn en_bios_queda_el_hueco_que_grub_necesita() {
        // El MiB 0 es lo que GRUB usa para su segunda etapa en MBR. Si la primera
        // partición empezara en 0 no habría dónde escribirla y el disco no
        // arrancaría.
        let plan = planificar(&disco_de(50), Firmware::Bios, SistemaArchivos::Ext4, false).unwrap();
        assert_eq!(plan[0].inicio_mib, 1);
    }

    /// Ninguna partición puede salir sin sistema de archivos.
    ///
    /// `_setup_partition` de archinstall pide `safe_fs_type` para **todas** las
    /// que crea, y esa propiedad lanza si el valor es `None`. Una partición sin
    /// filesystem mata la instalación en el paso de particionado.
    #[test]
    fn ninguna_particion_sale_sin_sistema_de_archivos() {
        for firmware in [Firmware::Uefi, Firmware::Bios] {
            for fs in [SistemaArchivos::Ext4, SistemaArchivos::Btrfs] {
                for cifrar in [false, true] {
                    let plan = planificar(&disco_de(50), firmware, fs, cifrar).unwrap();
                    for p in &plan {
                        assert!(
                            p.sistema_archivos.is_some(),
                            "{:?} sale sin filesystem con {firmware:?}/{fs:?}",
                            p.rol
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn btrfs_deja_el_punto_de_montaje_en_el_subvolumen() {
        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        let raiz = plan.last().unwrap();

        // Éste es el bug que borra la instalación: con `mountpoint` y
        // subvolúmenes a la vez, archinstall monta la partición cruda en `/` y
        // el sistema queda instalado afuera de `@`, así que el primer arranque
        // encuentra un `@` vacío.
        assert!(
            raiz.punto_montaje.is_none(),
            "la raíz btrfs no puede llevar punto de montaje propio"
        );
        assert_eq!(raiz.subvolumenes.len(), 7);
        assert!(raiz.subvolumenes.contains(&("@", "/")));
        assert!(raiz.subvolumenes.contains(&("@log", "/var/log")));
    }

    #[test]
    fn sin_btrfs_la_raiz_se_monta_directo() {
        for fs in [SistemaArchivos::Ext4, SistemaArchivos::Xfs] {
            let plan = planificar(&disco_de(50), Firmware::Uefi, fs, false).unwrap();
            let raiz = plan.last().unwrap();
            assert_eq!(raiz.punto_montaje, Some("/"), "{fs:?}");
            assert!(raiz.subvolumenes.is_empty(), "{fs:?}");
        }
    }

    #[test]
    fn el_esp_nunca_va_cifrado() {
        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, true).unwrap();
        let esp = &plan[0];
        // El firmware lee el ESP antes de que exista nada que pueda descifrarlo.
        // Cifrarlo produce un equipo que no arranca.
        assert!(!esp.cifrada);
        assert!(plan.last().unwrap().cifrada, "la raíz sí tiene que ir cifrada");
    }

    #[test]
    fn nvme_comprime_mas_flojo_que_un_disco_comun() {
        let mut nvme = disco_de(50);
        nvme.nvme = true;
        let plan = planificar(&nvme, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        assert!(plan.last().unwrap().opciones_montaje.contains(&"compress=zstd:1".to_string()));

        let plan = planificar(&disco_de(50), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap();
        assert!(plan.last().unwrap().opciones_montaje.contains(&"compress=zstd".to_string()));
    }

    #[test]
    fn un_disco_chico_se_rechaza_antes_de_tocarlo() {
        let err = planificar(&disco_de(8), Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err();
        assert_eq!(
            err,
            ErrorPlan::Chico {
                tiene_gib: 8,
                minimo_gib: MINIMO_GIB
            }
        );
    }

    #[test]
    fn un_disco_montado_se_rechaza() {
        let mut disco = disco_de(500);
        disco.en_uso = true;
        // Es el pendrive del que arrancó la ISO. Borrarlo mata la instalación
        // en marcha.
        assert_eq!(
            planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err(),
            ErrorPlan::EnUso
        );
    }

    #[test]
    fn un_disco_de_cero_bytes_no_paniquea() {
        // `lsblk` puede informar tamaño cero para un lector de tarjetas vacío.
        // Antes de la comprobación explícita esto llegaba a la división y
        // salía un plan con una partición de tamaño absurdo.
        let mut disco = disco_de(0);
        disco.tamano_bytes = 0;
        assert_eq!(
            planificar(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err(),
            ErrorPlan::Invalido
        );
    }

    #[test]
    fn los_subvolumenes_no_se_repiten_ni_en_nombre_ni_en_punto_de_montaje() {
        let mut nombres: Vec<&str> = SUBVOLUMENES.iter().map(|(n, _)| *n).collect();
        let cantidad = nombres.len();
        nombres.sort_unstable();
        nombres.dedup();
        assert_eq!(nombres.len(), cantidad, "hay un subvolumen repetido");

        let mut puntos: Vec<&str> = SUBVOLUMENES.iter().map(|(_, p)| *p).collect();
        puntos.sort_unstable();
        puntos.dedup();
        assert_eq!(puntos.len(), cantidad, "hay dos subvolúmenes en el mismo punto");

        // Todos absolutos: archinstall los usa tal cual para armar la ruta
        // bajo el destino, y uno relativo terminaría montado en el directorio
        // de trabajo del instalador.
        assert!(SUBVOLUMENES.iter().all(|(_, p)| p.starts_with('/')));
    }
    fn asignar(particion: &str, punto: Option<&str>, formatear: bool) -> AsignacionManual {
        AsignacionManual {
            particion: particion.into(),
            punto_montaje: punto.map(str::to_string),
            formatear,
        }
    }

    /// Un disco repartido: ESP, Windows, y dos particiones de un Linux viejo.
    fn disco_repartido() -> Disco {
        let mut d = disco_de(500);
        let mut inicio = MIB;
        let mut nuevas = Vec::new();
        for (n, tam_gib, fs, guid, os) in [
            (1u32, 1u64, Some("vfat"), GUID_ESP, None),
            (
                2,
                100,
                Some("ntfs"),
                "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7",
                Some("Windows 11"),
            ),
            (
                3,
                60,
                Some("ext4"),
                "0fc63daf-8483-4772-8e79-3d69d8477de4",
                Some("Debian 12"),
            ),
            (
                4,
                200,
                Some("ext4"),
                "0fc63daf-8483-4772-8e79-3d69d8477de4",
                None,
            ),
        ] {
            let tam = tam_gib * 1024 * MIB;
            nuevas.push(ParticionExistente {
                ruta: format!("/dev/sda{n}"),
                inicio_bytes: inicio,
                tamano_bytes: tam,
                sistema_archivos: fs.map(str::to_string),
                etiqueta: None,
                numero: Some(n),
                tipo_particion: Some(guid.into()),
                sistema_operativo: os.map(str::to_string),
            });
            inicio += tam;
        }
        d.particiones = nuevas;
        d
    }

    /// **Lo que no se asigna, no se toca.**
    ///
    /// Es la promesa entera del modo manual, y la que más fácil se rompe: en
    /// los otros modos el plan se arma solo, acá lo arma alguien. Una partición
    /// que se cuela en el plan sin haber sido elegida es un dato ajeno perdido.
    #[test]
    fn en_manual_lo_que_no_se_asigna_no_aparece() {
        let disco = disco_repartido();
        // Se usa el ESP y la última; Windows y el Linux viejo se dejan.
        let asignaciones = vec![
            asignar("/dev/sda1", Some("/boot"), false),
            asignar("/dev/sda4", Some("/"), true),
            // Nombrada pero sin punto: es «dejala como está».
            asignar("/dev/sda3", None, false),
        ];
        let plan = planificar_manual(
            &disco,
            &asignaciones,
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            false,
        )
        .unwrap();

        let rutas: Vec<&str> = plan
            .particiones
            .iter()
            .filter_map(|p| p.ruta.as_deref())
            .collect();
        assert_eq!(rutas, ["/dev/sda1", "/dev/sda4"]);

        let victimas: Vec<&str> = plan
            .a_destruir(&disco)
            .iter()
            .map(|p| p.ruta.as_str())
            .collect();
        assert_eq!(victimas, ["/dev/sda4"], "sólo se formatea la raíz");
        assert!(!plan.borrar_disco);
    }

    /// **Un `/home` que ya existe se conserva y se monta.**
    ///
    /// Es la razón principal por la que alguien usa el modo manual: reinstalar
    /// el sistema y quedarse con los archivos. `Conservar` es `existing` en
    /// archinstall, que no lo formatea pero sí lo monta.
    #[test]
    fn en_manual_un_home_existente_se_conserva() {
        let disco = disco_repartido();
        let asignaciones = vec![
            asignar("/dev/sda1", Some("/boot"), false),
            asignar("/dev/sda3", Some("/"), true),
            asignar("/dev/sda4", Some("/home"), false),
        ];
        let plan = planificar_manual(
            &disco,
            &asignaciones,
            Firmware::Uefi,
            SistemaArchivos::Ext4,
            false,
        )
        .unwrap();

        let home = plan
            .particiones
            .iter()
            .find(|p| p.punto_montaje == Some("/home"))
            .unwrap();
        assert_eq!(home.accion, Accion::Conservar);
        assert_eq!(home.ruta.as_deref(), Some("/dev/sda4"));
        // El sistema de archivos que ya tiene, con el nombre de archinstall.
        assert_eq!(home.sistema_archivos, Some("ext4"));
        assert!(!home.cifrada);

        let victimas: Vec<&str> = plan
            .a_destruir(&disco)
            .iter()
            .map(|p| p.ruta.as_str())
            .collect();
        assert_eq!(victimas, ["/dev/sda3"], "el /home no se pierde");
    }

    /// **`vfat` no es `fat32`, y un nombre que archinstall no conozca no puede
    /// viajar.**
    ///
    /// `lsblk` dice `vfat`, `FilesystemType` de archinstall dice `fat32`. Un
    /// nombre que no esté en su enumeración hace que rechace el JSON entero con
    /// un `ValueError`, o sea que la instalación no arranca por el nombre de un
    /// sistema de archivos que ni siquiera se iba a tocar.
    #[test]
    fn el_nombre_del_sistema_de_archivos_se_traduce() {
        assert_eq!(fs_para_archinstall("vfat"), Some("fat32"));
        assert_eq!(fs_para_archinstall("swap"), Some("linux-swap"));
        assert_eq!(fs_para_archinstall("ext4"), Some("ext4"));
        assert_eq!(fs_para_archinstall("ntfs"), Some("ntfs"));
        // Los que no conoce van como `null`, que en una partición que no se
        // formatea archinstall acepta sin chistar.
        assert_eq!(fs_para_archinstall("zfs"), None);
        assert_eq!(fs_para_archinstall("apfs"), None);
        assert_eq!(fs_para_archinstall(""), None);

        // Y el ESP conservado sale como `fat32` y no como `vfat`.
        let disco = disco_repartido();
        let plan = planificar_manual(
            &disco,
            &[
                asignar("/dev/sda1", Some("/boot"), false),
                asignar("/dev/sda4", Some("/"), true),
            ],
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            false,
        )
        .unwrap();
        let esp = plan.particiones.iter().find(|p| p.rol == Rol::Esp).unwrap();
        assert_eq!(esp.sistema_archivos, Some("fat32"));
    }

    /// **Todo lo que el modo manual rechaza, y por qué.**
    #[test]
    fn en_manual_las_barandas_estan() {
        let disco = disco_repartido();
        let uefi = |a: Vec<AsignacionManual>| {
            planificar_manual(&disco, &a, Firmware::Uefi, SistemaArchivos::Btrfs, false)
        };
        let boot = || asignar("/dev/sda1", Some("/boot"), false);
        let raiz = || asignar("/dev/sda4", Some("/"), true);

        // Sin raíz no hay dónde instalar.
        assert_eq!(uefi(vec![boot()]).unwrap_err(), ErrorPlan::SinRaiz);

        // Sin arranque, archinstall muere al llegar al cargador con el disco ya
        // formateado.
        assert_eq!(uefi(vec![raiz()]).unwrap_err(), ErrorPlan::SinArranque);

        // La raíz sin formatear deja dos sistemas mezclados en el mismo árbol.
        assert_eq!(
            uefi(vec![boot(), asignar("/dev/sda4", Some("/"), false)]).unwrap_err(),
            ErrorPlan::LaRaizSeFormatea
        );

        // `/boot` en una partición que no es el ESP: no arrancaría, y se
        // descubriría al reiniciar.
        assert_eq!(
            uefi(vec![
                asignar("/dev/sda3", Some("/boot"), true),
                raiz()
            ])
            .unwrap_err(),
            ErrorPlan::ArranqueNoEsEsp {
                ruta: "/dev/sda3".into()
            }
        );

        // El mismo punto dos veces: archinstall montaría una encima de la otra.
        assert_eq!(
            uefi(vec![boot(), raiz(), asignar("/dev/sda3", Some("/"), true)]).unwrap_err(),
            ErrorPlan::AsignacionRepetida { que: "/".into() }
        );

        // Y la misma partición dos veces.
        assert_eq!(
            uefi(vec![
                boot(),
                raiz(),
                asignar("/dev/sda4", Some("/home"), false)
            ])
            .unwrap_err(),
            ErrorPlan::AsignacionRepetida {
                que: "/dev/sda4".into()
            }
        );

        // Un punto de montaje inventado.
        assert_eq!(
            uefi(vec![boot(), raiz(), asignar("/dev/sda3", Some("/hone"), false)]).unwrap_err(),
            ErrorPlan::PuntoDeMontajeDesconocido {
                punto: "/hone".into()
            }
        );

        // Una partición que ya no está.
        assert_eq!(
            uefi(vec![boot(), asignar("/dev/sda9", Some("/"), true)]).unwrap_err(),
            ErrorPlan::ParticionNoEsta {
                ruta: "/dev/sda9".into()
            }
        );

        // Una raíz que no llega al mínimo: sda1 es el ESP de 1 GiB, así que se
        // usa como raíz una que sí exista y sea chica.
        let mut chico = disco_repartido();
        chico.particiones[3].tamano_bytes = 10 * 1024 * MIB;
        assert!(matches!(
            planificar_manual(
                &chico,
                &[boot(), raiz()],
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            ),
            Err(ErrorPlan::Chico { .. })
        ));

        // Una partición desalineada: para formatearla hay que rehacerla, y
        // rehacerla en otro lado es escribir encima de la de al lado.
        let mut torcido = disco_repartido();
        torcido.particiones[3].inicio_bytes += 512;
        assert_eq!(
            planificar_manual(
                &torcido,
                &[boot(), raiz()],
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::ParticionDesalineada {
                ruta: "/dev/sda4".into()
            }
        );

        // Un ESP de 100 MiB tiene el GUID correcto y no alcanza igual: acá
        // adentro van el kernel y los dos initramfs.
        let mut flaco = disco_repartido();
        flaco.particiones[0].tamano_bytes = 100 * MIB;
        assert!(matches!(
            planificar_manual(
                &flaco,
                &[boot(), raiz()],
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            ),
            Err(ErrorPlan::EspChico { .. })
        ));

        // Y una partición cifrada que se quiere conservar y montar: abrir
        // volúmenes LUKS que no son la raíz todavía no se hace, así que
        // montarla fallaría a mitad de la instalación.
        let mut cifrada = disco_repartido();
        cifrada.particiones[2].sistema_archivos = Some("crypto_LUKS".into());
        assert_eq!(
            planificar_manual(
                &cifrada,
                &[boot(), raiz(), asignar("/dev/sda3", Some("/home"), false)],
                Firmware::Uefi,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::ParticionCifrada {
                ruta: "/dev/sda3".into()
            }
        );

        // Y en BIOS, nada de esto.
        assert_eq!(
            planificar_manual(
                &disco,
                &[boot(), raiz()],
                Firmware::Bios,
                SistemaArchivos::Btrfs,
                false
            )
            .unwrap_err(),
            ErrorPlan::SoloUefi
        );
    }

    /// **En manual, sólo la raíz va cifrada.**
    ///
    /// El ESP no puede: el firmware tiene que poder leerlo para arrancar. Y el
    /// resto tampoco, porque no hay dónde pedirle la frase a alguien en el
    /// arranque más que para la raíz.
    #[test]
    fn en_manual_solo_la_raiz_va_cifrada() {
        let disco = disco_repartido();
        let plan = planificar_manual(
            &disco,
            &[
                asignar("/dev/sda1", Some("/boot"), false),
                asignar("/dev/sda3", Some("/"), true),
                asignar("/dev/sda4", Some("/home"), true),
            ],
            Firmware::Uefi,
            SistemaArchivos::Btrfs,
            true,
        )
        .unwrap();

        for p in &plan.particiones {
            assert_eq!(
                p.cifrada,
                p.rol == Rol::Raiz,
                "{:?} ({:?}) tiene cifrada={}",
                p.ruta,
                p.rol,
                p.cifrada
            );
        }
        // Y el /home formateado se pierde, que es lo que se pidió.
        let victimas: Vec<&str> = plan
            .a_destruir(&disco)
            .iter()
            .map(|p| p.ruta.as_str())
            .collect();
        assert_eq!(victimas, ["/dev/sda3", "/dev/sda4"]);
    }

    // ── Propiedades ─────────────────────────────────────────────────────────
    //
    // Los tests de arriba dicen cada uno un caso. Éstos dicen lo que tiene que
    // valer **siempre**, en cualquier disco y con cualquier modo, y es lo que
    // corresponde en el único código del instalador cuyo error borra datos.
    //
    // Escritos antes del particionado manual a propósito: cuando alguien pueda
    // armar el plan a mano, esto es lo que va a impedir que se lleve puesto un
    // disco ajeno.

    /// Un disco con particiones ya usadas, alineadas y sin solaparse.
    ///
    /// Generado y no al azar: una lista de `(inicio, tamaño)` cualquiera casi
    /// nunca da particiones válidas, y con particiones inválidas las
    /// propiedades pasan sin haber probado nada. Acá se generan huecos y
    /// tamaños en MiB y se van encadenando, que es como las hace un
    /// particionador de verdad.
    fn un_disco_usado() -> impl Strategy<Value = Disco> {
        // Hasta seis particiones, cada una con el hueco que la precede.
        (
            20u64..2048,                                    // tamaño del disco en GiB
            proptest::collection::vec((0u64..4096, 1u64..60_000, 0usize..3), 0..6),
            any::<bool>(),                                  // ¿hay ESP?
            512u64..=4096,
            // Cuántos bytes se corre la tabla entera. Casi siempre cero, que es
            // lo que hace cualquier particionador desde hace quince años; pero
            // una tabla vieja hecha con otra herramienta las deja a mitad de un
            // MiB, y ése es justo el caso que el plan tiene que rechazar. Sin
            // esto, la comprobación de alineación no la ejercita nadie.
            proptest::prop_oneof![9 => Just(0u64), 1 => 1u64..1_048_576],
        )
            .prop_map(|(gib, tramos, con_esp, sector, desfase)| {
                let sector = if sector <= 512 { 512 } else { 4096 };
                let total_mib = gib * 1024;
                let mut particiones = Vec::new();
                let mut cursor = INICIO_MIB;

                if con_esp {
                    particiones.push(ParticionExistente {
                        ruta: "/dev/sda1".into(),
                        inicio_bytes: cursor * MIB + desfase,
                        tamano_bytes: 512 * MIB,
                        sistema_archivos: Some("vfat".into()),
                        etiqueta: Some("SYSTEM".into()),
                        numero: Some(1),
                        tipo_particion: Some(GUID_ESP.into()),
                        sistema_operativo: None,
                    });
                    cursor += 512;
                }

                for (hueco, tamano, tipo) in tramos {
                    cursor += hueco;
                    // Se para al llegar al final: el último MiB es la copia de
                    // la tabla GPT y ahí no va nada.
                    if cursor + tamano + RESERVA_FINAL_MIB + 1 > total_mib {
                        break;
                    }
                    let n = particiones.len() + 1;
                    particiones.push(ParticionExistente {
                        ruta: format!("/dev/sda{n}"),
                        inicio_bytes: cursor * MIB + desfase,
                        tamano_bytes: tamano * MIB,
                        sistema_archivos: match tipo {
                            0 => Some("ntfs".into()),
                            1 => Some("ext4".into()),
                            _ => None,
                        },
                        etiqueta: None,
                        numero: Some(n as u32),
                        tipo_particion: Some("0fc63daf-8483-4772-8e79-3d69d8477de4".into()),
                        sistema_operativo: if tipo == 0 {
                            Some("Windows 11".into())
                        } else {
                            None
                        },
                        });
                    cursor += tamano;
                }

                Disco {
                    ruta: "/dev/sda".into(),
                    modelo: "Disco generado".into(),
                    tamano_bytes: total_mib * MIB,
                    sector_logico: sector,
                    rotacional: false,
                    nvme: true,
                    en_uso: false,
                    particiones,
                }
            })
    }

    /// Todos los planes que un disco admite, con el esquema que los produjo.
    fn planes_de(disco: &Disco, fs: SistemaArchivos, cifrar: bool) -> Vec<(&'static str, Plan)> {
        let mut salida = Vec::new();
        for (nombre, esquema, particion) in [
            ("borrar", EsquemaDisco::BorrarTodo, None),
            ("al lado", EsquemaDisco::JuntoAOtroSistema, None),
        ] {
            if let Ok(p) = planificar_con(
                disco,
                esquema,
                particion,
                &[],
                Firmware::Uefi,
                fs,
                cifrar,
            ) {
                salida.push((nombre, p));
            }
        }
        // Y uno por cada partición que se podría elegir como destino.
        for e in &disco.particiones {
            if let Ok(p) = planificar_sobre(disco, &e.ruta, Firmware::Uefi, fs, cifrar) {
                salida.push(("sobre", p));
            }
        }

        // El manual, que es el que más falta hace acá: en los otros el plan lo
        // arma el instalador, y acá lo arma alguien. Se prueban todas las
        // combinaciones de «ESP en /boot, una partición en /, otra en /home»,
        // con y sin formatear la de /home.
        let esp = disco.particiones.iter().find(|p| p.es_esp());
        if let Some(esp) = esp {
            for raiz in disco.particiones.iter().filter(|p| !p.es_esp()) {
                for otra in disco.particiones.iter().filter(|p| !p.es_esp()) {
                    for formatear_otra in [false, true] {
                        let mut a = vec![
                            AsignacionManual {
                                particion: esp.ruta.clone(),
                                punto_montaje: Some("/boot".into()),
                                formatear: false,
                            },
                            AsignacionManual {
                                particion: raiz.ruta.clone(),
                                punto_montaje: Some("/".into()),
                                formatear: true,
                            },
                        ];
                        if otra.ruta != raiz.ruta {
                            a.push(AsignacionManual {
                                particion: otra.ruta.clone(),
                                punto_montaje: Some("/home".into()),
                                formatear: formatear_otra,
                            });
                        }
                        if let Ok(p) =
                            planificar_manual(disco, &a, Firmware::Uefi, fs, cifrar)
                        {
                            salida.push(("manual", p));
                        }
                    }
                }
            }
        }
        salida
    }

    /// **Que el generador produzca discos con los que se pueda hacer algo.**
    ///
    /// Sin esto las cinco propiedades de abajo pueden estar en verde sin haber
    /// probado nada: si `planes_de` devolviera la lista vacía siempre —porque
    /// el generador arma discos que ningún modo acepta— cada `for` no daría ni
    /// una vuelta y todo pasaría.
    ///
    /// Se corre una sola vez con muchas muestras y se mide, en vez de afirmarlo
    /// adentro de cada propiedad, que ahí sería ruido en cada caso.
    #[test]
    fn el_generador_de_discos_llega_a_los_tres_modos() {
        use proptest::strategy::{Strategy as _, ValueTree};
        use proptest::test_runner::TestRunner;

        let mut runner = TestRunner::deterministic();
        let estrategia = un_disco_usado();
        let (mut con_esp_ajeno, mut con_sobre, mut con_al_lado, mut total) = (0, 0, 0, 0);
        let (mut con_manual, mut desalineados) = (0, 0);

        for _ in 0..300 {
            let disco = estrategia.new_tree(&mut runner).unwrap().current();
            if disco.particiones.iter().any(|p| p.es_esp()) {
                con_esp_ajeno += 1;
            }
            if disco
                .particiones
                .iter()
                .any(|p| p.inicio_bytes % MIB != 0 || p.tamano_bytes % MIB != 0)
            {
                desalineados += 1;
            }
            for (nombre, _) in planes_de(&disco, SistemaArchivos::Btrfs, false) {
                total += 1;
                match nombre {
                    "sobre" => con_sobre += 1,
                    "al lado" => con_al_lado += 1,
                    "manual" => con_manual += 1,
                    _ => {}
                }
            }
        }

        assert!(total > 300, "sólo {total} planes en 300 discos: casi todos vacíos");
        assert!(con_sobre > 20, "sólo {con_sobre} planes «sobre una partición»");
        assert!(con_al_lado > 20, "sólo {con_al_lado} planes «al lado»");
        assert!(con_manual > 20, "sólo {con_manual} planes manuales");
        // Sin discos desalineados, las tres comprobaciones de alineación —una
        // por modo— no las ejercita nadie, y `lo_que_se_reusa_conserva_su_geometria`
        // pasaría sin haber visto el caso que la motiva.
        assert!(
            desalineados > 10,
            "sólo {desalineados} discos desalineados en 300"
        );
        assert!(
            con_esp_ajeno > 50,
            "sólo {con_esp_ajeno} discos con ESP ajeno: la propiedad del ESP no probaría nada"
        );
    }

    proptest::proptest! {
        /// **Nada que el plan no declare destruido se toca.**
        ///
        /// La invariante que sostiene toda la pantalla de confirmación: lo que
        /// se le muestra a la persona es `a_destruir`, y si el plan pisara algo
        /// que no está en esa lista, se perdería sin que nadie lo hubiera
        /// aceptado.
        ///
        /// Se comprueba por geometría, que es lo que archinstall va a ejecutar,
        /// y no por la ruta: una partición se pierde igual si la pisan sin
        /// nombrarla.
        ///
        /// Cada acción tiene su regla, porque «pisar» no quiere decir lo mismo
        /// en las tres:
        ///
        ///   - `Conservar` **es** una partición que ya está, no la pisa. Se
        ///     saltea, pero se comprueba que la que dice conservar exista y sea
        ///     exactamente ésa.
        ///   - `Formatear` destruye la que nombra, así que tiene que estar
        ///     declarada — y no puede pisar ninguna otra.
        ///   - `Crear` no puede pisar nada, porque va en espacio libre.
        #[test]
        fn ningun_plan_pisa_lo_que_no_declara(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                if plan.borrar_disco {
                    continue; // Ahí se declara todo, y no hay nada que respetar.
                }
                let declaradas: Vec<&str> = plan
                    .a_destruir(&disco)
                    .iter()
                    .map(|p| p.ruta.as_str())
                    .collect();

                for p in &plan.particiones {
                    let inicio = p.inicio_mib * MIB;
                    let fin = inicio + p.tamano_mib * MIB;

                    if p.accion == Accion::Formatear {
                        let ruta = p.ruta.as_deref().unwrap_or("");
                        proptest::prop_assert!(
                            declaradas.contains(&ruta),
                            "{nombre}: formatea {ruta} y no lo declara. declaradas={declaradas:?}"
                        );
                    }

                    for e in &disco.particiones {
                        if !(inicio < e.fin_bytes() && e.inicio_bytes < fin) {
                            continue;
                        }
                        match p.accion {
                            // La que se conserva tiene que ser exactamente ésa
                            // y no una que se le solape: solaparse con otra
                            // querría decir que la geometría se copió mal.
                            Accion::Conservar => proptest::prop_assert_eq!(
                                p.ruta.as_deref(), Some(e.ruta.as_str()),
                                "{}: dice conservar {:?} y se solapa con {}",
                                nombre, p.ruta, e.ruta
                            ),
                            Accion::Formatear => proptest::prop_assert_eq!(
                                p.ruta.as_deref(), Some(e.ruta.as_str()),
                                "{}: formatea {:?} y además pisa {}",
                                nombre, p.ruta, e.ruta
                            ),
                            Accion::Crear => proptest::prop_assert!(
                                false,
                                "{nombre}: crea una partición encima de {}. plan={:?}",
                                e.ruta, plan.particiones
                            ),
                        }
                    }
                }
            }
        }

        /// **Una partición que se reusa conserva su geometría exacta.**
        ///
        /// `Conservar` y `Formatear` no crean nada: apuntan a una partición que
        /// ya está. Si el plan le cambiara los números —aunque fuera por el
        /// redondeo a MiB de una partición desalineada— archinstall la rehace
        /// en otro lado, y «otro lado» es encima de la de al lado.
        ///
        /// Por eso el generador desalinea a veces: si todas las particiones
        /// entraran justas en MiB, esta propiedad y la comprobación de
        /// alineación que la sostiene no las ejercitaría nadie.
        #[test]
        fn lo_que_se_reusa_conserva_su_geometria(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                for p in &plan.particiones {
                    if p.accion == Accion::Crear {
                        continue;
                    }
                    let ruta = p.ruta.as_deref().unwrap_or("");
                    let e = disco
                        .particiones
                        .iter()
                        .find(|e| e.ruta == ruta)
                        .expect("una partición reusada tiene que existir");
                    proptest::prop_assert_eq!(
                        p.inicio_mib * MIB, e.inicio_bytes,
                        "{}: {} se movería de {} a {}",
                        nombre, ruta, e.inicio_bytes, p.inicio_mib * MIB
                    );
                    proptest::prop_assert_eq!(
                        p.tamano_mib * MIB, e.tamano_bytes,
                        "{}: {} cambiaría de tamaño",
                        nombre, ruta
                    );
                }
            }
        }

        /// **Ningún plan se sale del disco ni pisa la copia de la tabla GPT.**
        ///
        /// El último MiB es donde GPT guarda su copia de respaldo. Una
        /// partición que llegue hasta ahí deja una tabla que algunos firmwares
        /// rechazan, y el equipo no arranca.
        #[test]
        fn ningun_plan_se_pasa_del_final(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                for p in &plan.particiones {
                    let fin = (p.inicio_mib + p.tamano_mib) * MIB;
                    proptest::prop_assert!(
                        fin <= disco.tamano_bytes - MIB,
                        "{nombre}: {p:?} termina en {fin} y el disco tiene {}",
                        disco.tamano_bytes
                    );
                    proptest::prop_assert!(
                        p.inicio_mib >= INICIO_MIB,
                        "{nombre}: {p:?} empieza encima de la tabla"
                    );
                    proptest::prop_assert!(p.tamano_mib > 0, "{nombre}: {p:?} vacía");
                }
            }
        }

        /// **Las particiones de un plan no se pisan entre sí.**
        ///
        /// archinstall lo valida —«Partitions overlap»— pero recién al empezar,
        /// que en el modo destructivo es con la tabla ya borrada.
        #[test]
        fn ningun_plan_se_pisa_a_si_mismo(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                let mut ordenadas = plan.particiones.clone();
                ordenadas.sort_by_key(|p| p.inicio_mib);
                for par in ordenadas.windows(2) {
                    proptest::prop_assert!(
                        par[0].inicio_mib + par[0].tamano_mib <= par[1].inicio_mib,
                        "{nombre}: {:?} y {:?} se pisan",
                        par[0], par[1]
                    );
                }
            }
        }

        /// **Todo plan tiene arranque, y el arranque nunca va cifrado.**
        ///
        /// Lo primero lo exige archinstall: `add_bootloader` busca una partición
        /// con la bandera `boot` **y** punto de montaje, y sin ella la
        /// instalación muere al llegar al cargador — con el disco ya formateado.
        ///
        /// Lo segundo es peor si falla: el firmware tiene que poder leer el ESP
        /// para arrancar, así que cifrarlo da un equipo que no enciende.
        #[test]
        fn todo_plan_arranca_y_el_esp_va_en_claro(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                let arranque = plan
                    .particiones
                    .iter()
                    .find(|p| p.banderas.contains(&"boot") && p.punto_montaje.is_some());
                proptest::prop_assert!(
                    arranque.is_some(),
                    "{nombre}: ninguna partición con bandera boot y punto de montaje: {:?}",
                    plan.particiones
                );
                let arranque = arranque.unwrap();
                proptest::prop_assert!(!arranque.cifrada, "{nombre}: el arranque va cifrado");
                proptest::prop_assert!(
                    arranque.sistema_archivos.is_some(),
                    "{nombre}: sin sistema de archivos, archinstall muere en safe_fs_type"
                );
            }
        }

        /// **Un ESP que ya existía nunca se formatea.**
        ///
        /// Es lo único que separa un dual boot que anda de un Windows que ya no
        /// arranca. Vale para los dos modos no destructivos, y por eso se
        /// comprueba sobre todos los planes en vez de en cada uno.
        #[test]
        fn un_esp_ajeno_nunca_se_formatea(
            disco in un_disco_usado(),
            cifrar in any::<bool>(),
        ) {
            let ajenos: Vec<&str> = disco
                .particiones
                .iter()
                .filter(|p| p.es_esp())
                .map(|p| p.ruta.as_str())
                .collect();
            if ajenos.is_empty() {
                return Ok(());
            }
            for (nombre, plan) in planes_de(&disco, SistemaArchivos::Btrfs, cifrar) {
                if plan.borrar_disco {
                    continue;
                }
                for p in &plan.particiones {
                    if let Some(ruta) = p.ruta.as_deref() {
                        if ajenos.contains(&ruta) {
                            proptest::prop_assert_eq!(
                                p.accion, Accion::Conservar,
                                "{}: el ESP {} no se conserva", nombre, ruta
                            );
                        }
                    }
                }
            }
        }
    }


}
