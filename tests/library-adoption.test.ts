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

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { Glob } from 'bun';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { olvidarLosIconosDelTema } from '@vasakgroup/vue-libvasak';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { nextTick } from 'vue';
import { AlertMessage } from '@vasakgroup/vue-libvasak';
import { useInstalacionStore } from '@/stores/instalacion';
import InstallationView from '@/views/InstallationView.vue';
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
	const FUENTE = fileURLToPath(new URL('../src/', import.meta.url));
	const fuentes = [...new Glob('**/*.vue').scanSync(FUENTE)];

	test('es el de la librería con el icono de su tono, que es lo único que el instalador agrega', async () => {
		// De las seis copias de aviso del sistema, la de acá era la única con
		// icono: un aviso que dice que se va a borrar un disco tiene que hacerse
		// mirar. La capa propia que lo ponía se fue: la librería trae el icono
		// por tono con `icon="auto"`, así que cada aviso tiene que pedirlo.
		const sinIcono: string[] = [];
		let avisos = 0;
		for (const ruta of fuentes) {
			const texto = await Bun.file(join(FUENTE, ruta)).text();
			for (const [etiqueta] of texto.matchAll(/<AlertMessage\b[^>]*>/gs)) {
				avisos++;
				if (!etiqueta.includes('icon="auto"')) sinIcono.push(`${ruta}: ${etiqueta.replace(/\s+/g, ' ')}`);
			}
		}

		expect(avisos).toBeGreaterThan(20);
		expect(sinIcono).toEqual([]);
	});

	test('y con `auto` dibuja el de cada tono', async () => {
		for (const [tone, icono] of [
			['error', 'dialog-error'],
			['warning', 'dialog-warning'],
			['success', 'object-select'],
			['info', 'dialog-information'],
		] as const) {
			olvidarTodo();
			olvidarLosIconosDelTema();
			ponerEnElTema(icono, `fuente-de-${icono}`);
			const vista = mount(AlertMessage, { props: { tone, icon: 'auto' }, slots: { default: 'x' } });
			await nextTick();
			await new Promise((listo) => setTimeout(listo, 0));

			expect(vista.find('img').attributes('src')).toBe(`fuente-de-${icono}`);
			vista.unmount();
		}
	});

	test('y el rol lo sigue poniendo la librería: el error interrumpe', () => {
		const error = mount(AlertMessage, { props: { tone: 'error' }, slots: { default: 'x' } });
		const aviso = mount(AlertMessage, { props: { tone: 'warning' }, slots: { default: 'x' } });

		expect(error.attributes('role')).toBe('alert');
		expect(aviso.attributes('role')).toBe('status');
	});
});

