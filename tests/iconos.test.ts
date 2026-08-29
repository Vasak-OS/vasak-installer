/**
 * Que cada icono que nombra el instalador exista de verdad en el tema.
 *
 * Es lo que se rompe callado: un nombre mal escrito no tira ningún error —el
 * plugin devuelve cadena vacía y el componente dibuja el hueco reservado—, así
 * que el resultado es un espacio en blanco donde tendría que haber un símbolo.
 * Y como la interfaz sigue funcionando, se descubre mirando una captura.
 *
 * Se busca contra el tema **instalado** y no contra el árbol de
 * `vasakos-icon-theme`, porque instalado es como lo va a ver el plugin: si un
 * icono existe en el repositorio pero el paquete no lo envía, acá tiene que
 * fallar igual.
 */

import { describe, expect, test } from 'bun:test';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { ICONO_PASO, todosLosIconos } from '../src/tools/iconos';
import { PASOS } from '../src/stores/instalacion';

/**
 * Dónde busca GTK, en el orden en que hereda el tema.
 *
 * `hicolor` va último porque es el respaldo de la especificación: un icono que
 * sólo esté ahí igual se resuelve, y por eso cuenta como encontrado.
 */
const TEMAS = [
	'/usr/share/icons/VasakOS-dark',
	'/usr/share/icons/VasakOS',
	'/usr/share/icons/VasakOS-light',
	'/usr/share/icons/Adwaita',
	'/usr/share/icons/hicolor',
];

/** Todos los nombres de archivo de icono que hay en los temas, sin extensión. */
function nombresDisponibles(): Set<string> {
	const encontrados = new Set<string>();

	const recorrer = (dir: string, profundidad: number) => {
		if (profundidad > 4) return;
		let entradas: string[];
		try {
			entradas = readdirSync(dir);
		} catch {
			return;
		}
		for (const entrada of entradas) {
			const ruta = join(dir, entrada);
			let esDirectorio = false;
			try {
				esDirectorio = statSync(ruta).isDirectory();
			} catch {
				// Un enlace simbólico roto: se saltea. El tema tiene cientos de
				// enlaces y uno colgado no invalida la comprobación.
				continue;
			}
			if (esDirectorio) {
				recorrer(ruta, profundidad + 1);
				continue;
			}
			const punto = entrada.indexOf('.');
			if (punto <= 0) continue;
			encontrados.add(entrada.slice(0, punto));
		}
	};

	for (const tema of TEMAS) {
		if (existsSync(tema)) recorrer(tema, 0);
	}
	return encontrados;
}

describe('los iconos que nombra el instalador', () => {
	const disponibles = nombresDisponibles();
	// Sin ningún tema instalado no hay nada contra qué comparar; el test se
	// saltea en lugar de fallar, que es lo que corresponde en una máquina de
	// integración sin escritorio.
	const hayTema = disponibles.size > 0;

	test('el tema está instalado y se pudo leer', () => {
		if (!hayTema) return;
		expect(disponibles.size).toBeGreaterThan(100);
	});

	test('todos existen en el tema', () => {
		if (!hayTema) return;

		// El plugin resuelve con `FORCE_SYMBOLIC`, así que `network-wireless`
		// llega al disco como `network-wireless-symbolic`. Se aceptan las dos
		// formas: un icono que sólo exista sin la variante simbólica igual se
		// dibuja.
		const faltantes = todosLosIconos().filter(
			(nombre) => !disponibles.has(`${nombre}-symbolic`) && !disponibles.has(nombre)
		);

		expect(faltantes).toEqual([]);
	});

	test('cada paso del asistente tiene su icono', () => {
		// Un paso sin entrada en el mapa deja el círculo de la barra lateral
		// vacío, y la barra pierde justamente lo que la hace legible de un
		// vistazo.
		for (const paso of PASOS) {
			expect(ICONO_PASO[paso]).toBeTruthy();
		}
	});

	test('ningún nombre lleva el sufijo -symbolic', () => {
		// Lo agrega el plugin. Escribirlo a mano da `foo-symbolic-symbolic`, que
		// no resuelve y deja el hueco vacío sin decir nada.
		for (const nombre of todosLosIconos()) {
			expect(nombre.endsWith('-symbolic')).toBe(false);
		}
	});

	/**
	 * Los iconos del catálogo de complementos también tienen que existir.
	 *
	 * Viven en un archivo de datos editable, así que no hay ningún compilador que
	 * los mire: sumar un navegador con un icono mal escrito deja un hueco en
	 * blanco al lado de su nombre y no falla nada.
	 *
	 * Se sacan con una expresión regular en vez de parsear el TOML: es un archivo
	 * nuestro con una forma conocida, y traer un parser al frontend sólo para
	 * este test sería una dependencia por un `grep`.
	 */
	test('los iconos del catálogo de complementos existen', () => {
		if (!hayTema) return;

		const toml = readFileSync('src-tauri/complementos.toml', 'utf8');
		const iconos = [...toml.matchAll(/^icono\s*=\s*"([^"]+)"/gm)].map((m) => m[1]);

		expect(iconos.length).toBeGreaterThan(5);
		const faltantes = iconos.filter(
			(nombre) => !disponibles.has(`${nombre}-symbolic`) && !disponibles.has(nombre)
		);
		expect(faltantes).toEqual([]);
	});

	test('ningún nombre lleva extensión ni ruta', () => {
		// El plugin acepta rutas absolutas —tiene un cerco para eso— así que un
		// nombre con `/` no falla, resuelve otra cosa. Y con `.svg` no resuelve
		// nada.
		for (const nombre of todosLosIconos()) {
			expect(nombre).not.toContain('/');
			expect(nombre).not.toContain('.');
		}
	});
});
