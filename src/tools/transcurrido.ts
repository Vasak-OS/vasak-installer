/**
 * Cuánto viene tardando la instalación.
 *
 * Existe porque la barra de avance **se queda quieta** durante los pasos largos.
 * `pacstrap` puede tardar veinte minutos y no informa fracción, así que el avance
 * general no se mueve ni un píxel: alguien mirando eso no tiene forma de
 * distinguir «está trabajando» de «se colgó».
 *
 * Un reloj que corre es la respuesta honesta a eso. No inventa progreso —no dice
 * cuánto falta, porque no se sabe— pero se mueve cada segundo y el movimiento
 * viene de algo real. Una animación que finge avance sería peor que la barra
 * quieta: mentiría con más convicción.
 *
 * Vive aparte del componente para poder probarlo: el formateo de tiempos tiene
 * casos que se rompen callados —el minuto 60, el segundo 0— y quedan raros sin
 * que nadie los note.
 */

/**
 * Un lapso en segundos, como lo lee una persona.
 *
 * Sin horas hasta que hacen falta: «94:03» para una instalación de hora y media
 * es más difícil de leer que «1 h 34 min», y una instalación normal no llega ni a
 * la primera hora.
 *
 * Los segundos se muestran sólo durante el primer minuto. Después dejan de
 * aportar —a nadie le importa el segundo 43 del minuto 12— y encima hacen que el
 * texto cambie de ancho todo el tiempo, que es justo el tipo de movimiento que
 * distrae en lugar de informar.
 */
export function comoLapso(segundos: number): string {
	// Negativo no debería pasar, pero un reloj del sistema que se ajusta hacia
	// atrás durante la instalación lo produce. Mostrar «-3 s» hace dudar de todo
	// lo demás que dice la pantalla.
	const s = Math.max(0, Math.floor(segundos));

	if (s < 60) return `${s} s`;

	const minutos = Math.floor(s / 60);
	if (minutos < 60) return `${minutos} min`;

	const horas = Math.floor(minutos / 60);
	const resto = minutos % 60;
	// Sin el resto cuando es cero: «2 h» y no «2 h 0 min».
	return resto === 0 ? `${horas} h` : `${horas} h ${resto} min`;
}

/**
 * Cuántos segundos pasaron entre dos marcas de tiempo en milisegundos.
 *
 * Separado del formateo para que el componente no tenga que hacer la división, y
 * para poder probar el caso del reloj que va hacia atrás sin simular un reloj.
 */
export function segundosDesde(inicio: number, ahora: number): number {
	return Math.max(0, Math.floor((ahora - inicio) / 1000));
}
