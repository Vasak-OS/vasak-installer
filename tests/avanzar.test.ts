/**
 * Cuándo se puede avanzar de paso.
 *
 * Es la lógica que decide si el botón «Continuar» está disponible, y por lo
 * tanto lo que impide llegar al punto sin retorno con datos incompletos. Un
 * error acá no se ve: el botón queda habilitado, la instalación arranca, y falla
 * a los veinte minutos con el disco ya formateado.
 *
 * Se prueba el almacén directo, sin montar componentes: lo que importa son las
 * condiciones, no cómo se dibujan.
 */

import { beforeEach, describe, expect, test } from 'bun:test';
import { createPinia, setActivePinia } from 'pinia';
import { useInstalacionStore } from '../src/stores/instalacion';

function discoDe(ruta: string, enUso = false) {
	return {
		ruta,
		modelo: 'Disco de prueba',
		tamano_bytes: 256 * 1024 ** 3,
		sector_logico: 512,
		rotacional: false,
		nvme: false,
		en_uso: enUso,
		particiones: [],
	};
}

/** Un almacén con todo completo, para ir rompiéndolo de a un campo. */
function almacenCompleto() {
	const store = useInstalacionStore();
	store.sistema = {
		firmware: 'uefi',
		memoria_bytes: 8 * 1024 ** 3,
		cpu: 'CPU de prueba',
		nucleos: 4,
		hay_red: true,
		virtualizacion: null,
	};
	store.discos = [discoDe('/dev/sda')];
	store.ayudanteListo = true;

	store.eleccion.zonaHoraria = 'America/Argentina/Buenos_Aires';
	store.eleccion.idiomaSistema = 'es_AR';
	store.eleccion.teclado = 'la-latin1';
	store.eleccion.disco = '/dev/sda';
	store.eleccion.usuario = 'pato';
	store.eleccion.hostname = 'vasak';
	store.secretos.usuario = 'una contraseña';
	store.secretos.usuarioRepetida = 'una contraseña';
	return store;
}

beforeEach(() => {
	setActivePinia(createPinia());
});

describe('paso de red', () => {
	test('sin conexión no se puede avanzar', () => {
		const store = almacenCompleto();
		store.sistema = { ...store.sistema!, hay_red: false };
		// La instalación baja todo de los repositorios. Dejar avanzar sin red
		// sólo posterga el fracaso hasta después de haber formateado el disco.
		expect(store.puedeAvanzar('red')).toBe(false);
	});

	test('con conexión sí', () => {
		expect(almacenCompleto().puedeAvanzar('red')).toBe(true);
	});

	test('antes del sondeo tampoco', () => {
		const store = useInstalacionStore();
		// `sistema` en null es «todavía no se sabe», y no se puede tratar como
		// «hay red».
		expect(store.puedeAvanzar('red')).toBe(false);
	});
});

describe('paso del disco', () => {
	test('sin disco elegido no se puede avanzar', () => {
		const store = almacenCompleto();
		store.eleccion.disco = '';
		expect(store.puedeAvanzar('disco')).toBe(false);
	});

	test('un disco en uso no habilita el paso', () => {
		const store = almacenCompleto();
		store.discos = [discoDe('/dev/sda', true)];
		// Suele ser el medio del que se arrancó. Es la última barrera antes de
		// que el resumen ofrezca borrarlo.
		expect(store.puedeAvanzar('disco')).toBe(false);
	});

	test('un disco demasiado chico no habilita el paso', () => {
		const store = almacenCompleto();
		// 16 GiB: el más grande de los que no están en uso, así que queda
		// preseleccionado. Antes el botón quedaba habilitado y el rechazo llegaba
		// recién al apretar Instalar, cuando el backend lo rechaza por tamaño.
		store.discos = [{ ...discoDe('/dev/sda'), tamano_bytes: 16 * 1024 ** 3 }];
		expect(store.puedeAvanzar('disco')).toBe(false);

		// Justo en el mínimo sí.
		store.discos = [{ ...discoDe('/dev/sda'), tamano_bytes: 20 * 1024 ** 3 }];
		expect(store.puedeAvanzar('disco')).toBe(true);
	});

	test('un disco que ya no está en la lista no habilita el paso', () => {
		const store = almacenCompleto();
		// Alguien desconectó el disco externo que había elegido.
		store.discos = [discoDe('/dev/sdb')];
		expect(store.puedeAvanzar('disco')).toBe(false);
	});

	test('con cifrado hacen falta las dos frases y tienen que coincidir', () => {
		const store = almacenCompleto();
		store.eleccion.cifrar = true;
		expect(store.puedeAvanzar('disco')).toBe(false);

		store.secretos.cifrado = 'frase del disco';
		// Sólo una de las dos: una frase mal tipeada y confirmada una sola vez es
		// un disco que no se puede abrir nunca más.
		expect(store.puedeAvanzar('disco')).toBe(false);

		store.secretos.cifradoRepetida = 'frase del disc';
		expect(store.puedeAvanzar('disco')).toBe(false);

		store.secretos.cifradoRepetida = 'frase del disco';
		expect(store.puedeAvanzar('disco')).toBe(true);
	});

	test('sin cifrado la frase no se mira', () => {
		const store = almacenCompleto();
		store.secretos.cifrado = 'sobrante';
		store.secretos.cifradoRepetida = '';
		expect(store.puedeAvanzar('disco')).toBe(true);
	});
});

