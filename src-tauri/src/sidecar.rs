//! El lado de la ventana del canal con el ayudante privilegiado.
//!
//! Lanza `pkexec …/vasak-installer-helper`, le manda peticiones y reparte lo que
//! vuelve: las respuestas a quien las pidió, y el progreso y el registro a la
//! interfaz como eventos de Tauri.
//!
//! El ayudante **no se lanza al abrir la aplicación**. Una ventana que arranca
//! pidiendo autorización antes de mostrar nada le pide a la persona que apruebe
//! algo que todavía no sabe qué es. Se lanza al llegar al paso del disco, que es
//! donde por primera vez hace falta root y donde ya se entiende para qué.
//!
//! Casi todo el sondeo no pasa por acá: `lsblk`, `/proc` y los catálogos de
//! `/usr/share` se leen sin privilegios desde este mismo proceso. Root hace
//! falta para dos cosas nada más — ponerle nombre a los sistemas operativos que
//! ya están instalados (`os-prober` monta particiones) y la instalación.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::protocol::{CuerpoPeticion, Mensaje, Peticion, Resultado};

/// Eventos que la interfaz escucha.
pub const EVENTO_PROGRESO: &str = "instalacion://progreso";
pub const EVENTO_REGISTRO: &str = "instalacion://registro";
pub const EVENTO_FIN: &str = "instalacion://fin";
/// El ayudante murió sin haber terminado. La interfaz lo trata como fallo.
pub const EVENTO_CAIDO: &str = "instalacion://ayudante-caido";

/// Cuánto se espera una respuesta del ayudante.
///
/// Generoso porque `os-prober` monta cada partición ajena para mirarla adentro y
/// en un disco con cuatro sistemas tarda de verdad. No cubre la instalación: esa
/// no espera respuesta, avisa por eventos.
const ESPERA_RESPUESTA: Duration = Duration::from_secs(120);

/// Dónde vive el ayudante, en orden de preferencia.
///
/// La variable de entorno primero para poder desarrollar sin instalar el
/// paquete. `/usr/lib` y no `/usr/bin` porque no es un programa que alguien
/// quiera ejecutar a mano: es la mitad privilegiada de esta aplicación.
fn ruta_ayudante() -> Option<PathBuf> {
    if let Some(desde_entorno) = std::env::var_os("VASAK_INSTALLER_HELPER") {
        let ruta = PathBuf::from(desde_entorno);
        if ruta.is_file() {
            return Some(ruta);
        }
    }
    let candidatas = [
        PathBuf::from("/usr/lib/vasak-installer/vasak-installer-helper"),
        PathBuf::from("target/debug/vasak-installer-helper"),
        PathBuf::from("target/release/vasak-installer-helper"),
        PathBuf::from("src-tauri/target/debug/vasak-installer-helper"),
        PathBuf::from("src-tauri/target/release/vasak-installer-helper"),
    ];
    candidatas.into_iter().find(|c| c.is_file())
}

pub struct Ayudante {
    hijo: Mutex<Child>,
    entrada: Mutex<ChildStdin>,
    siguiente_id: AtomicU64,
    /// Quién está esperando cada `id`. El hilo lector busca acá cuando llega un
    /// `reply`.
    pendientes: Arc<Mutex<HashMap<u64, mpsc::Sender<Resultado>>>>,
}

