/**
 * La barra de pasos, montada.
 *
 * El marco pasó a ser el de `@vasakgroup/vue-libvasak`, el mismo que usan las
 * otras cuatro ventanas del escritorio. Lo que se comprueba acá es lo del
 * instalador —que los diez pasos estén, que el estado de cada uno sea el que
 * corresponde y que no se pueda volver una vez que se empezó a escribir en el
 * disco—; cómo se pliega y cómo se ve el marco es de la librería y se prueba
 * allá.
 *
 * Un paso no es un botón de la librería a propósito: tiene número, estado,
 * descripción y un emblema de terminado, y ninguno de los cuatro lo tiene
 * `SideButton`.
 */

import { beforeEach, describe, expect, test } from 'bun:test';
import { SideBar, SideButton } from '@vasakgroup/vue-libvasak';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import PasoBoton from '@/components/sidebar/PasoBoton.vue';
import PasosSidebar from '@/components/sidebar/PasosSidebar.vue';
import { PASOS, type Paso } from '@/stores/instalacion';
import { olvidarTodo } from './dobles';

function montarLaBarra(actual: Paso = 'teclado', navegable = true) {
	return mount(PasosSidebar, { props: { actual, navegable } });
}

/** El paso cuyo título es ése. La clave, que es lo que devuelve el `t()` doble. */
function pasoDe(vista: ReturnType<typeof montarLaBarra>, paso: Paso) {
	return vista
		.findAllComponents(PasoBoton)
		.find((boton) => boton.props('titulo') === `pasos.${paso}.titulo`);
}

beforeEach(() => {
	olvidarTodo();
});

describe('la barra', () => {
	test('es la compartida y no un `nav` escrito acá', () => {
		// El instalador es la primera ventana que alguien ve del sistema: que se
		// lea como parte del escritorio importa acá más que en ninguna otra.
		const vista = montarLaBarra();

		expect(vista.findComponent(SideBar).exists()).toBe(true);
		expect(vista.findAll('nav')).toHaveLength(0);
	});

	test('sin área de título, que el nombre ya está en la barra de arriba', () => {
		const vista = montarLaBarra();

		const barra = vista.findComponent(SideBar);
		expect(barra.find('header').exists()).toBe(false);
		// Sin cabecera el botón de plegar necesita su propio lugar, o la barra
		// deja de poder plegarse.
		expect(barra.find('button[aria-label="barraLateral.plegar"]').exists()).toBe(true);
	});

	test('los pasos no son botones de la librería', () => {
		// Un paso tiene número, estado, descripción y emblema de terminado.
		// `SideButton` no tiene ninguno de los cuatro: usarlo habría sido
		// perder la mitad de lo que la barra dice.
		const vista = montarLaBarra();

		expect(vista.findAllComponents(SideButton)).toHaveLength(0);
		expect(vista.findAllComponents(PasoBoton)).toHaveLength(PASOS.length);
	});

	test('siguen siendo una lista ordenada con su nombre', () => {
		// Para un lector de pantalla esto es «lista de 10 elementos, elemento
		// 4», que es la misma información que la barra le da a quien la ve.
		const vista = montarLaBarra();

		const lista = vista.find('ol');
		expect(lista.exists()).toBe(true);
		expect(lista.attributes('aria-label')).toBeDefined();
		expect(lista.findAll('li')).toHaveLength(PASOS.length);
	});
});

describe('el estado de cada paso', () => {
	test('los de antes están hechos, el de ahora es el actual y los de después esperan', () => {
		const vista = montarLaBarra('teclado');

		expect(pasoDe(vista, 'bienvenida')?.props('estado')).toBe('hecho');
		expect(pasoDe(vista, 'teclado')?.props('estado')).toBe('actual');
		expect(pasoDe(vista, 'disco')?.props('estado')).toBe('pendiente');
	});

	test('el actual se marca para quien no ve el color', () => {
		// Es `step` y no `page`: esto es un asistente, no una navegación.
		const vista = montarLaBarra('teclado');

		expect(pasoDe(vista, 'teclado')?.find('button').attributes('aria-current')).toBe('step');
		expect(pasoDe(vista, 'disco')?.find('button').attributes('aria-current')).toBeUndefined();
	});
});

describe('volver a un paso', () => {
	test('se puede a uno anterior, y avisa a quién', async () => {
		const vista = montarLaBarra('teclado');

		await pasoDe(vista, 'region')?.find('button').trigger('click');

		expect(vista.emitted('ir')?.[0]).toEqual(['region']);
	});

	test('no se puede a uno que todavía no llegó', () => {
		// Saltear pasos deja la instalación sin la mitad de lo que necesita.
		const vista = montarLaBarra('teclado');

		expect(pasoDe(vista, 'disco')?.find('button').attributes('disabled')).toBeDefined();
	});

	test('ni a ninguno una vez que se empezó a escribir en el disco', async () => {
		// Lo que ya se escribió no se deshace volviendo a la pantalla anterior:
		// dejar el botón vivo sería ofrecer algo que no existe.
		const vista = montarLaBarra('instalacion', false);

		for (const paso of PASOS) {
			const boton = pasoDe(vista, paso);
			if (boton) expect(boton.find('button').attributes('disabled')).toBeDefined();
		}
	});
});

describe('plegada', () => {
	test('cada paso conserva su nombre para el puntero y el lector de pantalla', async () => {
		// 84 píxeles es el ancho del icono: el título y la descripción no
		// entran. Sin el `title` y el `aria-label`, plegada la barra es una
		// columna de dibujos sin explicación y diez botones sin nombre.
		const vista = montarLaBarra('teclado');
		await vista.findComponent(SideBar).find('button[aria-label]').trigger('click');
		await nextTick();

		const paso = pasoDe(vista, 'teclado')?.find('button');
		expect(paso?.attributes('title')).toContain('pasos.teclado.titulo');
		expect(paso?.attributes('aria-label')).toContain('pasos.teclado.titulo');
		// Y el número adelante, que es lo que dice en qué punto de los diez está.
		expect(paso?.attributes('title')).toContain('4.');
	});
});

describe('la hoja de estilos', () => {
	test('escanea la librería, o la barra llega sin ninguna de sus reglas', async () => {
		// Tailwind v4 no mira dentro de `node_modules`. Sin esta línea, las
		// clases que sólo existen en los componentes de la librería no entran
		// nunca en la hoja: la ventana abre con el marcado puesto y sin
		// paddings, sin anchos y con los iconos a tamaño natural. Pasó.
		const css = await Bun.file(new URL('../src/assets/main.css', import.meta.url)).text();

		expect(css).toContain('@source');
		expect(css).toContain('@vasakgroup/vue-libvasak');
	});
});
