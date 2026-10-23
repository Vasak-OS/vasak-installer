import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { applyTheme, saveTheme, THEME_KEY } from '../src/tools/theme-cache';

/** Un `Storage` mínimo en memoria, para no tocar el global. */
function memoryStore(): Storage {
	const data = new Map<string, string>();
	return {
		get length() {
			return data.size;
		},
		clear: () => data.clear(),
		getItem: (k: string) => data.get(k) ?? null,
		key: (i: number) => Array.from(data.keys())[i] ?? null,
		removeItem: (k: string) => void data.delete(k),
		setItem: (k: string, v: string) => void data.set(k, v),
	};
}

function throwingStore(): Storage {
	const fail = () => {
		throw new Error('storage inaccesible');
	};
	return { ...memoryStore(), getItem: fail, setItem: fail } as Storage;
}

const read = (path: string) => readFileSync(join(import.meta.dir, '..', path), 'utf8');

describe('theme-cache', () => {
	test('la clave es la misma que lee el script del <head>', () => {
		expect(THEME_KEY).toBe('vsk-theme');
	});

	test('saveTheme guarda la clase oscura y sólo las variables --*', () => {
		const root = document.createElement('div');
		root.classList.add('dark');
		root.style.setProperty('--ui-background', '#111');
		root.style.setProperty('color', 'red');
		const store = memoryStore();

		saveTheme(root, store);

		expect(JSON.parse(store.getItem(THEME_KEY) as string)).toEqual({
			dark: true,
			vars: { '--ui-background': '#111' },
		});
	});

	test('applyTheme reaplica la clase y las variables', () => {
		const store = memoryStore();
		store.setItem(THEME_KEY, JSON.stringify({ dark: true, vars: { '--primary': '#abc' } }));
		const root = document.createElement('div');

		applyTheme(root, store);

		expect(root.classList.contains('dark')).toBe(true);
		expect(root.style.getPropertyValue('--primary')).toBe('#abc');
	});

	test('ida y vuelta: lo guardado de un elemento se reaplica en otro', () => {
		const a = document.createElement('div');
		a.style.setProperty('--ui-line', '#222');
		const store = memoryStore();
		saveTheme(a, store);

		const b = document.createElement('div');
		applyTheme(b, store);

		expect(b.classList.contains('dark')).toBe(false);
		expect(b.style.getPropertyValue('--ui-line')).toBe('#222');
	});

	test('sin clave no hace nada', () => {
		const root = document.createElement('div');
		applyTheme(root, memoryStore());
		expect(root.className).toBe('');
		expect(root.getAttribute('style')).toBeNull();
	});

	test('un JSON inválido no lanza', () => {
		const store = memoryStore();
		store.setItem(THEME_KEY, '{no es json');
		expect(() => applyTheme(document.createElement('div'), store)).not.toThrow();
	});

	test('un Storage que lanza no rompe el arranque', () => {
		const root = document.createElement('div');
		expect(() => saveTheme(root, throwingStore())).not.toThrow();
		expect(() => applyTheme(root, throwingStore())).not.toThrow();
	});
});

describe('cableado del tema recordado', () => {
	test('index.html reaplica el tema antes de montar', () => {
		const html = read('index.html');
		const head = html.slice(0, html.indexOf('</head>'));
		expect(head).toContain('vsk-theme');
		expect(head).toContain('classList.add("dark")');
		expect(head).toContain('setProperty');
	});

	test('App.vue guarda el tema tras cada loadConfig', () => {
		const app = read('src/App.vue');
		expect(app).toContain("from '@/tools/theme-cache'");
		const loads = app.match(/loadConfig\(\)/g) ?? [];
		const saves = app.match(/loadConfig\(\)\s*\.then\(\(\) => saveTheme\(\)\)/g) ?? [];
		expect(loads.length).toBe(2);
		expect(saves.length).toBe(2);
	});
});