impl Ayudante {
    /// Lanza el ayudante con pkexec.
    ///
    /// En el medio live la regla de polkit de VasakOS lo permite sin preguntar,
    /// así que esto no muestra nada. Corrido desde un sistema instalado, pide
    /// autorización de administrador — que es lo correcto, y por eso la regla
    /// permisiva va en la ISO y no en el paquete.
    pub fn lanzar(app: AppHandle) -> Result<Self, String> {
        let ruta = ruta_ayudante().ok_or_else(|| {
            "no se encontró el ayudante privilegiado. Con el paquete instalado tendría que \
             estar en /usr/lib/vasak-installer/vasak-installer-helper; para desarrollo, \
             compilalo y apuntá VASAK_INSTALLER_HELPER a él."
                .to_string()
        })?;

        let mut hijo = Command::new("pkexec")
            .arg(&ruta)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // El error del ayudante no se captura a propósito: va a la misma
            // terminal que la aplicación y de ahí al diario del sistema. Un
            // pánico en el ayudante tiene que quedar registrado aunque el canal
            // NDJSON ya no funcione, que es exactamente cuando pasa.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("no se pudo lanzar pkexec: {e}"))?;

        let entrada = hijo
            .stdin
            .take()
            .ok_or_else(|| "el ayudante no aceptó entrada".to_string())?;
        let stdout = hijo
            .stdout
            .take()
            .ok_or_else(|| "el ayudante no devolvió salida".to_string())?;

        let pendientes: Arc<Mutex<HashMap<u64, mpsc::Sender<Resultado>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        {
            let pendientes = Arc::clone(&pendientes);
            std::thread::spawn(move || {
                // Si llegó un `Done`, el ayudante cierra porque terminó, no
                // porque se cayó. Sin esta marca, **toda instalación exitosa**
                // emitía además el evento de caída, y la interfaz tenía que
                // adivinar por su cuenta que no era un fallo — con guardas que
                // dependen del orden en que Tauri entrega los eventos.
                let mut termino_bien = false;
                let lector = BufReader::new(stdout);
                for linea in lector.lines() {
                    let Ok(linea) = linea else { break };
                    if linea.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Mensaje>(&linea) {
                        Ok(mensaje) => {
                            if matches!(mensaje, Mensaje::Done { .. }) {
                                termino_bien = true;
                            }
                            repartir(&app, &pendientes, mensaje);
                        }
                        // pkexec escribe sus propios mensajes por esta misma
                        // salida cuando la autorización falla, y no son JSON.
                        // Se muestran como registro en vez de descartarse: son
                        // justamente el texto que explica por qué no arrancó.
                        Err(_) => {
                            let _ = app.emit(
                                EVENTO_REGISTRO,
                                serde_json::json!({ "nivel": "warn", "linea": linea }),
                            );
                        }
                    }
                }

                // El ayudante cerró. Se despierta a todo el que estaba
                // esperando: sin esto, una respuesta que nunca llega deja la
                // interfaz con el spinner puesto hasta el plazo, y el plazo son
                // dos minutos.
                let mut guard = match pendientes.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                for (_, tx) in guard.drain() {
                    let _ = tx.send(Resultado::fallido("el ayudante privilegiado se cerró"));
                }

                // La caída se avisa **sólo si no hubo `Done`**: el ayudante sale
                // por su cuenta apenas termina la instalación, y ese cierre es
                // parte del funcionamiento normal.
                if !termino_bien {
                    let _ = app.emit(EVENTO_CAIDO, ());
                }
            });
        }

        Ok(Self {
            hijo: Mutex::new(hijo),
            entrada: Mutex::new(entrada),
            siguiente_id: AtomicU64::new(1),
            pendientes,
        })
    }

