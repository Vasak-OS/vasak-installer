/**
 * Los campos de contraseña son `PasswordField` de la librería.
 *
 * Eran `TextInput type="password"`: sin botón de mostrar —quien se equivoca al
 * escribir una frase de cifrado no tiene cómo verlo— y sin el aviso de Bloq
 * Mayús, en la pantalla donde un error de tipeo deja el disco sin abrir.
 *
 * Se prueban las dos pantallas montadas, con el campo vivo: que sea el
 * componente, que pida `new-password` (todas crean la clave) y que el botón de
 * mostrar cambie el tipo del campo. Y una guarda sobre el fuente, para que no
 * vuelva un `type="password"` crudo en otra pantalla.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { Glob } from 'bun';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PasswordField } from '@vasakgroup/vue-libvasak';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { useInstalacionStore } from '@/stores/instalacion';
import AccountView from '@/views/AccountView.vue';
import DiskView from '@/views/DiskView.vue';
import { olvidarTodo, responder } from './dobles';

let view: VueWrapper | null = null;

beforeEach(() => {
	olvidarTodo();
	setActivePinia(createPinia());
	responder('fuerza_contrasena', 0);
	responder('sugerir_usuario', '');
	responder('validar_usuario', null);
	responder('validar_hostname', null);
	responder('sondear_discos', []);
	responder('sondear_discos_con_sistemas', []);
	responder('preparar_ayudante', null);
	responder('ayudante_listo', false);
	responder('puntos_de_montaje', []);
	responder('vista_previa_particionado', { firmware: 'uefi', particiones: [], se_pierde: [] });
});

afterEach(() => {
	view?.unmount();
	view = null;
});

/** Cada campo tiene que ser un `PasswordField` que crea la clave y se puede revelar. */
async function expectRevealable(wrapper: VueWrapper, amount: number) {
	const fields = wrapper.findAllComponents(PasswordField);
	expect(fields).toHaveLength(amount);
	for (const field of fields) {
		expect((field.props() as { autocomplete?: string }).autocomplete).toBe('new-password');
		const input = field.find('input');
		expect(input.attributes('type')).toBe('password');
		const toggle = field.find('button[aria-pressed]');
		expect(toggle.exists()).toBe(true);
		await toggle.trigger('click');
		expect(input.attributes('type')).toBe('text');
		expect(toggle.attributes('aria-pressed')).toBe('true');
	}
	// Ningún campo de contraseña crudo quedó fuera del componente.
	expect(wrapper.findAll('input[type="password"]').length).toBe(0);
}

describe('los campos de contraseña', () => {
	test('la cuenta: contraseña y repetir, y las de root, se pueden mostrar', async () => {
		const store = useInstalacionStore();
		store.eleccion.rootHabilitado = true;
		view = mount(AccountView);
		await flushPromises();

		await expectRevealable(view, 4);
	});

	test('el disco: la frase de cifrado y su repetición se pueden mostrar', async () => {
		const store = useInstalacionStore();
		store.eleccion.cifrar = true;
		view = mount(DiskView);
		await flushPromises();

		await expectRevealable(view, 2);
	});
});

describe('el fuente', () => {
	test('ninguna pantalla vuelve a un campo `type="password"` crudo', async () => {
		const ROOT = fileURLToPath(new URL('../src/', import.meta.url));
		const offenders: string[] = [];
		for (const path of new Glob('**/*.vue').scanSync(ROOT)) {
			const text = await Bun.file(join(ROOT, path)).text();
			if (/type=["']password["']/.test(text)) offenders.push(path);
		}
		expect(offenders).toEqual([]);
	});
});
