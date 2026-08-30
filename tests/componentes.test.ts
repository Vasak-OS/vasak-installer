/**
 * Que todo componente usado en una plantilla esté importado.
 *
 * Vue trata una etiqueta desconocida como un elemento personalizado del
 * navegador: no falla, no avisa en la consola de producción, y **no dibuja
 * nada**. `vue-tsc`, `biome` y `vite build` pasan los tres en verde.
 *
 * Pasó de verdad: al renombrar `Icono` a `IconoSistema`, un uso escrito en
 * varias líneas —`<Icono\n  :nombre=…`— no coincidió con el reemplazo y quedó
 * sin renombrar. Era la marca de terminado y de fallado de la lista de pasos de
 * la instalación: el sitio donde alguien mira si algo salió mal. Las tres
 * herramientas dieron el visto bueno y el emblema simplemente no aparecía.
 *
 * Esto no reemplaza a un linter de plantillas; cubre exactamente el caso que se
 * escapó, que es un componente en PascalCase usado sin importar.
 */

import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

/** Todos los `.vue` del proyecto. */
function archivosVue(dir: string): string[] {
	const salida: string[] = [];
	for (const entrada of readdirSync(dir)) {
		const ruta = join(dir, entrada);
		if (statSync(ruta).isDirectory()) {
			salida.push(...archivosVue(ruta));
		} else if (entrada.endsWith('.vue')) {
			salida.push(ruta);
		}
	}
	return salida;
}

/**
 * Los componentes que Vue resuelve solo y no hace falta importar.
 *
 * `component` es el dinámico —`<component :is="…">`— y los otros son los que
 * trae el propio Vue.
 */
const INTEGRADOS = new Set([
	'component',
	'Transition',
	'TransitionGroup',
	'KeepAlive',
	'Teleport',
	'Suspense',
]);

/**
 * Las etiquetas en PascalCase que usa una plantilla.
 *
 * Sólo PascalCase: es la convención del proyecto para los componentes, y así no
 * hay que distinguir un `<div>` de un elemento personalizado de verdad.
 */
function componentesUsados(contenido: string): Set<string> {
	const plantilla = contenido
		.slice(contenido.indexOf('<template'))
		// Los comentarios se sacan antes de buscar. Varios explican el componente
		// nombrándolo —«sin el `slot`, `<WindowAppLayout>…</WindowAppLayout>`
		// descartaba lo que se le pusiera dentro»— y sin esto cada uno de esos
		// comentarios se cuenta como un uso que nadie importó.
		.replace(/<!--[\s\S]*?-->/g, '');

	const usos = new Set<string>();
	for (const [, nombre] of plantilla.matchAll(/<([A-Z][A-Za-z0-9]*)[\s/>]/g)) {
		if (!INTEGRADOS.has(nombre)) usos.add(nombre);
	}
	return usos;
}

/** Los nombres que el `<script setup>` trae con un `import`. */
function componentesImportados(contenido: string): Set<string> {
	const script = contenido.slice(0, contenido.indexOf('<template'));
	const nombres = new Set<string>();
	for (const [, nombre] of script.matchAll(/^\s*import\s+([A-Z][A-Za-z0-9]*)\s+from/gm)) {
		nombres.add(nombre);
	}
	// Los que llegan por `defineAsyncComponent` o por un objeto de vistas, como
	// el mapa de `App.vue`, se usan con `<component :is>` y ya están exentos.
	return nombres;
}

describe('las plantillas', () => {
	const archivos = archivosVue('src');

	test('hay componentes que revisar', () => {
		expect(archivos.length).toBeGreaterThan(5);
	});

	test('todo componente usado está importado', () => {
		const faltantes: string[] = [];

		for (const archivo of archivos) {
			const contenido = readFileSync(archivo, 'utf8');
			if (!contenido.includes('<template')) continue;

			const importados = componentesImportados(contenido);
			for (const usado of componentesUsados(contenido)) {
				if (!importados.has(usado)) {
					faltantes.push(`${archivo}: <${usado}> se usa y no se importa`);
				}
			}
		}

		expect(faltantes).toEqual([]);
	});
});
