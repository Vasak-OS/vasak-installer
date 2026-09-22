/**
 * La barra lateral anima lo que cambia, y nada más.
 *
 * El instalador no dibuja ningún icono del tema, así que el planificador de
 * recarga de la 1.3.0 —lo que motivó esta tanda en los otros repositorios— acá
 * no se nota. Lo que sí le llega de la librería nueva es esto: hasta la 1.1.0
 * `SideBar` traía `transition-all`, que obliga al navegador a mirar **cada**
 * propiedad animable en cada cambio, incluidas las que nadie toca. Lo único que
 * la barra cambia al plegarse es el ancho.
 *
 * No falla nunca ni se ve como un error: se ve como una aplicación que va un
 * poco pesada. Por eso hay una prueba y no un «se actualizó la dependencia».
 *
 * Y es el motivo de que este PR exista: el manifiesto pedía `^1.1.0` —que
 * admite la 1.4.0— pero `bun.lock` se había quedado en la 1.1.0, así que el
 * arreglo estaba publicado desde hacía cuatro minors y acá no había llegado.
 */

import { describe, expect, test } from 'bun:test';
import { SideBar } from '@vasakgroup/vue-libvasak';
import { mount } from '@vue/test-utils';
import PasosSidebar from '@/components/sidebar/PasosSidebar.vue';

function mountSidebar() {
	return mount(PasosSidebar, { props: { actual: 'teclado' as const, navegable: true } });
}

describe('la barra lateral del instalador', () => {
	test('acota la transición al ancho, que es lo único que cambia', () => {
		const clases = mountSidebar().findComponent(SideBar).classes();

		expect(clases).toContain('transition-[width]');
		expect(clases).not.toContain('transition-all');
	});
});
