/**
 * Que cada icono que nombra el instalador exista de verdad, **en la variante que
 * se le pide**.
 *
 * Es lo que se rompe callado. Un nombre mal escrito no tira ningún error: el
 * plugin no lo encuentra, devuelve `image-missing` —el icono de imagen rota— y
 * la interfaz sigue funcionando. Se descubre mirando la pantalla.
 *
 * Y no alcanza con que el nombre exista: el tema tiene **dos versiones de casi
 * todo**, la de color y la simbólica, y el instalador pide una u otra según para
 * qué es el icono. Pedir a color un nombre que sólo existe en simbólico cae en
 * el mismo `image-missing`. Le pasó a «Desarrollo» con `builder-build`.
 *
 * Se busca contra el tema **instalado** y no contra el árbol de
 * `vasakos-icon-theme`, porque instalado es como lo va a ver el plugin: si un
 * icono existe en el repositorio pero el paquete no lo envía, acá tiene que
 * fallar igual.
 */

import { describe, expect, test } from 'bun:test';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { PASOS } from '../src/stores/instalacion';
import { ICONO_PASO, iconoDeDisco, todosLosIconos } from '../src/tools/iconos';

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

/**
 * Nombre de icono → todas las rutas donde aparece.
 *
 * Se guardan las rutas y no sólo los nombres porque **la carpeta y el sufijo son
 * lo que distingue las dos variantes**, y con un conjunto de nombres a secas no
 * se puede saber cuál de las dos se encontró.
 */
function inventario(): Map<string, string[]> {
	const encontrados = new Map<string, string[]>();

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
			const nombre = entrada.slice(0, punto);
			const previas = encontrados.get(nombre);
			if (previas) previas.push(ruta);
			else encontrados.set(nombre, [ruta]);
		}
	};

	for (const tema of TEMAS) {
		if (existsSync(tema)) recorrer(tema, 0);
	}
	return encontrados;
}

const RUTAS = inventario();

/**
 * Si una ruta es la variante simbólica.
 *
 * Se miran las dos señales, porque el tema usa las dos y ninguna sola alcanza:
 * `printer` tiene su simbólico en `devices/32/printer-symbolic.svg`, fuera de
 * toda carpeta `symbolic/`; y hay dieciséis archivos dentro de carpetas
 * `symbolic/` cuyo nombre no lleva el sufijo. Mirando sólo la carpeta se
 * rechazaría `printer` por no tener simbólico —que sí lo tiene—, y mirando sólo
 * el sufijo, esos dieciséis pasarían por iconos a color.
 */
function esSimbolica(ruta: string): boolean {
	const archivo = ruta.slice(ruta.lastIndexOf('/') + 1);
	const sinExtension = archivo.slice(0, archivo.indexOf('.'));
	return ruta.includes('/symbolic/') || sinExtension.endsWith('-symbolic');
}

/** Si el nombre tiene versión **a color**, que es lo que pide `tipo="icono"`. */
function hayVersionAColor(nombre: string): boolean {
	return (RUTAS.get(nombre) ?? []).some((ruta) => !esSimbolica(ruta));
}

/** Si el nombre tiene versión **simbólica**, que es lo que pide `tipo="simbolo"`. */
function hayVersionSimbolica(nombre: string): boolean {
	if ((RUTAS.get(`${nombre}-symbolic`) ?? []).length > 0) return true;
	// Un archivo con el nombre pelado adentro de una carpeta `symbolic/`.
	return (RUTAS.get(nombre) ?? []).some(esSimbolica);
}

/** Si el nombre existe en alguna de las dos variantes. */
function existe(nombre: string): boolean {
	return hayVersionAColor(nombre) || hayVersionSimbolica(nombre);
}

