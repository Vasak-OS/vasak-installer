/**
 * La barra de pasos, montada.
 *
 * El marco es el de `@vasakgroup/vue-libvasak` y, desde vue-libvasak#74, cada
 * paso también: `SideButton` con la descripción, la insignia con el número y la
 * ranura del icono, donde va un `IconTile` con el emblema de terminado. Lo que
 * se comprueba acá es lo del instalador —que los diez pasos estén, que el
 * estado de cada uno sea el que corresponde y que no se pueda volver una vez
 * que se empezó a escribir en el disco—; cómo se pliega y cómo se ve el botón
 * es de la librería y se prueba allá.
 *
 * Y la ventana angosta: por debajo de 30rem va una columna por vez, como en un
 * celular. `happy-dom` no evalúa consultas de contenedor, así que lo que se
 * prueba es lo que sí decide el código —qué se muestra con qué clase y qué
 * hace cada botón—; el resultado dibujado está en las capturas del banco.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { IconTile, SideBar, SideButton } from '@vasakgroup/vue-libvasak';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import App from '@/App.vue';
import StepList from '@/components/sidebar/StepList.vue';
import StepsSidebar from '@/components/sidebar/StepsSidebar.vue';
import { PASOS, type Paso, useInstalacionStore } from '@/stores/instalacion';
import { olvidarTodo } from './dobles';

function mountSidebar(current: Paso = 'teclado', navigable = true) {
	return mount(StepsSidebar, { props: { current, navigable } });
}

/** El botón del paso cuyo título es ése. La clave, que es lo que devuelve el `t()` doble. */
function stepButton(view: VueWrapper, step: Paso) {
	return view
		.findAllComponents(SideButton)
		.find((button) => button.props('label') === `pasos.${step}.titulo`);
}

function stateOf(view: VueWrapper, step: Paso) {
	return view.find(`li[data-step="${step}"]`).attributes('data-state');
}

beforeEach(() => {
	olvidarTodo();
});

describe('la barra', () => {
	test('es la compartida y no un `nav` escrito acá', () => {
		// El instalador es la primera ventana que alguien ve del sistema: que se
		// lea como parte del escritorio importa acá más que en ninguna otra.
		const view = mountSidebar();

		expect(view.findComponent(SideBar).exists()).toBe(true);
		expect(view.findAll('nav')).toHaveLength(0);
	});

	test('sin área de título, que el nombre ya está en la barra de arriba', () => {
		const view = mountSidebar();

		const bar = view.findComponent(SideBar);
		expect(bar.find('header').exists()).toBe(false);
		// Sin cabecera el botón de plegar necesita su propio lugar, o la barra
		// deja de poder plegarse.
		expect(bar.find('button[aria-label="barraLateral.plegar"]').exists()).toBe(true);
	});

	test('cada paso es el botón de la librería, con su descripción y su número', () => {
		const view = mountSidebar();

		const buttons = view.findAllComponents(SideButton);
		expect(buttons).toHaveLength(PASOS.length);
		expect(buttons.map((b) => b.props('badge'))).toEqual(PASOS.map((_, i) => i + 1));
		expect(stepButton(view, 'disco')?.props('description')).toBe('pasos.disco.descripcion');
	});

	test('siguen siendo una lista ordenada con su nombre', () => {
		// Para un lector de pantalla esto es «lista de 10 elementos, elemento
		// 4», que es la misma información que la barra le da a quien la ve.
		const view = mountSidebar();

		const list = view.find('ol');
		expect(list.exists()).toBe(true);
		// El nombre de la lista y no el del primer paso: con `pasos.bienvenida`
		// un lector de pantalla anunciaba la lista entera como «Bienvenida».
		expect(list.attributes('aria-label')).toBe('barraLateral.steps');
		expect(list.findAll('li')).toHaveLength(PASOS.length);
	});
});

describe('el estado de cada paso', () => {
	test('los de antes están hechos, el de ahora es el actual y los de después esperan', () => {
		const view = mountSidebar('teclado');

		expect(stateOf(view, 'bienvenida')).toBe('done');
		expect(stateOf(view, 'teclado')).toBe('current');
		expect(stateOf(view, 'disco')).toBe('pending');
		expect(stepButton(view, 'teclado')?.props('active')).toBe(true);
		expect(stepButton(view, 'bienvenida')?.props('active')).toBe(false);
	});

	test('el hecho lleva el emblema, y no reemplaza al icono del paso', () => {
		// Reemplazar el icono por un tilde dejaba los pasos hechos iguales entre
		// sí. El emblema es un glifo además de un color (WCAG 1.4.1).
		const view = mountSidebar('teclado');
		const tile = (step: Paso) => view.find(`li[data-step="${step}"]`).findComponent(IconTile);

		expect(tile('bienvenida').props('status')).toBe('success');
		expect(tile('bienvenida').props('name')).toBe('help-about');
		expect(tile('teclado').props('status')).toBeNull();
		expect(tile('teclado').props('tone')).toBe('selected');
		expect(tile('disco').props('status')).toBeNull();
		// Simbólicos, como estaban: un icono a color a 20 píxeles es una mancha.
		expect(tile('disco').props('type')).toBe('symbol');
	});

	test('el actual se marca para quien no ve el color, como paso y no como página', () => {
		// Es `step` y no `page`: esto es un asistente, no una navegación. Le
		// gana al `page` que pone la librería.
		const view = mountSidebar('teclado');

		expect(stepButton(view, 'teclado')?.find('button').attributes('aria-current')).toBe('step');
		expect(stepButton(view, 'disco')?.find('button').attributes('aria-current')).toBeUndefined();
	});

	test('el actual no se ve apagado aunque no se pueda volver a él', () => {
		// `SideButton` deshabilitado baja la opacidad; el paso en el que se está
		// es justo el que no tiene que verse así.
		const view = mountSidebar('teclado');

		expect(stepButton(view, 'teclado')?.find('button').classes()).toContain(
			'aria-[current=step]:opacity-100'
		);
	});
});