describe('paso de la cuenta', () => {
	test('con todo completo se puede avanzar', () => {
		expect(almacenCompleto().puedeAvanzar('cuenta')).toBe(true);
	});

	test('contraseñas distintas lo impiden', () => {
		const store = almacenCompleto();
		store.secretos.usuarioRepetida = 'otra cosa';
		expect(store.puedeAvanzar('cuenta')).toBe(false);
	});

	test('una contraseña vacía lo impide aunque las dos coincidan', () => {
		const store = almacenCompleto();
		store.secretos.usuario = '';
		store.secretos.usuarioRepetida = '';
		// Dos vacías «coinciden». Sin la comprobación de largo, esto pasaba y la
		// cuenta quedaba sin contraseña.
		expect(store.puedeAvanzar('cuenta')).toBe(false);
	});

	test('sin nombre de equipo no se puede avanzar', () => {
		const store = almacenCompleto();
		store.eleccion.hostname = '';
		expect(store.puedeAvanzar('cuenta')).toBe(false);
	});

	test('con root habilitado hacen falta sus dos contraseñas', () => {
		const store = almacenCompleto();
		store.eleccion.rootHabilitado = true;
		expect(store.puedeAvanzar('cuenta')).toBe(false);

		store.secretos.root = 'clave de root';
		store.secretos.rootRepetida = 'clave de root';
		expect(store.puedeAvanzar('cuenta')).toBe(true);
	});

	test('con root deshabilitado sus contraseñas no se miran', () => {
		const store = almacenCompleto();
		store.eleccion.rootHabilitado = false;
		store.secretos.root = '';
		expect(store.puedeAvanzar('cuenta')).toBe(true);
	});
});

describe('paso del resumen', () => {
	test('sin el ayudante privilegiado no se puede instalar', () => {
		const store = almacenCompleto();
		store.ayudanteListo = false;
		// Es el botón que arranca el punto sin retorno: habilitarlo sin
		// autorización produce un error justo después de apretarlo, que es el
		// peor momento posible para descubrirlo.
		expect(store.puedeAvanzar('resumen')).toBe(false);
	});

	test('con el ayudante listo sí', () => {
		expect(almacenCompleto().puedeAvanzar('resumen')).toBe(true);
	});
});

describe('los secretos', () => {
	test('olvidarSecretos deja todos los campos vacíos', () => {
		const store = almacenCompleto();
		store.secretos.root = 'clave de root';
		store.secretos.cifrado = 'frase';
		store.secretos.cifradoRepetida = 'frase';

		store.olvidarSecretos();

		// Los seis, no sólo los dos principales: uno que se olvide de limpiar es
		// una contraseña que sigue en memoria del WebView durante toda la
		// instalación.
		for (const valor of Object.values(store.secretos)) {
			expect(valor).toBe('');
		}
	});

	test('el plan lleva cadena vacía y no undefined cuando el secreto no aplica', () => {
		const store = almacenCompleto();
		store.eleccion.rootHabilitado = false;
		store.eleccion.cifrar = false;

		const plan = store.armarPlan();
		// El backend espera los tres campos siempre. Un campo ausente hace fallar
		// la deserialización con un error que no dice cuál falta.
		expect(plan.secretos.root).toBe('');
		expect(plan.secretos.cifrado).toBe('');
		expect(plan.secretos.usuario).toBe('una contraseña');
	});
});