    /// Manda una petición y espera la respuesta.
    pub fn pedir(&self, cuerpo: CuerpoPeticion) -> Result<Value, String> {
        let id = self.siguiente_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();

        // El receptor se registra **antes** de escribir. Al revés hay una
        // carrera real: el ayudante contesta el sondeo de discos en
        // milisegundos, y una respuesta que llega antes del registro no
        // encuentra a quién entregársela.
        {
            let mut guard = self
                .pendientes
                .lock()
                .map_err(|_| "el canal con el ayudante está roto".to_string())?;
            guard.insert(id, tx);
        }

        let peticion = Peticion { id, cuerpo };
        let texto = serde_json::to_string(&peticion).map_err(|e| e.to_string())?;

        // Si la escritura falla, la respuesta no va a llegar nunca: la entrada
        // pendiente se saca antes de devolver el error. Sin esto quedaba en el
        // mapa hasta que el ayudante cerrara, y cada petición fallida dejaba la
        // suya — un goteo que además hace que el aviso de «llegó tarde la
        // respuesta N» aparezca por ids que ya nadie espera.
        let escritura = {
            let mut entrada = match self.entrada.lock() {
                Ok(e) => e,
                Err(_) => {
                    self.olvidar(id);
                    return Err("el canal con el ayudante está roto".into());
                }
            };
            writeln!(entrada, "{texto}").and_then(|_| entrada.flush())
        };
        if let Err(e) = escritura {
            self.olvidar(id);
            return Err(format!("no se pudo escribirle al ayudante: {e}"));
        }

        match rx.recv_timeout(ESPERA_RESPUESTA) {
            Ok(Resultado::Ok { data, .. }) => Ok(data),
            Ok(Resultado::Err { error, .. }) => Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.olvidar(id);
                Err(format!(
                    "el ayudante no contestó en {} segundos",
                    ESPERA_RESPUESTA.as_secs()
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("el ayudante privilegiado se cerró".into())
            }
        }
    }

    fn olvidar(&self, id: u64) {
        if let Ok(mut guard) = self.pendientes.lock() {
            guard.remove(&id);
        }
    }

    /// Manda una petición sin esperar respuesta.
    ///
    /// Para `Cancelar`: el ayudante la atiende en su hilo de entrada
    /// justamente para poder matar la instalación mientras el resto está
    /// ocupado, así que esperarle una respuesta sería esperar a que termine lo
    /// que se está cancelando.
    pub fn enviar(&self, cuerpo: CuerpoPeticion) -> Result<(), String> {
        let id = self.siguiente_id.fetch_add(1, Ordering::Relaxed);
        let texto = serde_json::to_string(&Peticion { id, cuerpo }).map_err(|e| e.to_string())?;
        let mut entrada = self
            .entrada
            .lock()
            .map_err(|_| "el canal con el ayudante está roto".to_string())?;
        writeln!(entrada, "{texto}")
            .and_then(|_| entrada.flush())
            .map_err(|e| format!("no se pudo escribirle al ayudante: {e}"))
    }

    /// Si el ayudante sigue vivo.
    ///
    /// `try_wait` y no `wait`: preguntar sin bloquear es todo el punto. Con
    /// `wait` esta función colgaría hasta que el ayudante termine, que es hasta
    /// el final de la instalación.
    pub fn vivo(&self) -> bool {
        let Ok(mut hijo) = self.hijo.lock() else {
            return false;
        };
        matches!(hijo.try_wait(), Ok(None))
    }
}

impl Drop for Ayudante {
    fn drop(&mut self) {
        // Cerrar la entrada es lo que le dice al ayudante que se vaya: su hilo
        // lector ve el fin de archivo y sale. Matarlo sería peor —si está a
        // mitad de un `mkfs`, dejaría el disco en un estado más raro que
        // dejándolo terminar.
        if let Ok(mut hijo) = self.hijo.lock() {
            let _ = hijo.try_wait();
        }
    }
}

fn repartir(
    app: &AppHandle,
    pendientes: &Arc<Mutex<HashMap<u64, mpsc::Sender<Resultado>>>>,
    mensaje: Mensaje,
) {
    match mensaje {
        Mensaje::Reply { id, resultado } => {
            let tx = {
                let mut guard = match pendientes.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                guard.remove(&id)
            };
            match tx {
                Some(tx) => {
                    let _ = tx.send(resultado);
                }
                // Una respuesta sin nadie esperándola es una que llegó después
                // del plazo. Se registra: significa que el plazo está corto.
                None => {
                    let _ = app.emit(
                        EVENTO_REGISTRO,
                        serde_json::json!({
                            "nivel": "warn",
                            "linea": format!("llegó tarde la respuesta {id}")
                        }),
                    );
                }
            }
        }
        Mensaje::Progress(p) => {
            let _ = app.emit(EVENTO_PROGRESO, p);
        }
        Mensaje::Log { nivel, linea } => {
            let _ = app.emit(
                EVENTO_REGISTRO,
                serde_json::json!({ "nivel": nivel, "linea": linea }),
            );
        }
        Mensaje::Done { ok, error } => {
            let _ = app.emit(EVENTO_FIN, serde_json::json!({ "ok": ok, "error": error }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La variable de entorno tiene que ganarle a las rutas fijas: es lo que
    /// permite desarrollar sin instalar el paquete, y si perdiera, un
    /// `/usr/lib/vasak-installer` de una versión vieja se usaría en vez del
    /// recién compilado.
    #[test]
    fn la_variable_de_entorno_manda() {
        let dir = std::env::temp_dir().join(format!("vsk-helper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let falso = dir.join("ayudante-de-prueba");
        std::fs::write(&falso, "#!/bin/sh\n").unwrap();

        std::env::set_var("VASAK_INSTALLER_HELPER", &falso);
        assert_eq!(ruta_ayudante(), Some(falso.clone()));

        // Una ruta que no existe no se devuelve: si se devolviera, el error
        // sería «pkexec no pudo ejecutar» en vez de «no se encontró el
        // ayudante», que dice mucho menos.
        std::env::set_var("VASAK_INSTALLER_HELPER", dir.join("no-existe"));
        assert_ne!(ruta_ayudante(), Some(dir.join("no-existe")));

        std::env::remove_var("VASAK_INSTALLER_HELPER");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Los nombres de los eventos son contrato con el frontend. Cambiar uno acá
    /// sin cambiarlo allá deja la barra de progreso quieta sin ningún error.
    #[test]
    fn los_nombres_de_los_eventos_no_cambian_sin_querer() {
        assert_eq!(EVENTO_PROGRESO, "instalacion://progreso");
        assert_eq!(EVENTO_REGISTRO, "instalacion://registro");
        assert_eq!(EVENTO_FIN, "instalacion://fin");
        assert_eq!(EVENTO_CAIDO, "instalacion://ayudante-caido");
    }
}
