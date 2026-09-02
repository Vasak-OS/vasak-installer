//! Que los paquetes elegidos se puedan instalar juntos — **comprobado antes de
//! formatear**.
//!
//! # Por qué existe
//!
//! Una instalación real murió así: alguien eligió Firefox, Firefox arrastra
//! `ffmpeg`, `ffmpeg` pide el `jack` virtual —que tiene dos proveedores— y
//! pacman eligió `jack2`, que conflictúa con el `pipewire-jack` que el
//! escritorio exige. `pacstrap` salió con «unresolvable package conflicts
//! detected» **después** de haber formateado el disco.
//!
//! Eso último es lo que hace que valga un módulo entero. El error en sí se
//! arregla nombrando el proveedor como objetivo —`paquetes.txt` ya lo hace con
//! los tres que conocemos— pero esa lista se mantiene a mano, y el próximo
//! complemento que sume una dependencia virtual nueva vuelve a romper igual: sin
//! aviso, y con el disco ya en blanco.
//!
//! Esto no elige proveedores ni arregla la lista. Lo único que hace es
//! **preguntar antes**, cuando el disco todavía está intacto y el peor
//! desenlace posible es un mensaje.
//!
//! # Por qué no alcanza `pacman -Sp`
//!
//! Porque no detecta conflictos. Comprobado sobre una raíz vacía con la lista
//! que rompió la instalación: imprime `jack2` **y** `pipewire-jack` en la misma
//! salida y termina con código 0. La detección de conflictos vive en la
//! preparación de la transacción, que `-p` se saltea.
//!
//! Así que la resolución se le pide a pacman —eso lo hace bien— y la
//! comparación la hacemos acá, con lo que la propia base de datos declara: dos
//! paquetes no pueden convivir si uno choca con el nombre del otro, o con un
//! nombre virtual que el otro ocupa. Es la misma regla que aplica pacman.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// Lo que la base de datos dice de un paquete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ficha {
    pub nombre: String,
    /// Los nombres que ocupa además del suyo, incluidos los virtuales.
    pub provee: BTreeSet<String>,
    /// Con qué declara no poder convivir.
    pub conflictos: BTreeSet<String>,
}

/// Dos paquetes que no pueden instalarse juntos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choque {
    pub uno: String,
    pub otro: String,
    /// El nombre por el que chocan: el de un paquete, o el virtual que ocupa.
    /// Es el dato que dice qué proveedor hay que fijar.
    pub por: String,
}

impl std::fmt::Display for Choque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} y {} no pueden convivir (por «{}»)", self.uno, self.otro, self.por)
    }
}

/// Le saca la restricción de versión a una entrada de dependencia.
///
/// `foo>=1.2` es `foo` para comparar nombres. Los `libfoo.so=0-64` quedan con su
/// nombre de biblioteca, que es lo que hay que comparar de ellos.
fn nombre_solo(entrada: &str) -> &str {
    match entrada.find(['<', '>', '=']) {
        Some(i) => &entrada[..i],
        None => entrada,
    }
}

/// Si la entrada lleva restricción de versión.
///
/// **Las versionadas se ignoran**, y es a propósito. Un `conflicts=('foo<1.2')`
/// suele estar satisfecho en una instalación nueva, donde todo viene al día, así
/// que tratarlo como un choque frenaría instalaciones legítimas. Este módulo
/// puede pasar por alto un conflicto; lo que no puede es inventar uno, porque de
/// eso depende que se pueda instalar.
fn versionada(entrada: &str) -> bool {
    entrada.contains(['<', '>', '='])
}

