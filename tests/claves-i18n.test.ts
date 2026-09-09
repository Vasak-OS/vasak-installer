/**
 * Que toda clave de idioma que se usa exista en el catálogo.
 *
 * `t('disco.esquema')` con la clave mal escrita **no falla**: el plugin devuelve
 * la clave y la pantalla muestra `disco.esquema` donde iba un título. `vue-tsc`,
 * `biome` y `vite build` pasan los tres en verde, y el test de catálogos sólo
 * compara los idiomas entre sí — dos catálogos a los que les falta la misma
 * clave están perfectamente de acuerdo.
 *
 * Es el mismo tipo de fallo callado que el del componente sin importar, y se
 * cubre igual: leyendo el código, no montando nada.
 */

import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const IDIOMAS = ['es', 'en'];

/** Los archivos de código donde puede haber claves. */
function fuentes(dir: string): string[] {
	const salida: string[] = [];
	for (const entrada of readdirSync(dir)) {
		const ruta = join(dir, entrada);
		if (statSync(ruta).isDirectory()) salida.push(...fuentes(ruta));
		else if (entrada.endsWith('.vue') || entrada.endsWith('.ts')) salida.push(ruta);
	}
	return salida;
}

/**
 * Las claves de un `.yml`, aplanadas a `seccion.clave` o `a.b.c`.
 *
 * Se parsea a mano y no con una biblioteca de YAML: el catálogo es de forma
 * conocida —sangría de dos espacios, sin listas— y traer un parser al frontend
 * sólo para este test sería una dependencia por treinta líneas.
 *
 * Tres niveles porque `pasos:` los tiene: `pasos.bienvenida.titulo`. Con dos
 * niveles nada más, todas las claves de esa sección salían como faltantes.
 *
 * Sólo cuentan las hojas —las que tienen algo después de los dos puntos—: un
 * nivel intermedio como `pasos.bienvenida` no es una clave que se pueda pedir.
 */
function clavesDe(idioma: string): Set<string> {
	const texto = readFileSync(`src-tauri/locales/${idioma}.yml`, 'utf8');
	const claves = new Set<string>();
	const camino: string[] = [];
	for (const linea of texto.split('\n')) {
		const m = linea.match(/^( *)([a-zA-Z][a-zA-Z0-9]*):(.*)$/);
		if (!m) continue;
		const nivel = m[1].length / 2;
		camino.length = nivel;
		camino.push(m[2]);
		// Con valor en la misma línea es una hoja; sin él, abre una sección.
		// Un `>-` o un `|` también son hoja: el texto va en las de abajo.
		if (m[3].trim() !== '') claves.add(camino.join('.'));
	}
	return claves;
}

const claves = clavesDe('es');
const secciones = new Set([...claves].map((c) => c.split('.')[0]));
const archivos = [...fuentes('src')];

/** El `<script>` de un `.vue`, o el archivo entero si es un `.ts`. */
function guion(fuente: string, archivo: string): string {
	if (!archivo.endsWith('.vue')) return fuente;
	const m = fuente.match(/<script[^>]*>([\s\S]*?)<\/script>/);
	return m ? m[1] : '';
}

/**
 * Las claves que un archivo usa.
 *
 * Dos formas, porque el código usa las dos:
 *
 *   - `t('disco.titulo')`, que es la mayoría, y se busca en todo el archivo;
 *   - `'disco.titulo'` suelta, en las tablas de opciones y en los `computed`
 *     que eligen una clave según el estado — ahí el `t()` recibe una variable y
 *     la clave literal está en otro lado.
 *
 * La segunda se busca **sólo en el guion y sólo entre comillas simples**, que
 * es como las escribe el formateador. En la plantilla no: ahí cada atributo va
 * entre comillas dobles, así que `:key="disco.ruta"` y `v-if="disco.nvme"`
 * entraban como claves inexistentes. Y se pide además que el prefijo sea una
 * sección del catálogo, para que un `'algo.ts'` cualquiera no cuente.
 */
function clavesUsadas(fuente: string, archivo: string): string[] {
	const usadas = new Set<string>();
	for (const m of fuente.matchAll(/\bt\(\s*['"]([^'"]+)['"]/g)) usadas.add(m[1]);
	for (const m of guion(fuente, archivo).matchAll(
		/'([a-zA-Z][a-zA-Z0-9]*(?:\.[a-zA-Z][a-zA-Z0-9]*)+)'/g
	)) {
		if (secciones.has(m[1].split('.')[0])) usadas.add(m[1]);
	}
	return [...usadas];
}

describe('las claves de idioma', () => {
	test('hay catálogo y archivos que revisar', () => {
		expect(claves.size).toBeGreaterThan(100);
		expect(archivos.length).toBeGreaterThan(10);
	});

	test('todas las que se usan existen en el catálogo', () => {
		const faltantes: string[] = [];
		for (const archivo of archivos) {
			for (const clave of clavesUsadas(readFileSync(archivo, 'utf8'), archivo)) {
				if (!claves.has(clave)) faltantes.push(`${archivo}: ${clave}`);
			}
		}
		expect(faltantes).toEqual([]);
	});

	test('los dos idiomas tienen exactamente las mismas', () => {
		// El de Rust ya lo comprueba, pero acá se necesita igual: este test lee
		// sólo el catálogo español para saber qué existe, y sin esta condición
		// una clave que estuviera nada más que en español pasaría por buena.
		for (const idioma of IDIOMAS.filter((i) => i !== 'es')) {
			expect([...clavesDe(idioma)].sort()).toEqual([...claves].sort());
		}
	});
});
