/**
 * Lo que cambia al adoptar la librería.
 *
 * El instalador tenía su propio campo de texto, su barra de progreso, su
 * selector con búsqueda, su aviso y su interruptor. Cinco piezas que también
 * existían, con otro nombre y otra suerte, en las demás ventanas del sistema.
 *
 * Lo que se comprueba acá es lo del instalador y no lo de la librería. Quedan
 * dos cosas: el icono del aviso, que es lo único que esta ventana agrega encima
 * del componente compartido —de las seis copias de aviso del sistema era la
 * única que lo tenía—, y la barra de progreso, que es el único punto donde la
 * adopción **cambió una unidad**: la fracción de 0 a 1 pasó a por ciento, y una
 * barra que se equivoca por cien se lee como que no avanza nunca.
 *
 * La fila apretable del interruptor ya no está acá: pasó a ser `SwitchRow` de
 * la librería, y se prueba allá.
 */

import { beforeEach, describe, expect, test } from 'bun:test';
import { olvidarLosIconosDelTema } from '@vasakgroup/vue-libvasak';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import InstalacionView from '@/views/InstalacionView.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { olvidarTodo, ponerEnElTema } from './dobles';

beforeEach(() => {
	olvidarTodo();
	// Y la memoria de iconos de la librería, que es otra cosa: vive en su módulo
	// y se comparte entre archivos de prueba, así que el primero que pida un
	// icono con el tema sin preparar deja guardado que no hay ninguno. Sin esto
	// este archivo pasa solo y falla en la suite —pasó—.
	olvidarLosIconosDelTema();
	setActivePinia(createPinia());
});

describe('el aviso', () => {
	test('cada tono trae su icono, que es lo único que el instalador agrega', async () => {
		// De las seis copias de aviso del sistema, la de acá era la única con
		// icono: un aviso que dice que se va a borrar un disco tiene que hacerse
		// mirar. El color y el rol los pone la librería.
		for (const [tone, icono] of [
			['error', 'dialog-error'],
			['warning', 'dialog-warning'],
			['success', 'object-select'],
			['info', 'dialog-information'],
		] as const) {
			olvidarTodo();
			olvidarLosIconosDelTema();
			ponerEnElTema(icono, `fuente-de-${icono}`);
			const vista = mount(AlertMessage, { props: { tone }, slots: { default: 'x' } });
			await nextTick();
			await new Promise((listo) => setTimeout(listo, 0));

			expect(vista.find('img').attributes('src')).toBe(`fuente-de-${icono}`);
			vista.unmount();
		}
	});

	test('y el rol lo sigue poniendo la librería: el error interrumpe', () => {
		// Es lo que se perdería si esto volviera a ser un componente propio.
		const error = mount(AlertMessage, { props: { tone: 'error' }, slots: { default: 'x' } });
		const aviso = mount(AlertMessage, { props: { tone: 'warning' }, slots: { default: 'x' } });

		expect(error.attributes('role')).toBe('alert');
		expect(aviso.attributes('role')).toBe('status');
	});
});

describe('la barra de progreso, que cambió de unidad', () => {
	function conPasos(hechos: number, total: number, parcial: number | null = null) {
		const store = useInstalacionStore();
		store.pasosInstalacion = Array.from({ length: total }, (_, i) => `paso${i}`);
		const mapa = new Map();
		for (let i = 0; i < total; i++) {
			const estado = i < hechos ? 'hecho' : i === hechos && parcial !== null ? 'en_curso' : 'pendiente';
			mapa.set(`paso${i}`, { paso: `paso${i}`, estado, fraccion: estado === 'en_curso' ? parcial : null, detalle: null });
		}
		store.progreso = mapa;
		return mount(InstalacionView);
	}

	const laBarra = (vista: ReturnType<typeof conPasos>) =>
		vista.find('[role="progressbar"]');

	test('la mitad de los pasos es 50 y no 0,5', async () => {
		// La librería habla en por ciento y el instalador hablaba en fracción.
		// Con la fracción sin convertir, `aria-valuenow` diría «0» durante toda
		// la instalación y la barra no se movería nunca.
		const vista = conPasos(5, 10);
		await nextTick();

		expect(laBarra(vista).attributes('aria-valuenow')).toBe('50');
	});

	test('y un paso a medias suma su parte', async () => {
		const vista = conPasos(2, 4, 0.5);
		await nextTick();

		expect(laBarra(vista).attributes('aria-valuenow')).toBe('62.5');
	});

	test('el número escrito al lado dice lo mismo que la barra', async () => {
		// Los dos leen el mismo cálculo: si se separan, uno de los dos miente.
		const vista = conPasos(1, 4);
		await nextTick();

		expect(laBarra(vista).attributes('aria-valuenow')).toBe('25');
		expect(vista.text()).toContain('25%');
	});
});
