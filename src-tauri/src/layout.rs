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

use crate::protocol::SistemaArchivos;

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
/// **Un Windows recién instalado no llega**: su ESP es de 100 MiB. Ver
/// `ErrorPlan::EspChico`.
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
    if disco.tamano_bytes == 0 || disco.sector_logico == 0 {
        return Err(ErrorPlan::Invalido);
    }
    if disco.en_uso {
        return Err(ErrorPlan::EnUso);
    }
    if firmware != Firmware::Uefi {
        return Err(ErrorPlan::SoloUefi);
    }

    // El ESP de quien ya vive en el disco.
    let esp_existente = disco.particiones.iter().find(|p| p.es_esp());
    if let Some(esp) = esp_existente {
        let mib = esp.tamano_bytes / MIB;
        if mib < MINIMO_ESP_REUSABLE_MIB {
            return Err(ErrorPlan::EspChico {
                tiene_mib: mib,
                minimo_mib: MINIMO_ESP_REUSABLE_MIB,
            });
        }
    }

    let huecos = huecos_libres(disco);
    let mayor = huecos.first().copied().unwrap_or(Hueco {
        inicio_mib: INICIO_MIB,
        tamano_mib: 0,
    });

    // Lo que hace falta en el hueco: la raíz, más el ESP si hay que crearlo.
    let arranque_en_el_hueco = if esp_existente.is_some() {
        0
    } else {
        ARRANQUE_MIB
    };
    let minimo_mib = MINIMO_GIB * 1024 + arranque_en_el_hueco;
    if mayor.tamano_mib < minimo_mib {
        return Err(ErrorPlan::SinEspacioLibre {
            mayor_hueco_gib: mayor.tamano_mib / 1024,
        });
    }

    let mut plan = Vec::with_capacity(2);
    let mut cursor = mayor.inicio_mib;

    match esp_existente {
        // Se monta y no se formatea. Es la diferencia entera entre un dual boot
        // que anda y un Windows que ya no arranca.
        Some(esp) => plan.push(ParticionPlaneada {
            // La geometría es la que tiene: no se recalcula nada, porque no se
            // la va a tocar. Se manda igual porque archinstall la pide.
            inicio_mib: esp.inicio_bytes / MIB,
            tamano_mib: esp.tamano_bytes / MIB,
            // El que ya tiene. Formatearlo es exactamente lo que no se hace,
            // así que esto es sólo lo que se le informa a archinstall para que
            // sepa montarlo.
            sistema_archivos: Some("fat32"),
            punto_montaje: Some("/boot"),
            opciones_montaje: vec!["umask=0077".into()],
            banderas: vec!["boot", "esp"],
            subvolumenes: Vec::new(),
            cifrada: false,
            rol: Rol::Esp,
            accion: Accion::Conservar,
            ruta: Some(esp.ruta.clone()),
        }),
        // No hay ninguno: hay que hacerlo, y va al principio del hueco.
        None => {
            plan.push(ParticionPlaneada {
                inicio_mib: cursor,
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
            });
            cursor += ARRANQUE_MIB;
        }
    }

    let tamano_raiz = mayor
        .inicio_mib
        .saturating_add(mayor.tamano_mib)
        .saturating_sub(cursor);

    let usa_subvolumenes = fs == SistemaArchivos::Btrfs;
    plan.push(ParticionPlaneada {
        inicio_mib: cursor,
        tamano_mib: tamano_raiz,
        sistema_archivos: Some(fs.como_archinstall()),
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

    /// **Un ESP de Windows de fábrica no alcanza, y se dice.**
    ///
    /// Son 100 MiB y en VasakOS el ESP se monta en `/boot`: adentro van el
    /// kernel, los dos initramfs y el microcódigo. Fallar acá con un motivo es
    /// mucho mejor que instalar y que la primera actualización de kernel se
    /// quede sin espacio — que deja un sistema que no arranca.
    #[test]
    fn el_esp_de_cien_mib_de_windows_no_sirve() {
        let disco = disco_con_windows(500, 100);
        let err =
            planificar_junto_a(&disco, Firmware::Uefi, SistemaArchivos::Btrfs, false).unwrap_err();
        assert_eq!(
            err,
            ErrorPlan::EspChico {
                tiene_mib: 100,
                minimo_mib: MINIMO_ESP_REUSABLE_MIB
            }
        );
        // Y no se cae en crear un segundo ESP, que muchos firmwares no manejan.
        assert!(err.to_string().contains("100"));
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
}
