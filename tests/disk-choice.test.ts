/**
 * El disco elegido, de punta a punta.
 *
 * Es la pantalla donde un error cuesta datos: si lo que se marca, lo que dice
 * el resumen y lo que recibe el backend no son **el mismo disco**, se borra uno
 * que nadie eligió. Al pasar la pantalla a los componentes de la librería
 * (`OptionGroup variant="card"`, vue-libvasak#74) se comprueban las tres cosas
 * juntas, con la ventana entera montada y no con el almacén solo, que es como
 * se escaparía una vista que muestra una cosa y guarda otra.
 *
 * Y la otra mitad: que nada quede elegido que antes no lo estaba. El grupo de
 * la librería sólo escribe cuando se lo toca; la única preselección sigue
 * siendo la del almacén —el disco usable más grande—, igual que antes.
 *
 * Nada de esto toca un disco: el backend es el doble de `tests/dobles.ts`, que
 * anota lo que se le pide y contesta lo que se le prepara.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { OptionGroup } from '@vasakgroup/vue-libvasak';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import App from '@/App.vue';
import { type Disco, useInstalacionStore } from '@/stores/instalacion';
import DiskView from '@/views/DiskView.vue';
import SummaryView from '@/views/SummaryView.vue';
import { olvidarTodo, pedidos, responder } from './dobles';

const GIB = 1024 ** 3;

function disk(path: string, gib: number, extra: Partial<Disco> = {}): Disco {
	return {
		ruta: path,
		modelo: `Modelo de ${path}`,
		tamano_bytes: gib * GIB,
		sector_logico: 512,
		rotacional: false,
		nvme: false,
		en_uso: false,
		particiones: [],
		...extra,
	};
}

/** Un disco con un Windows adentro, para que aparezca el selector de esquema. */
function withWindows(path: string, gib: number): Disco {
	return disk(path, gib, {
		nvme: true,
		particiones: [
			{
				ruta: `${path}p1`,
				inicio_bytes: 1024 ** 2,
				tamano_bytes: 0.1 * GIB,
				sistema_archivos: 'vfat',
				etiqueta: 'EFI',
				numero: 1,
				tipo_particion: 'c12a7328-f81f-11d2-ba4b-00a0c93ec93b',
				sistema_operativo: null,
			},
			{
				ruta: `${path}p2`,
				inicio_bytes: GIB,
				tamano_bytes: 300 * GIB,
				sistema_archivos: 'ntfs',
				etiqueta: null,
				numero: 2,
				tipo_particion: null,
				sistema_operativo: 'Windows 11',
			},
		],
	});
}

/**
 * Los cuatro de la máquina de prueba, en el orden en que los lista `lsblk`:
 * el pendrive primero —que es por qué el almacén no elige el primero—, el NVMe
 * con Windows, uno en uso y uno demasiado chico.
 */
const DISKS: Disco[] = [
	disk('/dev/sda', 32),
	withWindows('/dev/nvme0n1', 512),
	disk('/dev/sdb', 1000, { rotacional: true, en_uso: true }),
	disk('/dev/sdc', 8),
];

/** El backend de mentira, con lo justo para que la ventana arranque. */
function prepareBackend() {
	responder('sondear_sistema', {
		firmware: 'uefi',
		memoria_bytes: 16 * GIB,
		cpu: 'CPU de prueba',
		nucleos: 8,
		hay_red: true,
		virtualizacion: null,
	});
	responder('pasos_de_instalacion', ['particionar']);
	responder('catalogos', { zonas: ['UTC'], idiomas: ['es_AR'], teclados: ['us'] });
	responder('complementos_disponibles', {
		catalogo: [],
		categorias: [],
		hardware: { marcas: [], descripciones: [] },
		preseleccion: [],
		error: null,
	});
	responder('sondear_discos', () => structuredClone(DISKS));
	responder('sondear_discos_con_sistemas', () => structuredClone(DISKS));
	responder('preparar_ayudante', null);
	responder('ayudante_listo', true);
	responder('puntos_de_montaje', ['/', '/boot', '/home']);
	responder('vista_previa_particionado', (args: Record<string, unknown>) => ({
		firmware: 'uefi',
		particiones: [],
		se_pierde: args.esquema === 'borrar_todo' ? [`${String(args.disco)} entero`] : [],
	}));
}

/** El grupo de opciones cuyo nombre es ése (la clave, que es lo que devuelve el `t()` doble). */
function group(view: VueWrapper, label: string) {
	return view
		.findAllComponents(OptionGroup)
		.find((g) => (g.props() as { label?: string }).label === label);
}

/** Las tarjetas de disco, en orden. */
function diskCards(view: VueWrapper) {
	const disks = group(view, 'disco.titulo');
	if (!disks) throw new Error('no está el grupo de discos');
	return disks.findAll('[role="radio"]');
}

function checkedPaths(view: VueWrapper): string[] {
	return diskCards(view)
		.filter((card) => card.attributes('aria-checked') === 'true')
		.map((card) => card.find('[data-disk]').attributes('data-disk') as string);
}

let view: VueWrapper | null = null;

beforeEach(() => {
	olvidarTodo();
	setActivePinia(createPinia());
	prepareBackend();
});

afterEach(() => {
	view?.unmount();
	view = null;
});