describe('los complementos', () => {
	function conCatalogo() {
		const store = almacenCompleto();
		store.complementos = {
			catalogo: [
				{ id: 'firefox', categoria: 'navegador', paquetes: ['firefox'], servicios: [], icono: 'firefox', detectar: null, exclusivo: true, por_defecto: true },
				{ id: 'chromium', categoria: 'navegador', paquetes: ['chromium'], servicios: [], icono: 'chromium', detectar: null, exclusivo: true, por_defecto: false },
				{ id: 'impresoras', categoria: 'impresoras', paquetes: ['cups'], servicios: ['cups.socket'], icono: 'printer', detectar: null, exclusivo: false, por_defecto: false },
				{ id: 'juegos', categoria: 'extras', paquetes: ['steam'], servicios: [], icono: 'input-gaming', detectar: null, exclusivo: false, por_defecto: false },
			],
			categorias: ['navegador', 'impresoras', 'extras'],
			hardware: { marcas: [], descripciones: [] },
			preseleccion: ['firefox'],
			error: null,
		};
		store.eleccion.complementos = ['firefox'];
		return store;
	}

	test('siempre se puede pasar el paso: todo es opcional', () => {
		const store = useInstalacionStore();
		expect(store.puedeAvanzar('complementos')).toBe(true);
	});

	test('elegir otro navegador saca al anterior', () => {
		const store = conCatalogo();
		store.alternarComplemento('chromium');
		// Dos navegadores marcados a la vez es un estado que el grupo de opciones
		// no puede dibujar, y el plan instalaría los dos.
		expect(store.eleccion.complementos).toEqual(['chromium']);
	});

	test('volver a apretar el navegador elegido no lo desmarca', () => {
		const store = conCatalogo();
		store.alternarComplemento('firefox');
		// En un grupo del que se elige uno, no existe «ninguno»: para eso está la
		// opción explícita «Ninguno» del catálogo.
		expect(store.eleccion.complementos).toEqual(['firefox']);
	});

	test('los que no son excluyentes se marcan y se desmarcan', () => {
		const store = conCatalogo();
		store.alternarComplemento('impresoras');
		expect(store.eleccion.complementos).toContain('impresoras');
		store.alternarComplemento('impresoras');
		expect(store.eleccion.complementos).not.toContain('impresoras');
		// Y no se llevó puesto al navegador.
		expect(store.eleccion.complementos).toContain('firefox');
	});

	test('el orden es el del catálogo y no el de marcado', () => {
		const store = conCatalogo();
		store.alternarComplemento('juegos');
		store.alternarComplemento('impresoras');
		// Así el resumen enumera siempre igual y dos instalaciones con la misma
		// elección se pueden comparar.
		expect(store.eleccion.complementos).toEqual(['firefox', 'impresoras', 'juegos']);
	});

	test('un id que no está en el catálogo no hace nada', () => {
		const store = conCatalogo();
		store.alternarComplemento('no-existe');
		expect(store.eleccion.complementos).toEqual(['firefox']);
	});

	test('el plan lleva los elegidos', () => {
		const store = conCatalogo();
		store.alternarComplemento('impresoras');
		expect(store.armarPlan().complementos).toEqual(['firefox', 'impresoras']);
	});
});

describe('el registro', () => {
	test('no crece sin límite', () => {
		const store = useInstalacionStore();
		// `pacstrap` escribe una línea por paquete y son más de mil. Sin el tope,
		// Vue re-renderiza una lista de miles de nodos en cada línea nueva y la
		// ventana se traba justo cuando lo único que puede hacer es mostrar
		// progreso.
		for (let i = 0; i < 1200; i++) {
			store.anotarRegistro({ nivel: 'info', linea: `línea ${i}` });
		}
		expect(store.registro.length).toBe(500);
		// Y lo que se conserva es el final, que es donde dice qué falló.
		expect(store.registro[store.registro.length - 1].linea).toBe('línea 1199');
	});
});