/// Lee la salida de `LC_ALL=C pacman -Si`.
///
/// El `LC_ALL=C` no es decorativo: los nombres de los campos están traducidos, y
/// en un sistema en español esto tendría que buscar «En conflicto con». Se fija
/// el idioma al invocar y acá se leen los nombres en inglés, que son estables.
///
/// Los valores largos siguen en las líneas de abajo, indentados — pacman corta
/// las listas al ancho de la terminal— así que una continuación se pega al campo
/// que venía.
pub fn leer_fichas(salida: &str) -> Vec<Ficha> {
    let mut fichas = Vec::new();
    let mut nombre = String::new();
    let mut provee = BTreeSet::new();
    let mut conflictos = BTreeSet::new();
    // Cuál de los tres campos que nos interesan viene continuando.
    let mut campo = "";

    let cerrar = |nombre: &mut String,
                  provee: &mut BTreeSet<String>,
                  conflictos: &mut BTreeSet<String>,
                  fichas: &mut Vec<Ficha>| {
        if !nombre.is_empty() {
            fichas.push(Ficha {
                nombre: std::mem::take(nombre),
                provee: std::mem::take(provee),
                conflictos: std::mem::take(conflictos),
            });
        }
    };

    for linea in salida.lines() {
        if linea.trim().is_empty() {
            cerrar(&mut nombre, &mut provee, &mut conflictos, &mut fichas);
            campo = "";
            continue;
        }

        let continuacion = linea.starts_with(char::is_whitespace);
        if continuacion {
            if !campo.is_empty() {
                let destino = if campo == "provee" { &mut provee } else { &mut conflictos };
                sumar(destino, linea);
            }
            continue;
        }

        let Some((clave, valor)) = linea.split_once(':') else {
            campo = "";
            continue;
        };
        let valor = valor.trim();

        match clave.trim() {
            "Name" => {
                // Un nuevo `Name` sin línea vacía en el medio cierra el anterior:
                // `pacman -Si` de un paquete que está en dos repositorios lo
                // imprime dos veces seguidas.
                cerrar(&mut nombre, &mut provee, &mut conflictos, &mut fichas);
                nombre = valor.to_string();
                campo = "";
            }
            "Provides" => {
                campo = "provee";
                sumar(&mut provee, valor);
            }
            "Conflicts With" => {
                campo = "conflictos";
                sumar(&mut conflictos, valor);
            }
            // Cualquier otro campo corta la continuación: si no, la lista de
            // `Depends On` se sumaría al campo anterior.
            _ => campo = "",
        }
    }
    cerrar(&mut nombre, &mut provee, &mut conflictos, &mut fichas);

    fichas
}

/// Suma las entradas de una línea al conjunto, salteando `None` y las vacías.
fn sumar(destino: &mut BTreeSet<String>, texto: &str) {
    for entrada in texto.split_whitespace() {
        if entrada != "None" {
            destino.insert(entrada.to_string());
        }
    }
}

/// Busca pares que no puedan convivir en un conjunto ya resuelto.
///
/// La regla es la de pacman: un paquete choca con otro si lo nombra en sus
/// conflictos, o si nombra un virtual que el otro ocupa. Se mira en las dos
/// direcciones porque sólo uno de los dos suele declararlo —`pipewire-jack`
/// nombra a `jack2`, y `jack2` no lo nombra a él— y **cada par se informa una
/// vez**, que con datos reales importa más de lo que parece: `pacman -Si` sobre
/// 689 paquetes devolvió 1246 fichas, porque las que están en dos repositorios
/// vienen dos veces. Sin deduplicar, un solo problema se informaba dos veces y
/// la comparación hacía cuatro veces el trabajo.
///
/// De un nombre repetido se conserva la primera ficha. Es la que pacman
/// instalaría: elige por orden de repositorio, y ése es el orden en que las
/// imprimió.
pub fn choques(fichas: &[Ficha]) -> Vec<Choque> {
    // Índice de nombre ocupado → quién lo ocupa, incluido el nombre propio.
    // Con esto cada conflicto se resuelve por búsqueda en lugar de recorrer
    // todas las fichas contra todas.
    let mut unicas: Vec<&Ficha> = Vec::with_capacity(fichas.len());
    let mut vistas: BTreeSet<&str> = BTreeSet::new();
    for ficha in fichas {
        if vistas.insert(ficha.nombre.as_str()) {
            unicas.push(ficha);
        }
    }

    let mut duenos: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
    for ficha in &unicas {
        duenos.entry(ficha.nombre.as_str()).or_default().push(ficha.nombre.as_str());
        for provisto in &ficha.provee {
            duenos.entry(nombre_solo(provisto)).or_default().push(ficha.nombre.as_str());
        }
    }

    let mut encontrados = Vec::new();
    // Los pares ya informados, con los dos nombres en orden, para que «a choca
    // con b» y «b choca con a» sean el mismo hallazgo.
    let mut informados: BTreeSet<(&str, &str)> = BTreeSet::new();

    for ficha in &unicas {
        for conflicto in &ficha.conflictos {
            // Las versionadas no cuentan: ver [`versionada`].
            if versionada(conflicto) {
                continue;
            }
            let nombre = nombre_solo(conflicto);
            let Some(candidatos) = duenos.get(nombre) else {
                continue;
            };

            for otro in candidatos {
                // Un paquete que choca con un virtual que él mismo ocupa —como
                // `jack2` con `jack`— no choca consigo mismo.
                if *otro == ficha.nombre.as_str() {
                    continue;
                }
                let par = if ficha.nombre.as_str() < *otro {
                    (ficha.nombre.as_str(), *otro)
                } else {
                    (*otro, ficha.nombre.as_str())
                };
                if informados.insert(par) {
                    encontrados.push(Choque {
                        uno: par.0.to_string(),
                        otro: par.1.to_string(),
                        por: nombre.to_string(),
                    });
                }
            }
        }
    }

    encontrados
}

