/**
 * Formato de los números que ve la persona.
 *
 * Todo lo que sale del backend viene en bytes crudos y se formatea acá, en el
 * frontend, que es el único lado que sabe en qué idioma está la interfaz. Un
 * «1,0 GiB» armado en Rust aparecería con coma decimal en una interfaz en
 * inglés.
 */

/**
 * Bytes en la unidad binaria que corresponde.
 *
 * **GiB y no GB.** Los fabricantes venden en potencias de 10 y el sistema
 * operativo cuenta en potencias de 2, y esa es exactamente la razón por la que
 * un disco de «500 GB» aparece como 465 GiB. Mostrar la unidad correcta no
 * arregla la confusión, pero mentir la empeora: el instalador dice el número que
 * el sistema instalado va a mostrar después.
 *
 * @param locale el idioma para el separador decimal.
 */
export function formatearBytes(bytes: number, locale: string): string {
	const unidades = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
	if (!Number.isFinite(bytes) || bytes < 0) {
		return '—';
	}

	let valor = bytes;
	let unidad = 0;
	while (valor >= 1024 && unidad < unidades.length - 1) {
		valor /= 1024;
		unidad++;
	}

	// Sin decimales en bytes y kibibytes —«1,5 B» no significa nada— y uno solo
	// del mebibyte para arriba, que es donde el decimal sí distingue un disco de
	// otro.
	const decimales = unidad <= 1 ? 0 : 1;
	const numero = new Intl.NumberFormat(locale, {
		minimumFractionDigits: decimales,
		maximumFractionDigits: decimales,
	}).format(valor);

	return `${numero} ${unidades[unidad]}`;
}

/**
 * El nombre legible de una zona horaria: `America/Argentina/Buenos_Aires` →
 * `Buenos Aires`, con la región aparte.
 *
 * Se parte por `/` y se toma el último tramo porque las zonas con país
 * intermedio (`America/Argentina/Cordoba`) tienen tres niveles y el del medio no
 * le dice nada a nadie.
 */
export function nombreDeZona(zona: string): { region: string; ciudad: string } {
	const partes = zona.split('/');
	const ciudad = (partes[partes.length - 1] ?? zona).replaceAll('_', ' ');
	const region = (partes[0] ?? '').replaceAll('_', ' ');
	return { region, ciudad };
}

/**
 * El nombre legible de un local: `es_AR` → `español (Argentina)`.
 *
 * Usa `Intl.DisplayNames`, que está en el motor y sabe los nombres en el idioma
 * de la interfaz. La alternativa era una tabla de doscientas entradas por
 * idioma, que además envejece.
 *
 * Ante un local que `Intl` no conozca devuelve el código tal cual: un código es
 * peor que un nombre pero mucho mejor que nada, y `Intl.DisplayNames` **tira
 * excepción** con una etiqueta mal formada en lugar de devolver undefined.
 */
export function nombreDeIdioma(local: string, locale: string): string {
	// `es_AR` con guion bajo no es una etiqueta BCP 47 válida; la de verdad es
	// `es-AR`. Sin esta conversión, `Intl` tira `RangeError` con todos los
	// locales del sistema, que es como vienen de glibc.
	const etiqueta = local.replaceAll('_', '-');
	try {
		const idiomas = new Intl.DisplayNames([locale], { type: 'language' });
		const nombre = idiomas.of(etiqueta);
		return nombre && nombre !== etiqueta ? nombre : local;
	} catch {
		return local;
	}
}
