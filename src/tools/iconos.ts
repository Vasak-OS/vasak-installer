/**
 * Qué símbolo representa cada cosa del instalador.
 *
 * En un solo lugar porque los mismos pasos se dibujan en dos: la barra lateral
 * del asistente y la lista de progreso de la instalación. Con el nombre escrito
 * en cada componente, cambiar un icono era acordarse de los dos.
 *
 * Los nombres son los del tema (`vasakos-icon-theme`) **sin el sufijo
 * `-symbolic`**: el plugin resuelve por búsqueda de GTK con `FORCE_SYMBOLIC` y
 * lo agrega él. Un nombre que el tema no tenga no rompe nada, pero deja el hueco
 * vacío, que se ve peor que no haber puesto icono — hay un test que verifica que
 * todos los de acá existan.
 */

import type { Paso } from '@/stores/instalacion';

/** Los pasos del asistente. */
export const ICONO_PASO: Record<Paso, string> = {
	bienvenida: 'help-about',
	red: 'network-wireless',
	region: 'globe',
	teclado: 'input-keyboard',
	disco: 'drive-harddisk',
	cuenta: 'avatar-default',
	complementos: 'packages-app',
	resumen: 'document-properties',
	instalacion: 'system-software-install',
	fin: 'object-select',
};

/**
 * Los pasos de la instalación, que son los del backend (`Paso::clave()`) y no
 * los del asistente.
 */
export const ICONO_PASO_INSTALACION: Record<string, string> = {
	particionar: 'drive-multidisk',
	montar: 'drive-harddisk',
	espejos: 'network-server',
	sistemaBase: 'package-x-generic',
	escritorio: 'preferences-desktop-appearance',
	arranque: 'system-shutdown',
	usuarios: 'system-users',
	configuracion: 'preferences-system',
	vasakos: 'starred',
	cierre: 'object-select',
};

/**
 * El icono de la aplicación: el de la barra de título y el de la entrada del
 * menú.
 *
 * `system-os-installer` y no `system-software-install`: el segundo es el
 * instalador **de programas** —una tienda de aplicaciones—, y esto instala el
 * sistema operativo. Los dos existen en el tema y se parecen lo suficiente como
 * para que la confusión no se note hasta que alguien busca uno de los dos.
 *
 * Va acá y no escrito en la plantilla para que lo cubra el test que verifica
 * contra el tema instalado. La entrada `.desktop` nombra el mismo, pero esa
 * copia no la puede comprobar este módulo: la comprueba `iconos.test.ts`.
 */
export const ICONO_APLICACION = 'system-os-installer';

/**
 * La flecha del desplegable, en el selector buscable.
 *
 * `pan-down` es el nombre que GTK usa para exactamente esta flecha —la de un
 * combo o un expansor— y el tema lo tiene. Acá había un `▾` escrito a mano: un
 * carácter de texto no sigue el tema de iconos, se dibuja con la tipografía que
 * haya y cambia de forma y de peso entre una fuente y otra.
 */
export const ICONO_DESPLEGABLE = 'pan-down';

/** Marca de paso terminado, y de paso que falló. */
export const ICONO_HECHO = 'object-select';
export const ICONO_FALLADO = 'dialog-error';

/**
 * El icono de un disco.
 *
 * Se nombra el icono **semánticamente correcto** aunque hoy el tema dibuje lo
 * mismo para varios: `drive-harddisk-solidstate` es un enlace a
 * `drive-harddisk`, así que un SSD y un disco mecánico se ven idénticos. Nombrar
 * el correcto no cuesta nada y el día que el tema los distinga, funciona solo.
 *
 * Lo que **no** se hace es forzar una diferencia visual con un icono de otra
 * cosa. Un NVMe con `media-flash` mostraba una tarjeta SD —ese nombre es un
 * enlace a `gnome-dev-media-sdmmc`— y un SSD con `drive-multidisk` mostraba una
 * pila de discos de RAID. En la pantalla donde se elige qué disco formatear, un
 * icono que miente sobre qué dispositivo es resulta peor que uno repetido: la
 * diferencia entre NVMe, SSD y mecánico ya está escrita al lado, en texto.
 *
 * Tampoco se dibuja un icono de USB adivinando por la ruta: `lsblk` no informa
 * el transporte en el sondeo, y marcar como extraíble un disco interno —o al
 * revés— es la clase de error que nadie perdona acá.
 */
export function iconoDeDisco(disco: { nvme: boolean; rotacional: boolean }): string {
	if (disco.rotacional) return 'drive-harddisk';
	// Un NVMe es un disco de estado sólido; el tema no tiene un icono propio para
	// NVMe y `drive-harddisk-nvme` no existe.
	return 'drive-harddisk-solidstate';
}

/** El rol de una partición en la vista previa del particionado. */
export const ICONO_ROL_PARTICION: Record<string, string> = {
	esp: 'system-shutdown',
	raiz: 'drive-harddisk',
};

/** Los mensajes. */
export const ICONO_MENSAJE = {
	info: 'dialog-information',
	aviso: 'dialog-warning',
	error: 'dialog-error',
	exito: 'object-select',
} as const;

/**
 * Las filas del resumen del equipo, en la bienvenida.
 *
 * `am-memory` y no `media-flash` para la memoria: `media-flash` es una tarjeta
 * SD —en este tema, un enlace a `gnome-dev-media-sdmmc`— y la memoria RAM no es
 * una tarjeta de cámara. Es el mismo error que tenía el icono del disco NVMe.
 * `am-*` es el juego que usa vasak-monitor para estas mismas magnitudes.
 */
export const ICONO_EQUIPO = {
	procesador: 'am-cpu',
	memoria: 'am-memory',
	firmware: 'preferences-system-details',
	virtualizacion: 'computer',
} as const;

/** Todo lo de este módulo, para el test que verifica que el tema los tenga. */
export function todosLosIconos(): string[] {
	return [
		...Object.values(ICONO_PASO),
		...Object.values(ICONO_PASO_INSTALACION),
		...Object.values(ICONO_ROL_PARTICION),
		...Object.values(ICONO_MENSAJE),
		...Object.values(ICONO_EQUIPO),
		ICONO_HECHO,
		ICONO_FALLADO,
		ICONO_APLICACION,
		ICONO_DESPLEGABLE,
		iconoDeDisco({ nvme: true, rotacional: false }),
		iconoDeDisco({ nvme: false, rotacional: true }),
		iconoDeDisco({ nvme: false, rotacional: false }),
	];
}
