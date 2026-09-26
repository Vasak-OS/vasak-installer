/**
 * Qué botones lleva la ventana del instalador, y cuándo.
 *
 * Los tres, salvo **cerrar mientras el ayudante está escribiendo el disco**:
 * ahí cerrar deja el equipo a medio instalar, sin el sistema nuevo y sin lo que
 * había antes. Minimizar y maximizar se quedan siempre, que es lo que evita el
 * problema contrario: perder la ventana de vista y no poder traerla de vuelta.
 *
 * Y cuando la instalación termina, falla o se cancela, cerrar vuelve: ahí ya no
 * hay nada escribiendo el disco.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { WindowControls } from '@vasakgroup/vue-libvasak';
import { mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import App from '@/App.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { olvidarTodo, pedidos } from './dobles';

let vista: VueWrapper | null = null;

/**
 * La ventana, con el paso de turno reemplazado por un cartel vacío.
 *
 * Lo que se prueba acá es el armazón, no lo que hay adentro: cada paso pide sus
 * propios datos al ayudante y montarlos de verdad sería doblar media docena de
 * comandos para mirar los botones de la barra.
 */
function abrir() {
	vista = mount(App, {
		global: { stubs: { BienvenidaView: { template: '<div class="paso" />' } } },
	});
	return vista;
}

/** Los nombres accesibles de los botones de ventana, en orden. */
function botones(ventana: VueWrapper) {
	return ventana
		.findComponent(WindowControls)
		.findAll('button')
		.map((boton) => boton.attributes('aria-label'));
}

/** Deja la ventana como cuando el ayudante ya arrancó. */
async function empezarAInstalar(ventana: VueWrapper) {
	useInstalacionStore().navegacionBloqueada = true;
	await ventana.vm.$nextTick();
}

beforeEach(() => setActivePinia(createPinia()));

afterEach(() => {
	vista?.unmount();
	vista = null;
	olvidarTodo();
});

describe('antes de tocar el disco', () => {
	test('van los tres, con su nombre traducido', () => {
		// Todavía no hay nada escrito, así que cerrar no pierde nada del equipo.
		expect(botones(abrir())).toEqual(['ventana.minimizar', 'ventana.maximizar', 'ventana.cerrar']);
	});
});

describe('mientras el ayudante escribe el disco', () => {
	test('cerrar no está', async () => {
		const ventana = abrir();

		await empezarAInstalar(ventana);

		expect(botones(ventana)).toEqual(['ventana.minimizar', 'ventana.maximizar']);
	});

	test('pero minimizar y maximizar sí', async () => {
		// Sacar los tres sería el problema contrario: la ventana se pierde de
		// vista detrás de otra y no hay forma de traerla.
		const ventana = abrir();

		await empezarAInstalar(ventana);

		expect(botones(ventana)).toContain('ventana.minimizar');
		expect(botones(ventana)).toContain('ventana.maximizar');
	});

	test('y nada le pide a la ventana que se cierre', async () => {
		// Lo que importa no es que falte el botón sino que no haya forma: un
		// `close()` colgado de una tecla sería el mismo agujero con otra cara.
		const ventana = abrir();
		await empezarAInstalar(ventana);

		for (const boton of ventana.findAll('button')) await boton.trigger('click');

		expect(pedidos('plugin:window|close')).toHaveLength(0);
	});
});

describe('cuando la instalación deja de correr', () => {
	test('al terminar bien, cerrar vuelve', async () => {
		const ventana = abrir();
		await empezarAInstalar(ventana);

		useInstalacionStore().terminada = true;
		await ventana.vm.$nextTick();

		expect(botones(ventana)).toContain('ventana.cerrar');
	});

	test('y al fallar también', async () => {
		// Es cuando más falta hace: la pantalla dice qué pasó y hay que poder
		// irse. También es el estado en el que queda al cancelar, porque el
		// ayudante contesta el fin con un error.
		const ventana = abrir();
		await empezarAInstalar(ventana);

		useInstalacionStore().fallo = 'el disco se desenchufó';
		await ventana.vm.$nextTick();

		expect(botones(ventana)).toContain('ventana.cerrar');
	});
});
