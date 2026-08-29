import { describe, expect, test } from 'bun:test';
import { formatearBytes, nombreDeIdioma, nombreDeZona } from '../src/tools/formato';

describe('formatearBytes', () => {
	test('usa unidades binarias, que son las que muestra el sistema instalado', () => {
		// Un disco que el fabricante vende como «500 GB» tiene 500·10⁹ bytes, y
		// el sistema lo va a mostrar como 465 GiB. El instalador dice el mismo
		// número que la persona va a ver después; usar GB acá haría que el
		// instalador y el sistema instalado se contradigan.
		expect(formatearBytes(500 * 10 ** 9, 'en')).toBe('465.7 GiB');
		expect(formatearBytes(1024 ** 3, 'en')).toBe('1.0 GiB');
		expect(formatearBytes(1024 ** 4, 'en')).toBe('1.0 TiB');
	});

	test('no pone decimales donde no significan nada', () => {
		// «1,5 B» no quiere decir nada, y «512,0 KiB» tampoco aporta.
		expect(formatearBytes(512, 'en')).toBe('512 B');
		expect(formatearBytes(2048, 'en')).toBe('2 KiB');
		// Del mebibyte para arriba el decimal sí distingue un disco de otro.
		expect(formatearBytes(1536 * 1024, 'en')).toBe('1.5 MiB');
	});

	test('respeta el separador decimal del idioma', () => {
		// El formato se arma en el frontend justamente por esto: hecho en Rust,
		// una interfaz en inglés mostraría «465,7 GiB» con coma.
		expect(formatearBytes(1536 * 1024, 'es-AR')).toBe('1,5 MiB');
		expect(formatearBytes(1536 * 1024, 'en-US')).toBe('1.5 MiB');
	});

	test('un valor imposible no rompe la interfaz', () => {
		// `lsblk` puede informar tamaño cero para un lector de tarjetas vacío, y
		// un campo ausente llega como NaN. Ninguno de los dos puede dejar la
		// tarjeta del disco mostrando «NaN undefined».
		expect(formatearBytes(0, 'en')).toBe('0 B');
		expect(formatearBytes(Number.NaN, 'en')).toBe('—');
		expect(formatearBytes(-1, 'en')).toBe('—');
	});

	test('no se pasa de la unidad más grande que conoce', () => {
		// Sin el tope, el índice se sale del arreglo y la unidad sale
		// `undefined`.
		expect(formatearBytes(1024 ** 7, 'en')).toContain('PiB');
	});
});

describe('nombreDeZona', () => {
	test('se queda con la ciudad y deja la región aparte', () => {
		expect(nombreDeZona('America/Argentina/Buenos_Aires')).toEqual({
			region: 'America',
			ciudad: 'Buenos Aires',
		});
		expect(nombreDeZona('Europe/Madrid')).toEqual({ region: 'Europe', ciudad: 'Madrid' });
	});

	test('el guion bajo se convierte en espacio', () => {
		// Es el separador del archivo de zonas, no algo que alguien quiera leer.
		expect(nombreDeZona('America/Sao_Paulo').ciudad).toBe('Sao Paulo');
	});

	test('una zona sin barra no rompe', () => {
		expect(nombreDeZona('UTC')).toEqual({ region: 'UTC', ciudad: 'UTC' });
	});
});

describe('nombreDeIdioma', () => {
	test('traduce el local al idioma de la interfaz', () => {
		expect(nombreDeIdioma('es_AR', 'es').toLowerCase()).toContain('español');
		expect(nombreDeIdioma('fr_FR', 'en').toLowerCase()).toContain('french');
	});

	test('convierte el guion bajo de glibc a la etiqueta BCP 47', () => {
		// Los locales del sistema vienen como `es_AR`, y `Intl.DisplayNames`
		// **tira RangeError** con eso en vez de devolver undefined. Sin la
		// conversión, la lista de idiomas entera fallaba.
		expect(() => nombreDeIdioma('es_AR', 'en')).not.toThrow();
		expect(nombreDeIdioma('pt_BR', 'en')).not.toBe('pt_BR');
	});

	test('un local que Intl no conoce vuelve tal cual', () => {
		// Un código es peor que un nombre y muchísimo mejor que una excepción o
		// una fila vacía en el desplegable.
		expect(nombreDeIdioma('xx_YY', 'en')).toBe('xx_YY');
		expect(nombreDeIdioma('no es un local', 'en')).toBe('no es un local');
	});
});
