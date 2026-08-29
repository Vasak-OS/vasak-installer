/**
 * Símbolos del tema de iconos, con caché compartida entre componentes.
 *
 * `useReactiveIcon` del template resuelve un icono por instancia de componente.
 * Alcanza cuando hay tres o cuatro; acá hay cerca de cuarenta —nueve pasos en la
 * barra lateral, diez en la lista de instalación, uno por disco, uno por
 * mensaje— y varios se repiten. Sin caché, abrir el instalador dispara cuarenta
 * llamadas por IPC al arrancar, cada una con su ida y vuelta al backend, justo
 * mientras se está sondeando el equipo.
 *
 * Con la caché es **una por nombre distinto**, y volver a un paso ya visitado no
 * cuesta nada: el símbolo ya está.
 *
 * La caché guarda la promesa y no el resultado. Es lo que resuelve el caso de
 * dos componentes que piden el mismo icono en el mismo tick —la barra lateral y
 * la lista de pasos piden los mismos diez—: el segundo se cuelga de la promesa
 * del primero en lugar de disparar otra llamada.
 */

import { listen } from '@tauri-apps/api/event';
import { getSymbolSource } from '@vasakgroup/plugin-vicons';
import { onUnmounted, type Ref, ref, watch } from 'vue';

/** Nombre del símbolo → la promesa de su fuente `data:`. */
const cache = new Map<string, Promise<string>>();

/**
 * Sube cuando cambia el tema de iconos.
 *
 * Es un `ref` a nivel de módulo, así que un solo oyente sirve a toda la
 * aplicación: un oyente por componente serían cuarenta suscripciones al mismo
 * evento.
 */
const version = ref(0);
let oyenteArmado = false;

function armarOyente() {
	if (oyenteArmado) return;
	oyenteArmado = true;
	listen('vicons:theme-changed', () => {
		// La caché entera queda vieja: los símbolos del tema nuevo son otros
		// archivos. Vaciarla y subir la versión es lo que hace que todos los
		// componentes vuelvan a pedir.
		cache.clear();
		version.value++;
	}).catch((error) => {
		// Sin el oyente, un cambio de tema no se refleja hasta reiniciar. No es
		// motivo para romper nada, pero tiene que quedar dicho: sin el `catch`
		// esto sería un rechazo de promesa sin manejar.
		console.error('no se pudo escuchar el cambio de tema de iconos', error);
	});
}

function resolver(nombre: string): Promise<string> {
	const enCache = cache.get(nombre);
	if (enCache) return enCache;

	const promesa = getSymbolSource(nombre).catch((error) => {
		console.error(`no se pudo resolver el símbolo «${nombre}»`, error);
		// La entrada fallida se saca de la caché: un error de red o un tema a
		// medio instalar no puede dejar ese icono roto para siempre.
		cache.delete(nombre);
		return '';
	});

	cache.set(nombre, promesa);
	return promesa;
}

/**
 * La fuente `data:` de un símbolo, reactiva al nombre y al tema.
 *
 * Devuelve cadena vacía mientras resuelve y si falla; quien lo use tiene que
 * poder dibujarse sin icono, que es lo que evita el salto de disposición cuando
 * el tema no tiene ese nombre.
 */
export function useSimbolo(nombre: () => string): Ref<string> {
	armarOyente();
	const fuente = ref('');
	// Contador para descartar respuestas viejas: si el nombre cambia dos veces
	// seguidas, la primera respuesta puede llegar después de la segunda y dejar
	// el icono equivocado puesto.
	let token = 0;
	let vivo = true;

	watch(
		[nombre, version],
		async ([actual]) => {
			if (!actual) {
				fuente.value = '';
				return;
			}
			const mio = ++token;
			const resuelto = await resolver(actual);
			if (vivo && mio === token) fuente.value = resuelto;
		},
		{ immediate: true }
	);

	onUnmounted(() => {
		vivo = false;
	});

	return fuente;
}
