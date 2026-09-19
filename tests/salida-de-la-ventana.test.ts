/**
 * La ventana del instalador no tiene botón de cerrar, y por eso tiene «salir».
 *
 * Minimizar o cerrar mientras el ayudante está escribiendo el disco deja el
 * equipo a medio instalar, y el botón de la barra no distingue en qué paso
 * está. Así que la ventana va sin los tres botones.
 *
 * Eso sólo es defendible si hay otra salida: sin ella sería una ventana de la
 * que no se puede salir, que es peor que el problema que se estaba evitando. El
 * «cancelar» que ya existía vive **dentro** de la pantalla de instalación y no
 * alcanza a los pasos anteriores.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { WindowControls, WindowFrame } from '@vasakgroup/vue-libvasak';
import { mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import App from '@/App.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { olvidarTodo, pedidos } from './dobles';

let vista: VueWrapper | null = null;

/**
 * La ventana, con el paso de turno reemplazado por un cartel vacío.
 *
 * Lo que se prueba acá es el armazón de la ventana, no lo que hay adentro: cada
 * paso pide sus propios datos al ayudante y montarlos de verdad sería doblar
 * media docena de comandos para mirar un botón de la barra.
 */
function abrir() {
	vista = mount(App, {
		global: { stubs: { BienvenidaView: { template: '<div class="paso" />' } } },
	});
	return vista;
}

/** El botón que abre la confirmación de salida. */
function botonDeSalida(ventana: VueWrapper) {
	return ventana.find('[aria-label="ventana.salir"]');
}

beforeEach(() => {
	setActivePinia(createPinia());
});

afterEach(() => {
	vista?.unmount();
	vista = null;
	olvidarTodo();
});

describe('los botones de la ventana', () => {
	test('no hay ninguno', () => {
		// Ni minimizar, ni maximizar, ni cerrar.
		const ventana = abrir();

		expect(ventana.findComponent(WindowFrame).props('controls')).toEqual([]);
		expect(ventana.findComponent(WindowControls).findAll('button').length).toBe(0);
	});
});

describe('la salida', () => {
	test('está en la barra, así que alcanza a todos los pasos', () => {
		// El «cancelar» que ya existía vive dentro de la pantalla de
		// instalación: en bienvenida, disco o cuenta no hay ninguno.
		const ventana = abrir();

		expect(botonDeSalida(ventana).exists()).toBe(true);
	});

	test('pregunta antes de salir, no cierra de una', async () => {
		const ventana = abrir();

		await botonDeSalida(ventana).trigger('click');

		expect(ventana.text()).toContain('ventana.salirTitulo');
		// Y no le pidió nada a la ventana todavía.
		expect(pedidos('plugin:window|close').length).toBe(0);
	});

	test('antes de tocar el disco dice que no se pierde nada del equipo', async () => {
		const ventana = abrir();

		await botonDeSalida(ventana).trigger('click');

		expect(ventana.text()).toContain('ventana.salirTexto');
		expect(ventana.text()).not.toContain('ventana.salirInstalandoTexto');
	});

	test('y una vez empezada dice qué queda en el disco', async () => {
		// Un cartel que sólo diga «¿salir?» hace creer que se vuelve al estado
		// anterior, y el disco ya no tiene ni el sistema nuevo ni lo que había.
		const ventana = abrir();
		useInstalacionStore().navegacionBloqueada = true;
		await ventana.vm.$nextTick();

		await botonDeSalida(ventana).trigger('click');

		expect(ventana.text()).toContain('ventana.salirInstalandoTexto');
	});

	test('volver deja la ventana como estaba', async () => {
		const ventana = abrir();
		await botonDeSalida(ventana).trigger('click');

		await ventana.find('[data-prueba="volver"]').trigger('click');

		expect(ventana.text()).not.toContain('ventana.salirTitulo');
		expect(pedidos('plugin:window|close').length).toBe(0);
	});

	test('confirmar cierra la ventana', async () => {
		const ventana = abrir();
		await botonDeSalida(ventana).trigger('click');

		await ventana.find('[data-prueba="salir"]').trigger('click');
		await ventana.vm.$nextTick();

		expect(pedidos('plugin:window|close').length).toBe(1);
	});

	test('y si ya empezó, primero se lo dice al ayudante', async () => {
		// Cerrar la ventana no detiene el proceso que está escribiendo el
		// disco: si no se le avisa, sigue trabajando sobre un equipo cuya
		// interfaz ya no está.
		const ventana = abrir();
		useInstalacionStore().navegacionBloqueada = true;
		await ventana.vm.$nextTick();
		await botonDeSalida(ventana).trigger('click');

		await ventana.find('[data-prueba="salir"]').trigger('click');
		await ventana.vm.$nextTick();

		expect(pedidos('cancelar_instalacion').length).toBe(1);
		expect(pedidos('plugin:window|close').length).toBe(1);
	});
});
