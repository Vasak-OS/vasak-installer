/**
 * Iconos del tema del sistema, con caché compartida entre componentes.
 *
 * El tema tiene **dos versiones de casi todo**: la de color, en `scalable/`, que
 * es el icono que el escritorio muestra en todas partes; y la simbólica, un
 * glifo monocromo pensado para barras y listas densas. El plugin las resuelve
 * con dos llamadas distintas, y elegir mal es lo que hace que Firefox aparezca
 * como un contorno gris en vez del logo de Firefox.
 *
 * La regla del instalador:
 *
 * - **`icono`** donde el icono **es** la cosa: los navegadores, los discos, la
 *   impresora, la placa de video. Ahí se reconoce por su forma y su color, y un
 *   glifo monocromo lo vuelve irreconocible.
 * - **`simbolo`** donde el icono es una marca de estado o de navegación: los
 *   pasos de la barra lateral, el tilde de terminado, el aviso de un mensaje.
 *   Ahí lo que importa es que se lea a 16 píxeles y que siga al color del texto.
 *
 * ⚠️ Pedir `icono` para un nombre que sólo existe en simbólico **no falla**: el
 * plugin devuelve `image-missing`, o sea el icono de imagen rota. Por eso hay un
 * test que verifica que cada nombre pedido a color tenga su versión a color.
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
import { getIconSource, getSymbolSource } from '@vasakgroup/plugin-vicons';
import { onUnmounted, type Ref, ref, watch } from 'vue';

/** Cuál de las dos versiones del tema se pide. */
export type TipoIcono = 'icono' | 'simbolo';

/**
 * `tipo:nombre` → la promesa de su fuente `data:`.
 *
 * El tipo va en la clave: `firefox` a color y `firefox` simbólico son dos
 * archivos distintos, y con la clave sin el tipo el segundo que pidiera se
 * llevaba el del primero.
 */
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

function resolver(nombre: string, tipo: TipoIcono): Promise<string> {
	const clave = `${tipo}:${nombre}`;
	const enCache = cache.get(clave);
	if (enCache) return enCache;

	const traer = tipo === 'icono' ? getIconSource : getSymbolSource;
	const promesa = traer(nombre).catch((error) => {
		console.error(`no se pudo resolver «${nombre}» (${tipo})`, error);
		// La entrada fallida se saca de la caché: un tema a medio instalar no
		// puede dejar ese icono roto para siempre.
		cache.delete(clave);
		return '';
	});

	cache.set(clave, promesa);
	return promesa;
}

/**
 * La fuente `data:` de un icono, reactiva al nombre, al tipo y al tema.
 *
 * Devuelve cadena vacía mientras resuelve y si falla; quien lo use tiene que
 * poder dibujarse sin icono, que es lo que evita el salto de disposición cuando
 * el tema no tiene ese nombre.
 */
export function useIcono(
	nombre: () => string,
	tipo: () => TipoIcono = () => 'simbolo'
): Ref<string> {
	armarOyente();
	const fuente = ref('');
	// Contador para descartar respuestas viejas: si el nombre cambia dos veces
	// seguidas, la primera respuesta puede llegar después de la segunda y dejar
	// el icono equivocado puesto.
	let token = 0;
	let vivo = true;

	watch(
		[nombre, tipo, version],
		async ([actual, tipoActual]) => {
			// El token sube **antes** de cualquier salida, incluida la de nombre
			// vacío. Saliendo sin subirlo, una resolución que seguía pendiente
			// cumplía después `mio === token` y volvía a poner su icono: el
			// componente terminaba mostrando el icono de un nombre que ya no
			// tiene.
			const mio = ++token;
			if (!actual) {
				fuente.value = '';
				return;
			}
			const resuelto = await resolver(actual, tipoActual);
			if (vivo && mio === token) fuente.value = resuelto;
		},
		{ immediate: true }
	);

	onUnmounted(() => {
		vivo = false;
	});

	return fuente;
}
