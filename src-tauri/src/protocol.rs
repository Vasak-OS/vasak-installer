//! El idioma que hablan la interfaz y el ayudante privilegiado.
//!
//! Son dos procesos: la ventana corre como el usuario de la sesión live y el
//! ayudante corre como root, lanzado con pkexec. Entre los dos hay **una línea
//! de JSON por mensaje** (NDJSON) sobre la entrada y la salida estándar del hijo.
//!
//! Por qué NDJSON y no D-Bus: el ayudante vive lo que vive la instalación y no
//! le sirve a nadie más, así que un servicio de sistema con su nombre en el bus
//! sería superficie de ataque sin usuario. Y por qué una línea por mensaje y no
//! un JSON por invocación: la instalación manda **cientos** de eventos de
//! progreso mientras sigue viva, así que el canal tiene que ser un flujo.
//!
//! La regla que sostiene todo esto: **una línea que no parsea se descarta y se
//! registra, nunca rompe el flujo.** archinstall y pacman escriben en la misma
//! terminal, y un `print()` perdido de una dependencia no puede matar la
//! instalación.

use serde::{Deserialize, Serialize};

/// Lo que la interfaz le pide al ayudante.
///
/// El `id` lo elige la interfaz y vuelve en la respuesta: sin él no se puede
/// saber a qué pregunta contesta un `reply` cuando hay dos en vuelo, y las hay
/// —el sondeo de discos y el del sistema salen juntos al abrir la ventana.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peticion {
    pub id: u64,
    #[serde(flatten)]
    pub cuerpo: CuerpoPeticion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CuerpoPeticion {
    /// Los discos del equipo, con sus particiones actuales.
    SondearDiscos,
    /// Firmware (UEFI o BIOS), memoria, CPU y si hay red.
    SondearSistema,
    /// Arranca la instalación. Es el punto sin retorno.
    Instalar(Box<PlanInstalacion>),
    /// Corta una instalación en curso. No deshace lo hecho: el disco queda a
    /// medias, y la interfaz lo dice con esas palabras.
    Cancelar,
}

/// Todo lo que la interfaz juntó, en la forma en que la interfaz lo piensa.
///
/// **No es** el JSON de archinstall: ese lo arma `archconfig.rs` a partir de
/// esto. La separación es a propósito — el esquema de archinstall cambia entre
/// versiones mayores y no queremos que un cambio suyo se filtre hasta los
/// componentes de Vue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInstalacion {
    /// Ruta del disco a usar, tal como la devolvió el sondeo (`/dev/nvme0n1`).
    pub disco: String,
    pub esquema: EsquemaDisco,
    pub sistema_archivos: SistemaArchivos,
    /// Cifrado LUKS de la raíz. La frase va aparte, en `secretos`.
    pub cifrar: bool,
    /// zram en lugar de partición de intercambio: no se le reserva disco y en
    /// una máquina con poca memoria rinde más.
    pub zram: bool,

    pub zona_horaria: String,
    /// Local del sistema sin el `.UTF-8` (`es_AR`). La codificación va aparte
    /// porque archinstall las quiere en dos campos.
    pub idioma_sistema: String,
    pub teclado: String,
    pub ntp: bool,

    pub hostname: String,
    pub nombre_completo: String,
    pub usuario: String,
    /// Si el usuario va al grupo `wheel` y a sudoers.
    pub administrador: bool,
    /// Cuenta de root habilitada. Si es `false` no se le pone contraseña y
    /// queda bloqueada, que es lo que hace un sistema con sudo.
    pub root_habilitado: bool,

    /// Los identificadores de los complementos elegidos: navegador,
    /// controladores, impresoras, extras. Ver `complementos.rs`.
    ///
    /// Van como identificadores y no como listas de paquetes: el frontend no
    /// tiene por qué saber qué instala «Impresoras», y así el catálogo se puede
    /// editar sin que el plan que viaja por el canal cambie de forma.
    ///
    /// `default` para que un plan viejo —o un test— sin este campo siga
    /// deserializando: el instalador funcionaba antes de que existieran, y la
    /// ausencia significa «ninguno».
    #[serde(default)]
    pub complementos: Vec<String>,

    /// Contraseñas en claro. Van en la petición y **nunca** a un archivo: el
    /// ayudante las convierte en hash antes de escribir el archivo de
    /// credenciales que lee archinstall.
    pub secretos: Secretos,
}

