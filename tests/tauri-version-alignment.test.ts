import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * `tauri build` compara cada crate de Tauri con su paquete de npm y aborta si
 * no coinciden en major.minor («Found version mismatched Tauri packages»). Con
 * `--locked`, que es como compila el PKGBUILD, no hay forma de que lo arregle
 * sola: lo que digan los dos candados es lo que se compila. Esta prueba hace el
 * mismo chequeo sobre `src-tauri/Cargo.lock` y `bun.lock`, para que el
 * desfasaje se vea en el CI y no al empaquetar.
 *
 * Las parejas son las que mira el CLI de Tauri: `tauri` ↔ `@tauri-apps/api` y
 * `tauri-plugin-<x>` ↔ `@tauri-apps/plugin-<x>`.
 */

const ROOT = join(import.meta.dir, '..');
const read = (path: string) => readFileSync(join(ROOT, path), 'utf8');

/** Versiones de cada crate del `Cargo.lock` (puede haber más de una). */
function cargoLockVersions(lock: string): Map<string, string[]> {
	const versions = new Map<string, string[]>();
	const entry = /\[\[package\]\]\s*\nname = "([^"]+)"\s*\nversion = "([^"]+)"/g;
	for (const [, name, version] of lock.matchAll(entry)) {
		versions.set(name, [...(versions.get(name) ?? []), version]);
	}
	return versions;
}

/** Versión que `bun.lock` resolvió para cada paquete de primer nivel. */
function bunLockVersions(lock: string): Map<string, string> {
	const versions = new Map<string, string>();
	// Sólo las claves sin ruta (`"@scope/pkg": ["@scope/pkg@x.y.z", …`): las
	// anidadas (`"otro/@scope/pkg"`) no son las que ve el proyecto.
	const entry = /^\s*"(@?[^"/]+(?:\/[^"/]+)?)": \["\1@([^"]+)"/gm;
	for (const [, name, version] of lock.matchAll(entry)) {
		versions.set(name, version);
	}
	return versions;
}

function npmCounterpart(crate: string): string | undefined {
	if (crate === 'tauri') return '@tauri-apps/api';
	const plugin = /^tauri-plugin-(.+)$/.exec(crate);
	return plugin ? `@tauri-apps/plugin-${plugin[1]}` : undefined;
}

const majorMinor = (version: string) => version.split('.').slice(0, 2).join('.');

const crates = cargoLockVersions(read('src-tauri/Cargo.lock'));
const packages = bunLockVersions(read('bun.lock'));

const pairs = [...crates].flatMap(([crate, crateVersions]) => {
	const npm = npmCounterpart(crate);
	const npmVersion = npm ? packages.get(npm) : undefined;
	return npm && npmVersion ? [{ crate, crateVersions, npm, npmVersion }] : [];
});

describe('candados de Tauri', () => {
	test('se leen las parejas de los dos candados', () => {
		// Si el formato de algún candado cambia y la lectura deja de encontrar
		// parejas, la prueba de abajo pasaría sin comparar nada.
		expect(pairs.map((pair) => pair.crate)).toContain('tauri');
		expect(pairs.map((pair) => pair.crate)).toContain('tauri-plugin-shell');
	});

	test('cada crate de Tauri coincide en major.minor con su paquete de npm', () => {
		const mismatched = pairs.flatMap(({ crate, crateVersions, npm, npmVersion }) =>
			crateVersions
				.filter((version) => majorMinor(version) !== majorMinor(npmVersion))
				.map((version) => `${crate} (v${version}) : ${npm} (v${npmVersion})`),
		);
		expect(mismatched).toEqual([]);
	});
});