describe('la barra de progreso, que cambió de unidad', () => {
	/**
	 * Las vistas montadas, para desmontarlas pase lo que pase.
	 *
	 * `InstallationView` arranca un `setInterval` al montarse —el reloj de
	 * «transcurrido»— y lo limpia al desmontarse. Sin esto, cada prueba deja un
	 * temporizador vivo corriendo contra una vista que ya nadie mira, y una
	 * aserción que falle se saltea el desmontaje del final.
	 */
	const vistas = new Set<{ unmount: () => void }>();

	afterEach(() => {
		for (const vista of vistas) vista.unmount();
		vistas.clear();
	});

	function conPasos(hechos: number, total: number, parcial: number | null = null) {
		const store = useInstalacionStore();
		store.pasosInstalacion = Array.from({ length: total }, (_, i) => `paso${i}`);
		const mapa = new Map();
		for (let i = 0; i < total; i++) {
			const estado = i < hechos ? 'hecho' : i === hechos && parcial !== null ? 'en_curso' : 'pendiente';
			mapa.set(`paso${i}`, { paso: `paso${i}`, estado, fraccion: estado === 'en_curso' ? parcial : null, detalle: null });
		}
		store.progreso = mapa;
		const vista = mount(InstallationView);
		vistas.add(vista);
		return vista;
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

/**
 * Lo que trajo el molde de la plantilla y nunca se usó.
 *
 * `useReactiveIcon()` estaba en `src/composables/` desde el primer commit y
 * **ningún archivo lo importaba**: el instalador escribió `useIcono` en su lugar
 * —con caché compartida— y dejó el original ahí. La única mención que quedaba
 * era un comentario explicando por qué no se usa.
 *
 * Lo que lo hace peligroso no es que ocupe lugar: es que está disponible. Un
 * composable muerto no se ve raro —se lee como una pieza de la casa— y el
 * primero que necesite un icono lo va a usar. Así terminó habiendo una copia
 * distinta en cada repositorio del taller: al contarlas quedaban nueve, con
 * cinco firmas que ya no son intercambiables (Vasak-OS/vue-libvasak#54).
 */
describe('el composable de iconos del molde', () => {
	// `fileURLToPath` y no `.pathname`: éste deja los caracteres codificados tal
	// como están, así que un checkout en una ruta con un espacio llega con `%20`
	// y `scanSync` no encuentra nada.
	const FUENTE = fileURLToPath(new URL('../src/', import.meta.url));

	// El patrón se ancla en `src/`, así que las rutas vuelven relativas a ahí.
	const fuentes = [...new Glob('**/*.{vue,ts}').scanSync(FUENTE)];

	test('hay algo que mirar', () => {
		// Sin esto las dos de abajo pasan sobre una lista vacía, que es en lo que
		// quedan si el patrón deja de encontrar archivos.
		expect(fuentes).toContain('App.vue');
		expect(fuentes.length).toBeGreaterThan(10);
	});

	test('ya no está', () => {
		expect(fuentes.filter((ruta) => ruta.includes('useReactiveIcon'))).toEqual([]);
	});

	test('y nadie escribió otro con otro nombre — ya sin excepciones', async () => {
		// La forma de la copia y no su nombre: suscribirse al cambio de tema.
		// `useIcono` era la excepción declarada acá y se fue con el #63, así que
		// ya no queda ningún lugar del instalador que escuche ese aviso: lo hace
		// la librería, una sola vez para toda la ventana.
		const conOyente: string[] = [];
		for (const ruta of fuentes) {
			const texto = await Bun.file(join(FUENTE, ruta)).text();
			if (texto.includes('vicons:theme-changed')) conOyente.push(ruta);
		}

		expect(conOyente).toEqual([]);
	});

	test('y quien resuelve a mano es sólo el que no puede hacerlo de otra forma', async () => {
		// El otro lado de lo mismo: importar el complemento de iconos. El
		// oyente se puede escribir sin nombrar el evento, pero la fuente del
		// icono no se consigue sin pedírsela a alguien.
		//
		// `main.ts` es la única excepción y no puede dejar de serlo: el menú del
		// clic derecho lo dibuja el complemento **fuera de esta ventana**, así
		// que pide una función que resuelva un nombre a una fuente, no un
		// componente de Vue. Es la misma excepción que tiene `vasak-desktop`.
		const aMano: string[] = [];
		for (const ruta of fuentes) {
			const texto = await Bun.file(join(FUENTE, ruta)).text();
			if (texto.includes("from '@vasakgroup/plugin-vicons'")) aMano.push(ruta);
		}

		expect(aMano).toEqual(['main.ts']);
	});
});

/**
 * Las piezas dibujadas a mano que ya son de la librería (vue-libvasak#74).
 *
 * `PageHeader`, `SectionCard`, `OpcionRadio`, `SystemIcon`, `AlertMessage` y
 * `PasoBoton` eran copias de lo que la librería ya tiene: la cabecera con el
 * icono, la tarjeta de sección (`ConfigSection`, o `Panel` sin título), el grupo
 * de opciones en tarjeta (`OptionGroup variant="card"`), `ThemeIcon`, el aviso
 * con su icono por tono y `SideButton` con `IconTile`. Una copia al lado de la
 * biblioteca que hace lo mismo es una copia que se va a separar.
 */
/** El texto sin los comentarios de HTML, cortando por `<!--` y `-->`. */
function withoutComments(text: string): string {
	let out = '';
	let index = 0;
	while (index < text.length) {
		const open = text.indexOf('<!--', index);
		if (open === -1) return out + text.slice(index);
		out += text.slice(index, open);
		const close = text.indexOf('-->', open + 4);
		if (close === -1) return out;
		index = close + 3;
	}
	return out;
}

describe('las piezas propias', () => {
	const FUENTE = fileURLToPath(new URL('../src/', import.meta.url));
	const fuentes = [...new Glob('**/*.{vue,ts}').scanSync(FUENTE)];

	test('ya no están', () => {
		const viejas = ['PageHeader', 'SectionCard', 'OpcionRadio', 'SystemIcon', 'AlertMessage', 'PasoBoton', 'PasosSidebar'];
		expect(fuentes.filter((ruta) => viejas.some((nombre) => ruta.endsWith(`/${nombre}.vue`)))).toEqual([]);
		expect(fuentes.filter((ruta) => ruta.startsWith('components/ui/'))).toEqual([]);
	});

	test('y nadie las importa por su nombre viejo', async () => {
		for (const ruta of fuentes) {
			const texto = await Bun.file(join(FUENTE, ruta)).text();
			expect(texto, ruta).not.toContain("from '@/components/ui/");
			expect(texto, ruta).not.toMatch(/<(?:SectionCard|OpcionRadio|SystemIcon|PasoBoton)\b/);
		}
	});

	test('ni quedó un control nativo dibujado a mano', async () => {
		// El `<select>` y la casilla del modo manual eran nativos con clases
		// propias; ahora son `SelectField` y `Checkbox`. Casilla donde había
		// casilla.
		for (const ruta of fuentes.filter((r) => r.endsWith('.vue'))) {
			const texto = await Bun.file(join(FUENTE, ruta)).text();
			// Sin los comentarios: varios nombran el `<select>` que había. Se
			// cortan por delimitadores y no con un reemplazo de expresión
			// regular, que puede dejar un `<!--` armado con los pedazos.
			const plantilla = withoutComments(texto.slice(texto.indexOf('<template>')));
			expect(plantilla, ruta).not.toMatch(/<select\b|<input\b|<button\b/);
		}
	});
});
