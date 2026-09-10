/**
 * Que los argumentos de cada `invoke` coincidan con los del comando.
 *
 * Tauri deserializa los argumentos de un `#[tauri::command]` desde el objeto
 * que se le pasa. Uno de menos **no** es un valor por omisión: es un error de
 * deserialización que rechaza la llamada entera.
 *
 * Pasó de verdad, y es el motivo de que este archivo exista: al sumarle
 * `asignaciones` a `vista_previa_particionado` se cambió la firma en Rust y no
 * la llamada en TypeScript. `cargo test`, `cargo clippy`, `vue-tsc` y `biome`
 * pasaron los cinco en verde, y los 106 tests del frontend también — porque
 * ninguno cruza el borde del `invoke`. La vista previa del particionado quedó
 * rota para **todos** los esquemas, que es la pantalla anterior al punto sin
 * retorno.
 *
 * Se compara leyendo el código, sin ejecutar nada: los nombres van en
 * `snake_case` en Rust y en `camelCase` en la llamada, que es la conversión que
 * hace Tauri.
 */

import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

/** `particion_destino` → `particionDestino`. */
function aCamello(nombre: string): string {
	return nombre.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

/**
 * Los comandos de Rust, con sus argumentos.
 *
 * Se leen del código y no de una lista escrita a mano, que es lo que se
 * desactualiza. Los argumentos que Tauri inyecta —`app`, `window`, `state`, y
 * cualquiera que sea un `AppHandle` o similar— no vienen del frontend, así que
 * se descartan por tipo.
 */
function comandosDeRust(): Map<string, string[]> {
	const fuente = readFileSync('src-tauri/src/commands.rs', 'utf8');
	const comandos = new Map<string, string[]>();

	for (const m of fuente.matchAll(
		/#\[tauri::command\][\s\S]*?fn\s+(\w+)\s*\(([\s\S]*?)\)\s*(?:->|\{)/g
	)) {
		const [, nombre, firma] = m;
		const args: string[] = [];
		// Un argumento por línea, que es como los formatea rustfmt.
		for (const linea of firma.split('\n')) {
			const a = linea.match(/^\s*(\w+)\s*:\s*(.+?),?\s*$/);
			if (!a) continue;
			const [, arg, tipo] = a;
			// Los que inyecta Tauri, no el frontend.
			if (/AppHandle|Window|State|Runtime/.test(tipo)) continue;
			args.push(arg);
		}
		comandos.set(nombre, args);
	}
	return comandos;
}

/**
 * Las claves del primer nivel de un literal de objeto.
 *
 * Se recorre contando anidamiento en vez de buscar `^\t+clave:`, que fue la
 * primera versión: con eso, un objeto escrito en una sola línea
 * —`{ nombre: x }`, que es la mitad de las llamadas— no daba ninguna clave y el
 * test acusaba cinco comandos que estaban perfectos. Un test que da falsos
 * positivos se termina ignorando, que es peor que no tenerlo.
 */
function clavesDe(cuerpo: string): string[] {
	const claves: string[] = [];
	let nivel = 0;
	let comilla: string | null = null;
	let token = '';

	for (let i = 0; i < cuerpo.length; i++) {
		const c = cuerpo[i];

		// Los comentarios se saltean enteros. Sin esto, un `:` adentro de uno
		// —«Tauri deserializa: …»— corta el token y la clave que venía después
		// se pierde. Fue exactamente lo que pasó con `asignaciones`, y el test
		// acusaba de faltante una clave que estaba tres líneas más abajo.
		if (!comilla && c === '/' && cuerpo[i + 1] === '/') {
			const fin = cuerpo.indexOf('\n', i);
			i = fin === -1 ? cuerpo.length : fin;
			token = '';
			continue;
		}
		if (!comilla && c === '/' && cuerpo[i + 1] === '*') {
			const fin = cuerpo.indexOf('*/', i + 2);
			i = fin === -1 ? cuerpo.length : fin + 1;
			token = '';
			continue;
		}

		if (comilla) {
			if (c === comilla && cuerpo[i - 1] !== '\\') comilla = null;
			continue;
		}
		if (c === "'" || c === '"' || c === '`') {
			comilla = c;
			continue;
		}
		if ('{[('.includes(c)) {
			nivel++;
			continue;
		}
		if ('}])'.includes(c)) {
			nivel--;
			continue;
		}
		if (nivel > 0) continue;

		if (c === ':') {
			const clave = token.trim();
			if (/^\w+$/.test(clave)) claves.push(clave);
			token = '';
		} else if (c === ',') {
			token = '';
		} else {
			token += c;
		}
	}
	return claves;
}

/** Los archivos donde puede haber un `invoke`. */
function fuentes(dir: string): string[] {
	const salida: string[] = [];
	for (const entrada of readdirSync(dir)) {
		const ruta = join(dir, entrada);
		if (statSync(ruta).isDirectory()) salida.push(...fuentes(ruta));
		else if (entrada.endsWith('.vue') || entrada.endsWith('.ts')) salida.push(ruta);
	}
	return salida;
}

/** Cada `invoke('nombre', { ... })`, con las claves que le pasa. */
function llamadas(): { archivo: string; comando: string; claves: string[] }[] {
	const salida: { archivo: string; comando: string; claves: string[] }[] = [];
	for (const archivo of fuentes('src')) {
		const texto = readFileSync(archivo, 'utf8');
		for (const m of texto.matchAll(/invoke(?:<[^>]*>)?\(\s*'([^']+)'\s*(,)?/g)) {
			const [, comando, hayArgs] = m;
			if (!hayArgs) {
				salida.push({ archivo, comando, claves: [] });
				continue;
			}
			// El objeto que sigue, contando llaves para encontrar su final.
			const desde = texto.indexOf('{', m.index + m[0].length);
			if (desde === -1) {
				salida.push({ archivo, comando, claves: [] });
				continue;
			}
			let nivel = 0;
			let hasta = desde;
			for (; hasta < texto.length; hasta++) {
				if (texto[hasta] === '{') nivel++;
				else if (texto[hasta] === '}' && --nivel === 0) break;
			}
			salida.push({ archivo, comando, claves: clavesDe(texto.slice(desde + 1, hasta)) });
		}
	}
	return salida;
}

const comandos = comandosDeRust();
const invocaciones = llamadas();

describe('los comandos', () => {
	test('hay comandos y llamadas que revisar', () => {
		expect(comandos.size).toBeGreaterThan(5);
		expect(invocaciones.length).toBeGreaterThan(5);
	});

	test('toda llamada nombra un comando que existe', () => {
		const faltantes = invocaciones
			.filter((i) => !comandos.has(i.comando))
			.map((i) => `${i.archivo}: ${i.comando}`);
		expect(faltantes).toEqual([]);
	});

	test('toda llamada pasa todos los argumentos del comando', () => {
		const problemas: string[] = [];
		for (const { archivo, comando, claves } of invocaciones) {
			const esperados = comandos.get(comando);
			if (!esperados) continue;
			for (const arg of esperados) {
				if (!claves.includes(aCamello(arg)) && !claves.includes(arg)) {
					problemas.push(`${archivo}: ${comando} no recibe «${aCamello(arg)}»`);
				}
			}
		}
		expect(problemas).toEqual([]);
	});

	test('ninguna llamada pasa argumentos que el comando no tiene', () => {
		// Al revés: sobran silenciosamente, y suelen ser el renombre a medias
		// de un argumento — el viejo se manda y el nuevo no llega.
		const problemas: string[] = [];
		for (const { archivo, comando, claves } of invocaciones) {
			const esperados = comandos.get(comando)?.map(aCamello);
			if (!esperados) continue;
			for (const clave of claves) {
				if (!esperados.includes(clave)) {
					problemas.push(`${archivo}: ${comando} no tiene «${clave}»`);
				}
			}
		}
		expect(problemas).toEqual([]);
	});
});