describe('volver a un paso', () => {
	test('se puede a uno anterior, y avisa a quién', async () => {
		const view = mountSidebar('teclado');

		await stepButton(view, 'region')?.find('button').trigger('click');

		expect(view.emitted('go')?.[0]).toEqual(['region']);
	});

	test('no se puede a uno que todavía no llegó', async () => {
		// Saltear pasos deja la instalación sin la mitad de lo que necesita.
		const view = mountSidebar('teclado');

		expect(stepButton(view, 'disco')?.find('button').attributes('disabled')).toBeDefined();
		await stepButton(view, 'disco')?.find('button').trigger('click');
		expect(view.emitted('go')).toBeUndefined();
	});

	test('ni a ninguno una vez que se empezó a escribir en el disco', () => {
		// Lo que ya se escribió no se deshace volviendo a la pantalla anterior:
		// dejar el botón vivo sería ofrecer algo que no existe.
		const view = mountSidebar('instalacion', false);

		for (const step of PASOS) {
			expect(stepButton(view, step)?.find('button').attributes('disabled')).toBeDefined();
		}
	});
});

describe('plegada', () => {
	test('cada paso conserva su nombre, con el número, para el puntero y el lector de pantalla', async () => {
		// 84 píxeles es el ancho del icono: el título y la descripción no
		// entran. Sin el `title` y el `aria-label`, plegada la barra es una
		// columna de dibujos sin explicación y diez botones sin nombre.
		const view = mountSidebar('teclado');
		await view.findComponent(SideBar).find('button[aria-label]').trigger('click');
		await nextTick();

		const step = stepButton(view, 'teclado')?.find('button');
		expect(step?.attributes('title')).toBe('4. pasos.teclado.titulo');
		expect(step?.attributes('aria-label')).toBe('4. pasos.teclado.titulo');
	});
});

describe('la ventana angosta: una columna por vez', () => {
	let app: VueWrapper | null = null;

	beforeEach(() => setActivePinia(createPinia()));
	afterEach(() => {
		app?.unmount();
		app = null;
	});

	async function openApp(step: Paso = 'disco') {
		app = mount(App, {
			// Las pantallas no se miran acá: piden sus datos al backend y lo que
			// se prueba es el armazón de la ventana.
			global: {
				stubs: {
					WelcomeView: { template: '<div />' },
					DiskView: { template: '<div />' },
					RegionView: { template: '<div />' },
				},
			},
		});
		await flushPromises();
		useInstalacionStore().paso = step;
		await flushPromises();
		return app;
	}

	test('la fila de la ventana es el contenedor, y la barra se esconde por debajo de 30rem', async () => {
		// Por el ancho de la fila y no de la pantalla: un componente no sabe en
		// qué ventana está, y WebKitGTK no avisa de `resize`.
		const view = await openApp();

		expect(view.find('.\\@container\\/window').exists()).toBe(true);
		expect(view.find('[data-steps-rail]').classes()).toContain('@max-[30rem]/window:hidden');
	});

	test('en su lugar queda una franja con el paso actual, que sólo se ve angosta', async () => {
		// Lo que dice la barra no desaparece: el número, el total y el nombre.
		const view = await openApp('disco');
		const strip = view.find('[data-steps-bar]');

		expect(strip.classes()).toEqual(expect.arrayContaining(['hidden', '@max-[30rem]/window:flex']));
		expect(strip.text()).toBe('barraLateral.showSteps');
	});

	test('la franja abre la ficha de pasos en lugar del contenido, y «volver» la cierra', async () => {
		const view = await openApp('disco');
		expect(view.find('[data-steps-sheet]').exists()).toBe(false);

		await view.find('[data-steps-bar] button').trigger('click');

		const sheet = view.find('[data-steps-sheet]');
		expect(sheet.exists()).toBe(true);
		// Una columna: la ficha se ve sólo angosta, y ahí el contenido se va.
		expect(sheet.classes()).toEqual(expect.arrayContaining(['hidden', '@max-[30rem]/window:flex']));
		expect(view.find('[data-step-content]').classes()).toContain('@max-[30rem]/window:hidden');
		// Es la misma lista que la barra, desplegada.
		expect(sheet.findComponent(StepList).props('collapsed')).toBe(false);

		await sheet.find('button').trigger('click');
		expect(view.find('[data-steps-sheet]').exists()).toBe(false);
		expect(view.find('[data-step-content]').classes()).not.toContain('@max-[30rem]/window:hidden');
	});

	test('elegir un paso en la ficha va a ese paso y vuelve al contenido', async () => {
		const view = await openApp('disco');
		await view.find('[data-steps-bar] button').trigger('click');

		const sheet = view.find('[data-steps-sheet]');
		const region = sheet
			.findAllComponents(SideButton)
			.find((button: { props: () => unknown }) => (button.props() as { label?: string }).label === 'pasos.region.titulo');
		await region?.find('button').trigger('click');
		await flushPromises();

		expect(useInstalacionStore().paso).toBe('region');
		expect(view.find('[data-steps-sheet]').exists()).toBe(false);
	});

	test('y en ventana ancha nada de esto cambia el formato: la barra está, la franja no', async () => {
		// Las clases de la franja y de la ficha empiezan en `hidden`: sólo la
		// consulta de contenedor las muestra. La barra no tiene `hidden` propio.
		const view = await openApp('disco');

		expect(view.find('[data-steps-rail]').classes()).not.toContain('hidden');
		expect(view.find('[data-steps-bar]').classes()).toContain('hidden');
	});
});
