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

/** Marca de paso terminado, y de paso que falló. */
export const ICONO_HECHO = 'object-select';
export const ICONO_FALLADO = 'dialog-error';

/**
 * El icono de un disco.
 *
 * Se distingue lo que se puede distinguir de verdad con lo que informa `lsblk`:
 * NVMe, disco mecánico y el resto. **No** se dibuja un icono de USB adivinando
 * por la ruta: `lsblk` no informa el transporte en el sondeo, y marcar como
 * extraíble un disco interno —o al revés— en la pantalla donde se elige qué
 * formatear es la clase de error que nadie perdona.
 */
export function iconoDeDisco(disco: { nvme: boolean; rotacional: boolean }): string {
	if (disco.nvme) return 'media-flash';
	if (disco.rotacional) return 'drive-harddisk';
	return 'drive-multidisk';
}

/** El sistema de archivos de la raíz. */
export const ICONO_SISTEMA_ARCHIVOS: Record<string, string> = {
	btrfs: 'drive-multidisk',
	ext4: 'drive-harddisk',
	xfs: 'media-flash',
};

/** El rol de una partición en la vista previa del particionado. */
export const ICONO_ROL_PARTICION: Record<string, string> = {
	esp: 'system-shutdown',
	bios_grub: 'system-shutdown',
	raiz: 'drive-harddisk',
};

/** Los mensajes. */
export const ICONO_MENSAJE = {
	info: 'dialog-information',
	aviso: 'dialog-warning',
	error: 'dialog-error',
	exito: 'object-select',
} as const;

/** Las filas del resumen del equipo, en la bienvenida. */
export const ICONO_EQUIPO = {
	procesador: 'computer-chip',
	memoria: 'media-flash',
	firmware: 'preferences-system-details',
	virtualizacion: 'computer',
} as const;

/** Todo lo de este módulo, para el test que verifica que el tema los tenga. */
export function todosLosIconos(): string[] {
	return [
		...Object.values(ICONO_PASO),
		...Object.values(ICONO_PASO_INSTALACION),
		...Object.values(ICONO_SISTEMA_ARCHIVOS),
		...Object.values(ICONO_ROL_PARTICION),
		...Object.values(ICONO_MENSAJE),
		...Object.values(ICONO_EQUIPO),
		ICONO_HECHO,
		ICONO_FALLADO,
		iconoDeDisco({ nvme: true, rotacional: false }),
		iconoDeDisco({ nvme: false, rotacional: true }),
		iconoDeDisco({ nvme: false, rotacional: false }),
	];
}