/// Aparte del resto para que sea evidente qué campos no se pueden registrar.
///
/// `Debug` está escrito a mano justamente por eso: la petición entera se
/// registra en el diario cuando algo falla, y el derivado habría puesto las
/// contraseñas ahí.
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Secretos {
    pub usuario: String,
    pub root: String,
    /// Frase del volumen LUKS. Vacía si `cifrar` es `false`.
    pub cifrado: String,
}

impl std::fmt::Debug for Secretos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secretos")
            .field("usuario", &"(oculto)")
            .field("root", &"(oculto)")
            .field("cifrado", &"(oculto)")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EsquemaDisco {
    /// Borra el disco entero y arma la tabla de cero.
    BorrarTodo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SistemaArchivos {
    Btrfs,
    Ext4,
    Xfs,
}

impl SistemaArchivos {
    /// El nombre que usa archinstall en `fs_type`.
    pub fn como_archinstall(self) -> &'static str {
        match self {
            SistemaArchivos::Btrfs => "btrfs",
            SistemaArchivos::Ext4 => "ext4",
            SistemaArchivos::Xfs => "xfs",
        }
    }
}

/// Lo que el ayudante manda hacia arriba.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mensaje {
    /// Respuesta a una petición, con su `id`.
    Reply {
        id: u64,
        #[serde(flatten)]
        resultado: Resultado,
    },
    /// Un paso de la instalación cambió de estado.
    Progress(Progreso),
    /// Una línea de salida de archinstall o de pacman, para el registro que la
    /// interfaz muestra plegado.
    Log { nivel: Nivel, linea: String },
    /// La instalación terminó, bien o mal. Es terminal: después de esto el
    /// ayudante cierra.
    Done { ok: bool, error: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Resultado {
    Ok { ok: bool, data: serde_json::Value },
    Err { ok: bool, error: String },
}

impl Resultado {
    pub fn correcto(data: serde_json::Value) -> Self {
        Resultado::Ok { ok: true, data }
    }

    pub fn fallido(error: impl Into<String>) -> Self {
        Resultado::Err {
            ok: false,
            error: error.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Nivel {
    Info,
    Warn,
    Error,
}

/// Un paso con nombre propio, no un porcentaje suelto.
///
/// La fracción es del paso, no del total: la interfaz sabe cuántos pasos hay y
/// arma la barra general con eso. Un porcentaje global calculado en el backend
/// obliga a que el backend sepa cuánto pesa cada paso, y no lo sabe —pacstrap
/// de 1500 paquetes contra escribir un fstab no se comparan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progreso {
    pub paso: Paso,
    pub estado: EstadoPaso,
    /// Avance dentro del paso, 0.0 a 1.0. `None` cuando no se puede saber
    /// —montar no tiene mitades— y ahí la interfaz muestra una barra indefinida.
    pub fraccion: Option<f32>,
    /// Detalle para mostrar debajo del nombre del paso: el paquete que está
    /// bajando, el punto de montaje que está creando. Ya viene traducido no:
    /// viene crudo de archinstall, y la interfaz lo muestra como detalle
    /// técnico, no como texto de interfaz.
    pub detalle: Option<String>,
}

/// Los pasos de la instalación, en orden.
///
/// Están acá y no en el frontend porque el ayudante es quien decide cuándo
/// empieza cada uno, y porque los ganchos del plugin de archinstall se mapean
/// exactamente a esta lista. Agregar un paso es agregarlo en los tres lugares:
/// acá, en el plugin y en los catálogos de idioma —hay un test que verifica lo
/// último.
/// **`camelCase` y no `snake_case`.** Es lo que hace que la representación de
/// serde coincida con `clave()`, que es lo que emite el plugin de Python y lo
/// que espera el frontend.
///
/// Con `snake_case`, `SistemaBase` serializaba como `sistema_base` mientras el
/// plugin escribía `sistemaBase`: el ayudante no podía deserializar sus eventos
/// y los descartaba como «evento ilegible del plugin». Se perdían justo los del
/// paso más largo —el `pacstrap` del sistema base—, así que la barra se quedaba
/// quieta durante la mayor parte de la instalación. El resto de las variantes
/// son de una sola palabra y se ven iguales en los dos estilos, que es lo que
/// hizo que pasara desapercibido.
///
/// El test `la_representacion_de_serde_es_la_clave` los ata para siempre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Paso {
    /// Escribir la tabla de particiones y formatear.
    Particionar,
    /// Montar el destino en el orden correcto (`/` antes que `/boot`).
    Montar,
    /// Elegir espejos y sumar el repositorio de VasakOS.
    Espejos,
    /// pacstrap del sistema base. El paso largo.
    SistemaBase,
    /// Los paquetes del escritorio de VasakOS.
    Escritorio,
    /// initramfs, GRUB y la entrada de arranque.
    Arranque,
    /// Cuentas, contraseñas y grupos.
    Usuarios,
    /// Zona horaria, teclado, local, red y servicios.
    Configuracion,
    /// Los ajustes propios de VasakOS sobre el sistema instalado.
    Vasakos,
    /// Desmontar y cerrar.
    Cierre,
}

impl Paso {
    /// En orden. La interfaz la usa para armar la lista y para saber cuánto
    /// falta; el orden acá es el orden real de ejecución.
    pub const TODOS: &'static [Paso] = &[
        Paso::Particionar,
        Paso::Montar,
        Paso::Espejos,
        Paso::SistemaBase,
        Paso::Escritorio,
        Paso::Arranque,
        Paso::Usuarios,
        Paso::Configuracion,
        Paso::Vasakos,
        Paso::Cierre,
    ];

    /// La clave del catálogo de idioma. La interfaz la resuelve con `t()`.
    pub fn clave(self) -> &'static str {
        match self {
            Paso::Particionar => "particionar",
            Paso::Montar => "montar",
            Paso::Espejos => "espejos",
            Paso::SistemaBase => "sistemaBase",
            Paso::Escritorio => "escritorio",
            Paso::Arranque => "arranque",
            Paso::Usuarios => "usuarios",
            Paso::Configuracion => "configuracion",
            Paso::Vasakos => "vasakos",
            Paso::Cierre => "cierre",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoPaso {
    Pendiente,
    EnCurso,
    Hecho,
    Fallado,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El `Debug` de `Secretos` está escrito a mano para que la petición se
    /// pueda registrar sin filtrar contraseñas. Si alguien lo cambia por el
    /// derivado, esto lo agarra.
    #[test]
    fn el_debug_de_secretos_no_muestra_las_contrasenas() {
        let s = Secretos {
            usuario: "correo-caballo-bateria".into(),
            root: "otra-distinta".into(),
            cifrado: "frase-del-disco".into(),
        };
        let texto = format!("{s:?}");
        assert!(!texto.contains("correo-caballo-bateria"), "{texto}");
        assert!(!texto.contains("otra-distinta"), "{texto}");
        assert!(!texto.contains("frase-del-disco"), "{texto}");
    }

    /// El `flatten` de `CuerpoPeticion` sobre `Peticion` es lo que permite
    /// mandar `{"id":1,"kind":"sondear_discos"}` en vez de anidar. Un cambio de
    /// serde que rompa eso cambiaría el protocolo en silencio.
    #[test]
    fn la_peticion_es_plana() {
        let p = Peticion {
            id: 7,
            cuerpo: CuerpoPeticion::SondearDiscos,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, r#"{"id":7,"kind":"sondear_discos"}"#);

        let vuelta: Peticion = serde_json::from_str(&json).unwrap();
        assert_eq!(vuelta.id, 7);
        assert!(matches!(vuelta.cuerpo, CuerpoPeticion::SondearDiscos));
    }

    /// La representación de serde tiene que ser **exactamente** `clave()`.
    ///
    /// Son dos caminos hacia el mismo nombre: `clave()` lo usan el frontend y
    /// los catálogos de idioma; serde lo usa el ayudante para leer los eventos
    /// del plugin de Python, que escribe esos mismos nombres. Si se separan, los
    /// eventos de ese paso se descartan en silencio como «evento ilegible» y la
    /// barra se queda quieta sin que nada falle.
    ///
    /// Ya pasó: con `rename_all = "snake_case"`, `SistemaBase` era
    /// `sistema_base` para serde y `sistemaBase` para todo lo demás.
    #[test]
    fn la_representacion_de_serde_es_la_clave() {
        for paso in Paso::TODOS {
            let serializado = serde_json::to_string(paso).unwrap();
            let esperado = format!("\"{}\"", paso.clave());
            assert_eq!(
                serializado, esperado,
                "{paso:?}: serde escribe {serializado} y clave() dice {esperado}"
            );

            // Y a la inversa: lo que escribe el plugin tiene que volver a
            // parsear. Es el camino que de verdad se recorre en producción.
            let vuelta: Paso = serde_json::from_str(&esperado).unwrap();
            assert_eq!(vuelta, *paso);
        }
    }

    /// El mensaje completo que emite el plugin para el paso que estaba roto.
    ///
    /// Se prueba la línea entera y no sólo el enum, porque lo que falla en
    /// producción es el `from_str::<Mensaje>` del seguidor de eventos.
    #[test]
    fn el_evento_del_plugin_para_el_sistema_base_se_parsea() {
        let linea = r#"{"type":"progress","paso":"sistemaBase","estado":"en_curso","fraccion":null,"detalle":null}"#;
        let mensaje: Mensaje = serde_json::from_str(linea).expect("el plugin emite esto");
        match mensaje {
            Mensaje::Progress(p) => {
                assert_eq!(p.paso, Paso::SistemaBase);
                assert_eq!(p.estado, EstadoPaso::EnCurso);
            }
            otro => panic!("se parseó como {otro:?}"),
        }
    }

    /// Cada paso tiene que estar en `TODOS` exactamente una vez: la interfaz
    /// arma la lista con eso y un paso repetido o faltante deja la barra de
    /// progreso mintiendo.
    #[test]
    fn todos_los_pasos_estan_una_vez() {
        let mut claves: Vec<&str> = Paso::TODOS.iter().map(|p| p.clave()).collect();
        let cantidad = claves.len();
        claves.sort_unstable();
        claves.dedup();
        assert_eq!(claves.len(), cantidad, "hay un paso repetido en TODOS");
        assert_eq!(cantidad, 10, "se agregó o se quitó un paso sin actualizar el test");
    }

    /// `Resultado` va sin etiqueta dentro del `reply`, así que el frontend lee
    /// `ok` para decidir. Si serde cambiara el orden de las variantes de
    /// `untagged`, un error podría parsearse como éxito.
    #[test]
    fn el_resultado_distingue_exito_de_error() {
        let ok = serde_json::to_string(&Mensaje::Reply {
            id: 1,
            resultado: Resultado::correcto(serde_json::json!({"discos": []})),
        })
        .unwrap();
        assert!(ok.contains(r#""ok":true"#), "{ok}");
        assert!(ok.contains(r#""discos""#), "{ok}");

        let err = serde_json::to_string(&Mensaje::Reply {
            id: 2,
            resultado: Resultado::fallido("no hay discos"),
        })
        .unwrap();
        assert!(err.contains(r#""ok":false"#), "{err}");
        assert!(err.contains("no hay discos"), "{err}");
    }
}