/// Resuelve la lista contra una raíz vacía y devuelve los pares incompatibles.
///
/// `dir` es donde se arma esa raíz: tiene que ser escribible y se deja limpia
/// antes de usarla, para que no quede el resultado de una corrida anterior.
///
/// **Un fallo de este chequeo no es un fallo de la instalación.** Devuelve `Err`
/// cuando no pudo comprobar —no hay base de datos sincronizada, pacman no está,
/// se cayó la red— y quien lo llama tiene que seguir adelante avisando. Al revés
/// sería peor que el problema que resuelve: un chequeo roto dejaría el
/// instalador sin poder instalar nada.
pub fn revisar(paquetes: &[String], dir: &Path) -> Result<Vec<Choque>, String> {
    if paquetes.is_empty() {
        return Ok(Vec::new());
    }

    let raiz = dir.join("resolucion");
    let db = raiz.join("var/lib/pacman");
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(&db).map_err(|e| format!("no se pudo crear {}: {e}", db.display()))?;

    let nombres = resolver(paquetes, &raiz, &db)?;
    if nombres.is_empty() {
        return Err("pacman no devolvió ningún paquete".into());
    }

    let fichas = describir(&nombres, &db)?;
    Ok(choques(&fichas))
}

/// Los nombres de todo lo que se instalaría, dependencias incluidas.
///
/// Se sincroniza primero para resolver con la misma base que va a usar
/// `pacstrap`: la del medio vivo es de cuando se armó la imagen, y una lista
/// vieja daría un veredicto viejo. Si sincronizar no se puede —sin red, sin
/// privilegios— se cae a copiar la base que haya, que es mejor que no comprobar.
fn resolver(paquetes: &[String], raiz: &Path, db: &Path) -> Result<Vec<String>, String> {
    let imprimir = |sincronizar: bool| -> Result<std::process::Output, std::io::Error> {
        let mut comando = Command::new("pacman");
        comando.arg(if sincronizar { "-Syp" } else { "-Sp" });
        comando
            .args(["--print-format", "%n"])
            .arg("--root")
            .arg(raiz)
            .arg("--dbpath")
            .arg(db)
            .arg("--noconfirm")
            .args(paquetes)
            .output()
    };

    let salida = match imprimir(true) {
        Ok(salida) if salida.status.success() => salida,
        _ => {
            copiar_base(db)?;
            imprimir(false).map_err(|e| format!("no se pudo ejecutar pacman: {e}"))?
        }
    };

    if !salida.status.success() {
        // El error de pacman importa tal cual: si la lista ya es irresoluble por
        // otro motivo —un paquete que no existe— eso es lo que hay que decir.
        return Err(String::from_utf8_lossy(&salida.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&salida.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Copia la base sincronizada del sistema vivo a la raíz de prueba.
fn copiar_base(db: &Path) -> Result<(), String> {
    let origen = Path::new("/var/lib/pacman/sync");
    let destino = db.join("sync");
    std::fs::create_dir_all(&destino).map_err(|e| e.to_string())?;

    let entradas = std::fs::read_dir(origen)
        .map_err(|e| format!("no hay base de datos sincronizada en {}: {e}", origen.display()))?;
    let mut copiados = 0;
    for entrada in entradas.flatten() {
        if entrada.path().is_file() {
            std::fs::copy(entrada.path(), destino.join(entrada.file_name()))
                .map_err(|e| e.to_string())?;
            copiados += 1;
        }
    }

    if copiados == 0 {
        return Err("la base de datos de pacman está vacía".into());
    }
    Ok(())
}

/// Le pregunta a la base qué ocupa y con qué choca cada paquete.
fn describir(nombres: &[String], db: &Path) -> Result<Vec<Ficha>, String> {
    let salida = Command::new("pacman")
        .arg("-Si")
        .arg("--dbpath")
        .arg(db)
        .args(nombres)
        // El idioma se fija acá: los nombres de los campos están traducidos, y
        // el analizador lee los de inglés.
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("no se pudo ejecutar pacman -Si: {e}"))?;

    // Sin comprobar el estado a propósito: `pacman -Si` sale con error si **algún**
    // nombre no está en los repositorios, y aun así imprime todos los demás. Un
    // nombre que no aparece no puede chocar con nada, así que se sigue con lo que
    // sí describió.
    Ok(leer_fichas(&String::from_utf8_lossy(&salida.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ficha(nombre: &str, provee: &[&str], conflictos: &[&str]) -> Ficha {
        Ficha {
            nombre: nombre.into(),
            provee: provee.iter().map(|s| s.to_string()).collect(),
            conflictos: conflictos.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// La salida real de `LC_ALL=C pacman -Si jack2 pipewire-jack`, recortada a
    /// los campos que este módulo lee más uno que tiene que ignorar.
    const SALIDA: &str = "\
Repository      : extra
Name            : jack2
Version         : 1.9.22-2
Depends On      : libsamplerate  libdb  celt
Provides        : jack  libjack.so=0-64
Conflicts With  : jack
Replaces        : None

Repository      : extra
Name            : pipewire-jack
Version         : 1:1.6.8-1
Depends On      : pipewire
Provides        : jack  libjack.so=0-64
Conflicts With  : jack  jack2  pipewire-jack-client
Replaces        : None
";

    #[test]
    fn se_leen_los_dos_paquetes_con_lo_que_ocupan_y_con_lo_que_chocan() {
        let fichas = leer_fichas(SALIDA);

        assert_eq!(fichas.len(), 2);
        assert_eq!(fichas[0].nombre, "jack2");
        assert!(fichas[0].provee.contains("jack"));
        assert!(fichas[0].conflictos.contains("jack"));
        assert!(fichas[1].conflictos.contains("jack2"));
    }

    #[test]
    fn el_valor_none_no_es_un_nombre() {
        let fichas = leer_fichas(SALIDA);
        assert!(fichas.iter().all(|f| !f.conflictos.contains("None")));
    }

    #[test]
    fn los_campos_que_no_interesan_no_se_cuelan() {
        // `Depends On` viene justo antes de `Provides`, y una continuación mal
        // atribuida metería `celt` entre lo que el paquete ocupa.
        let fichas = leer_fichas(SALIDA);
        assert!(fichas[0].provee.iter().all(|p| p != "celt"));
        assert!(fichas[0].conflictos.iter().all(|c| c != "libsamplerate"));
    }

    #[test]
    fn una_lista_cortada_en_varias_lineas_se_lee_entera() {
        // pacman corta las listas largas al ancho de la terminal y sigue en la
        // línea de abajo, indentada. Sin esto se perdería justamente el
        // proveedor que rompe.
        let texto = "\
Name            : pipewire-jack
Conflicts With  : jack  jack2
                  pipewire-jack-client
Replaces        : None
";
        let fichas = leer_fichas(texto);
        assert!(fichas[0].conflictos.contains("pipewire-jack-client"));
    }

    #[test]
    fn el_mismo_paquete_en_dos_repositorios_se_lee_dos_veces_y_no_choca() {
        // `pacman -Si jack2` con el paquete en `extra` y en un repo de CachyOS
        // lo imprime dos veces seguidas. Chocaría consigo mismo por `jack`.
        let texto = format!("{}\n{}", SALIDA.split("\n\n").next().unwrap(), SALIDA.split("\n\n").next().unwrap());
        let fichas = leer_fichas(&texto);

        assert_eq!(fichas.len(), 2, "las dos entradas se leen");
        assert!(choques(&fichas).is_empty(), "pero es el mismo paquete");
    }

    #[test]
    fn el_choque_de_la_instalacion_que_se_rompio() {
        // El caso real: `jack2` entró porque ffmpeg pide `jack` y pacman eligió
        // el default; `pipewire-jack` entró porque el escritorio lo exige.
        let choques = choques(&leer_fichas(SALIDA));

        assert_eq!(choques.len(), 1);
        assert_eq!(choques[0].uno, "jack2");
        assert_eq!(choques[0].otro, "pipewire-jack");
        // Por `jack`, el nombre **virtual**, y no por `jack2` —que también
        // serviría para detectarlo, porque `pipewire-jack` lo nombra—. Se llega
        // primero por el virtual, y es el dato útil de los dos: dice qué
        // dependencia con varios proveedores hay que fijar.
        assert_eq!(choques[0].por, "jack");
    }

    #[test]
    fn se_detecta_aunque_lo_declare_solo_uno_de_los_dos() {
        // `jack2` no nombra a `pipewire-jack`: sólo choca con el virtual `jack`.
        // Mirando en una sola dirección, la mitad de los pares se escapan.
        let a = ficha("jack2", &["jack"], &["jack"]);
        let b = ficha("pipewire-jack", &["jack"], &[]);

        assert_eq!(choques(&[a, b]).len(), 1);
    }

    #[test]
    fn un_conjunto_sin_conflictos_no_informa_nada() {
        let fichas = vec![
            ficha("pipewire-jack", &["jack"], &["jack", "jack2"]),
            ficha("ffmpeg", &[], &[]),
            ficha("firefox", &[], &[]),
        ];

        assert!(choques(&fichas).is_empty());
    }

    #[test]
    fn un_conflicto_con_version_no_frena_la_instalacion() {
        // Un `conflicts=('foo<1.2')` suele estar satisfecho en un sistema nuevo.
        // Tratarlo como choque frenaría instalaciones legítimas, y este módulo
        // puede pasar algo por alto pero no puede inventar nada.
        let a = ficha("nuevo", &[], &["viejo<1.2"]);
        let b = ficha("viejo", &[], &[]);

        assert!(choques(&[a, b]).is_empty());
    }

    #[test]
    fn un_conflicto_sin_version_con_el_nombre_pelado_si() {
        let a = ficha("nuevo", &[], &["viejo"]);
        let b = ficha("viejo", &[], &[]);

        assert_eq!(choques(&[a, b]).len(), 1);
    }

    #[test]
    fn un_solo_choque_aunque_el_paquete_este_en_dos_repositorios() {
        // Lo encontró la prueba con datos reales: `jack2` está en `extra` y en
        // un repo de CachyOS, así que `pacman -Si` lo devuelve dos veces y el
        // mismo problema se informaba dos veces. Un aviso repetido hace dudar de
        // si son dos problemas distintos.
        let fichas = vec![
            ficha("jack2", &["jack"], &["jack"]),
            ficha("jack2", &["jack"], &["jack"]),
            ficha("pipewire-jack", &["jack"], &["jack", "jack2"]),
        ];

        assert_eq!(choques(&fichas).len(), 1);
    }

    #[test]
    fn cada_par_se_informa_una_sola_vez() {
        // Los dos se nombran mutuamente; dos avisos del mismo problema hacen
        // dudar de si son dos problemas.
        let a = ficha("uno", &[], &["otro"]);
        let b = ficha("otro", &[], &["uno"]);

        assert_eq!(choques(&[a, b]).len(), 1);
    }

    #[test]
    fn sin_paquetes_no_hay_nada_que_comprobar() {
        // Y no se arma ninguna raíz de prueba: el directorio no existe, así que
        // si intentara crearla, esto fallaría.
        let inexistente = Path::new("/no/existe/este/directorio");
        assert_eq!(revisar(&[], inexistente), Ok(Vec::new()));
    }

    #[test]
    fn el_choque_se_lee_como_una_frase() {
        let choque = Choque {
            uno: "jack2".into(),
            otro: "pipewire-jack".into(),
            por: "jack".into(),
        };

        assert_eq!(
            choque.to_string(),
            "jack2 y pipewire-jack no pueden convivir (por «jack»)"
        );
    }
}