describe('nada queda elegido que antes no lo estaba', () => {
	test('sin disco elegido, la pantalla no elige ninguno', async () => {
		// Antes no había preselección en la vista: la tarjeta sólo pintaba lo
		// que dijera el almacén. El grupo de la librería tampoco escribe solo.
		// Sin el ayudante, la pantalla no recarga la lista: así lo único que
		// podría elegir algo es la propia pantalla. (Con el ayudante, recarga y
		// el almacén preselecciona, que es lo de la prueba siguiente.)
		responder('ayudante_listo', false);
		const store = useInstalacionStore();
		store.discos = structuredClone(DISKS);
		view = mount(DiskView);
		await flushPromises();

		expect(store.eleccion.disco).toBe('');
		expect(checkedPaths(view)).toEqual([]);
	});

	test('la preselección sigue siendo la del almacén: el usable más grande, no el primero', async () => {
		// El pendrive va primero en la lista, el de un tera está en uso. Queda
		// el NVMe, igual que antes del cambio.
		const store = useInstalacionStore();
		await store.recargarDiscos();

		expect(store.eleccion.disco).toBe('/dev/nvme0n1');
	});

	test('y la pantalla muestra ésa, sin cambiarla', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		view = mount(DiskView);
		await flushPromises();

		expect(store.eleccion.disco).toBe('/dev/nvme0n1');
		expect(checkedPaths(view)).toEqual(['/dev/nvme0n1']);
	});

	test('el esquema y el sistema de archivos son los de siempre, y ninguna partición', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		view = mount(DiskView);
		await flushPromises();

		const checkedIn = (label: string) =>
			view ? group(view, label)?.findAll('[role="radio"][aria-checked="true"]').length : undefined;

		expect(store.eleccion.esquema).toBe('borrar_todo');
		expect(checkedIn('disco.esquema')).toBe(1);
		expect(store.eleccion.sistemaArchivos).toBe('btrfs');
		expect(checkedIn('disco.sistemaArchivos')).toBe(1);

		// Con «sobre una partición» aparece la lista, y no viene ninguna marcada.
		store.eleccion.esquema = 'sobre_una_particion';
		await flushPromises();
		expect(store.eleccion.particionDestino).toBe('');
		expect(checkedIn('disco.elegirParticion')).toBe(0);
	});
});

describe('elegir un disco', () => {
	test('el clic elige ése, y sólo ése', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		view = mount(DiskView);
		await flushPromises();

		await diskCards(view)[0]?.trigger('click');

		expect(store.eleccion.disco).toBe('/dev/sda');
		expect(checkedPaths(view)).toEqual(['/dev/sda']);
	});

	test('uno en uso o demasiado chico no se puede elegir, ni con el ratón ni con el teclado', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		view = mount(DiskView);
		await flushPromises();
		const cards = diskCards(view);

		expect(cards[2]?.attributes('disabled')).toBeDefined();
		expect(cards[3]?.attributes('disabled')).toBeDefined();
		await cards[2]?.trigger('click');
		await cards[3]?.trigger('click');
		expect(store.eleccion.disco).toBe('/dev/nvme0n1');

		// Las flechas eligen en un grupo de radios; desde el NVMe, abajo salta
		// los dos que no se pueden usar y vuelve al pendrive.
		await cards[1]?.trigger('keydown', { key: 'ArrowDown' });
		await nextTick();
		expect(store.eleccion.disco).toBe('/dev/sda');
	});

	test('cambiar de disco olvida la partición elegida en el otro', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		store.eleccion.esquema = 'sobre_una_particion';
		store.eleccion.particionDestino = '/dev/nvme0n1p2';
		view = mount(DiskView);
		await flushPromises();

		await diskCards(view)[0]?.trigger('click');

		expect(store.eleccion.particionDestino).toBe('');
		// Y el disco vacío vuelve a «borrar el disco», que es lo único posible.
		expect(store.eleccion.esquema as string).toBe('borrar_todo');
	});
});

describe('el resumen', () => {
	test('muestra el disco elegido, con su modelo, y nombra ése en el aviso', async () => {
		const store = useInstalacionStore();
		await store.recargarDiscos();
		store.eleccion.disco = '/dev/sda';
		await store.calcularVistaPrevia();
		view = mount(SummaryView);
		await flushPromises();

		expect(view.find('[data-chosen-disk]').text()).toBe('/dev/sda');
		expect(view.text()).toContain('Modelo de /dev/sda');
		expect(view.text()).not.toContain('Modelo de /dev/nvme0n1');
		// El aviso rojo sale del plan de **este** disco.
		expect(view.text()).toContain('/dev/sda entero');
	});
});

describe('de la pantalla al backend', () => {
	test('el disco que se toca es el que dice el resumen y el que viaja en el plan', async () => {
		// La ventana entera: se elige el pendrive en la pantalla del disco
		// —distinto del preseleccionado, para que no pase por casualidad—, se
		// mira el resumen y se aprieta «Instalar». El doble del backend anota el
		// plan; nada se instala.
		view = mount(App);
		await flushPromises();
		const store = useInstalacionStore();
		expect(store.eleccion.disco).toBe('/dev/nvme0n1');

		store.paso = 'disco';
		await flushPromises();
		await diskCards(view)[0]?.trigger('click');
		await flushPromises();
		expect(checkedPaths(view)).toEqual(['/dev/sda']);

		store.eleccion.usuario = 'ana';
		store.secretos.usuario = 'una frase larga';
		store.secretos.usuarioRepetida = 'una frase larga';
		store.paso = 'resumen';
		await flushPromises();

		const shown = view.find('[data-chosen-disk]').text();
		expect(shown).toBe('/dev/sda');

		const confirm = view.findAll('footer button').find((b) => b.text() === 'resumen.confirmar');
		expect(confirm?.attributes('disabled')).toBeUndefined();
		await confirm?.trigger('click');
		await flushPromises();

		const sent = pedidos('instalar');
		expect(sent).toHaveLength(1);
		const plan = sent[0]?.argumentos.plan as { disco: string };
		expect(plan.disco).toBe(shown);
		expect(plan.disco).toBe(store.eleccion.disco);
	});
});