describe('los iconos que nombra el instalador', () => {
	// Sin ningún tema instalado no hay nada contra qué comparar; los tests se
	// saltean en lugar de fallar, que es lo que corresponde en una máquina de
	// integración sin escritorio.
	const hayTema = RUTAS.size > 0;

	test('el tema está instalado y se pudo leer', () => {
		if (!hayTema) return;
		expect(RUTAS.size).toBeGreaterThan(100);
	});

	test('todos existen en el tema', () => {
		if (!hayTema) return;
		expect(todosLosIconos().filter((nombre) => !existe(nombre))).toEqual([]);
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
	 * Los iconos del catálogo de complementos existen **a color**.
	 *
	 * Los complementos se dibujan con `tipo="icono"`. Pedir a color un nombre que
	 * sólo existe en simbólico **no falla**: el plugin devuelve `image-missing`,
	 * o sea que al lado de «Desarrollo» aparece el icono de imagen rota. Y el
	 * catálogo es un archivo de datos editable que ningún compilador mira.
	 *
	 * Los nombres se sacan con una expresión regular en vez de parsear el TOML:
	 * es un archivo nuestro con una forma conocida, y traer un parser al frontend
	 * sólo para este test sería una dependencia por un `grep`.
	 */
	test('los iconos del catálogo de complementos existen a color', () => {
		if (!hayTema) return;

		const toml = readFileSync('src-tauri/complementos.toml', 'utf8');
		const iconos = [...toml.matchAll(/^icono\s*=\s*"([^"]+)"/gm)].map((m) => m[1]);
		expect(iconos.length).toBeGreaterThan(5);

		expect(iconos.filter((nombre) => !hayVersionAColor(nombre))).toEqual([]);
	});

	/**
	 * Los navegadores tienen que dibujarse con su logo de verdad.
	 *
	 * Es el caso que motivó separar los dos tipos: en glifo monocromo Firefox,
	 * Chromium y Brave son tres contornos que nadie distingue, y elegir navegador
	 * mirando tres contornos iguales no es elegir.
	 */
	test('los navegadores tienen su icono de aplicación a color', () => {
		if (!hayTema) return;
		for (const nombre of ['firefox', 'chromium', 'brave-browser']) {
			expect(hayVersionAColor(nombre)).toBe(true);
		}
	});

	/**
	 * Los que se dibujan como símbolo tienen que existir como símbolo.
	 *
	 * Al revés que el caso de arriba: un nombre que sólo existe a color, pedido
	 * como símbolo, también cae en `image-missing`.
	 */
	test('los iconos de los pasos existen en versión simbólica', () => {
		if (!hayTema) return;
		const sinSimbolo = Object.values(ICONO_PASO).filter((n) => !hayVersionSimbolica(n));
		expect(sinSimbolo).toEqual([]);
	});

	/**
	 * Cada tipo de disco usa **exactamente** el icono que le corresponde.
	 *
	 * Es el error que hubo: el NVMe se dibujaba con `media-flash`, que en este
	 * tema es un enlace a `gnome-dev-media-sdmmc` —una tarjeta SD—, y el SSD con
	 * `drive-multidisk`, una pila de discos de RAID. En la pantalla donde se
	 * elige qué disco formatear, un icono que miente sobre qué dispositivo es
	 * resulta peor que uno repetido.
	 *
	 * Se comparan nombres exactos y no un prefijo: con `startsWith` pasaría
	 * `drive-harddisk-nvme`, que no existe en el tema y volvería a dar el icono
	 * de imagen rota.
	 */
	test('cada tipo de disco usa el icono que le corresponde', () => {
		expect(iconoDeDisco({ nvme: false, rotacional: true })).toBe('drive-harddisk');
		expect(iconoDeDisco({ nvme: true, rotacional: false })).toBe('drive-harddisk-solidstate');
		expect(iconoDeDisco({ nvme: false, rotacional: false })).toBe('drive-harddisk-solidstate');

		// Y los dos nombres tienen que existir a color de verdad, que es como se
		// dibujan las tarjetas de disco.
		if (!hayTema) return;
		expect(hayVersionAColor('drive-harddisk')).toBe(true);
		expect(hayVersionAColor('drive-harddisk-solidstate')).toBe(true);
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
