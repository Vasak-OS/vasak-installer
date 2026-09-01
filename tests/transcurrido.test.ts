/**
 * El reloj de la instalación.
 *
 * Son cuentas chicas donde el error no se ve: un lapso mal formateado sigue
 * pareciendo un lapso, y el minuto 60 mostrado como «60 min» en lugar de «1 h»
 * nadie lo reporta.
 */

import { describe, expect, test } from 'bun:test';
import { comoLapso, segundosDesde } from '../src/tools/transcurrido';

describe('el lapso', () => {
	test('el primer minuto va en segundos', () => {
		expect(comoLapso(0)).toBe('0 s');
		expect(comoLapso(1)).toBe('1 s');
		expect(comoLapso(59)).toBe('59 s');
	});

	test('desde el minuto se muestran minutos', () => {
		// Los segundos dejan de aportar y hacen que el texto cambie de ancho todo
		// el tiempo, que distrae en lugar de informar.
		expect(comoLapso(60)).toBe('1 min');
		expect(comoLapso(119)).toBe('1 min');
		expect(comoLapso(120)).toBe('2 min');
		expect(comoLapso(59 * 60 + 59)).toBe('59 min');
	});

	test('desde la hora se muestran horas', () => {
		// «94 min» es más difícil de leer que «1 h 34 min».
		expect(comoLapso(60 * 60)).toBe('1 h');
		expect(comoLapso(60 * 60 + 34 * 60)).toBe('1 h 34 min');
		expect(comoLapso(2 * 60 * 60)).toBe('2 h');
	});

	test('las horas exactas no llevan «0 min»', () => {
		expect(comoLapso(3 * 60 * 60)).toBe('3 h');
		expect(comoLapso(3 * 60 * 60 + 60)).toBe('3 h 1 min');
	});

	test('un lapso negativo se muestra como cero', () => {
		// El reloj del sistema puede ajustarse hacia atrás durante la instalación
		// —hay un paso que sincroniza la hora—. Mostrar «-3 s» hace dudar de todo
		// lo demás que dice la pantalla.
		expect(comoLapso(-1)).toBe('0 s');
		expect(comoLapso(-9999)).toBe('0 s');
	});

	test('los decimales se truncan, no se redondean para arriba', () => {
		// Con redondeo, un lapso de 0,6 s mostraría «1 s» antes de que pasara el
		// primer segundo.
		expect(comoLapso(0.9)).toBe('0 s');
		expect(comoLapso(59.9)).toBe('59 s');
	});
});

describe('los segundos transcurridos', () => {
	test('salen de la diferencia en milisegundos', () => {
		expect(segundosDesde(1000, 4000)).toBe(3);
		expect(segundosDesde(0, 90_000)).toBe(90);
	});

	test('un reloj que va hacia atrás da cero', () => {
		expect(segundosDesde(5000, 1000)).toBe(0);
	});

	test('menos de un segundo es cero, no una fracción', () => {
		expect(segundosDesde(0, 999)).toBe(0);
	});
});
