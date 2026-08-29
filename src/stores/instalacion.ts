/**
 * El estado del asistente: lo que la persona fue eligiendo, y en qué paso está.
 *
 * Un solo almacén para todo el asistente y no uno por paso, porque la pantalla
 * de resumen necesita todo junto y el plan que se le manda al backend es un
 * único objeto. Partirlo obligaría a recomponerlo, y ahí es donde un campo se
 * queda atrás.
 *
 * **Las contraseñas viven acá, en memoria del proceso de la ventana, y no se
 * guardan en ninguna parte.** No van al almacén de configuración ni a
 * `localStorage`: se mandan una sola vez al ayudante junto con el plan, que las
 * convierte en hash antes de escribir nada. Al terminar la instalación se
 * limpian con `olvidarSecretos()`.
 */

import { invoke } from '@tauri-apps/api/core';
import { defineStore } from 'pinia';
import { computed, reactive, ref } from 'vue';

/** Los pasos del asistente, en orden. Es la fuente del sidebar. */
export const PASOS = [
	'bienvenida',
	'red',
	'region',
	'teclado',
	'disco',
	'cuenta',
	'complementos',
	'resumen',
	'instalacion',
	'fin',
] as const;

export type Paso = (typeof PASOS)[number];

export type SistemaArchivos = 'btrfs' | 'ext4' | 'xfs';

export interface ParticionExistente {
	ruta: string;
	tamano_bytes: number;
	sistema_archivos: string | null;
	etiqueta: string | null;
	sistema_operativo: string | null;
}

export interface Disco {
	ruta: string;
	modelo: string;
	tamano_bytes: number;
	sector_logico: number;
	rotacional: boolean;
	nvme: boolean;
	en_uso: boolean;
	particiones: ParticionExistente[];
}

export interface Sistema {
	firmware: 'uefi' | 'bios';
	memoria_bytes: number;
	cpu: string;
	nucleos: number;
	hay_red: boolean;
	virtualizacion: string | null;
}

export interface Complemento {
	id: string;
	categoria: 'navegador' | 'impresoras' | 'drivers' | 'extras';
	paquetes: string[];
	servicios: string[];
	icono: string;
	detectar: string | null;
	exclusivo: boolean;
	por_defecto: boolean;
}

export interface Hardware {
	marcas: string[];
	descripciones: string[];
}

export interface Complementos {
	catalogo: Complemento[];
	categorias: string[];
	hardware: Hardware;
	preseleccion: string[];
	error: string | null;
}

export interface Catalogos {
	zonas: string[];
	idiomas: string[];
	teclados: string[];
}

export interface ParticionVistaPrevia {
	rol: 'esp' | 'bios_grub' | 'raiz';
	inicio_bytes: number;
	tamano_bytes: number;
	sistema_archivos: string | null;
	punto_montaje: string | null;
	opciones_montaje: string[];
	subvolumenes: string[];
	cifrada: boolean;
}

export interface VistaPrevia {
	firmware: string;
	particiones: ParticionVistaPrevia[];
	se_pierde: string[];
}

/** Un paso de la instalación, tal como lo informa el backend. */
export interface ProgresoPaso {
	paso: string;
	estado: 'pendiente' | 'en_curso' | 'hecho' | 'fallado';
	fraccion: number | null;
	detalle: string | null;
}

export interface LineaRegistro {
	nivel: 'info' | 'warn' | 'error';
	linea: string;
}

/**
 * Cuántas líneas de registro se conservan.
 *
 * Una instalación produce miles: `pacstrap` escribe una por paquete y hay más de
 * mil. Sin este tope el arreglo crece sin freno y Vue re-renderiza una lista de
 * miles de nodos en cada línea nueva, con lo que la ventana se traba justo
 * cuando lo único que puede hacer es mostrar progreso.
 */
const MAX_REGISTRO = 500;

/**
 * El disco más chico en el que se puede instalar, en GiB.
 *
 * Duplica `MINIMO_GIB` de `layout.rs` a propósito: el backend es el que decide y
 * rechaza, y esto es para poder decirlo **antes** de que alguien llegue al
 * resumen. Si se separan, lo peor que pasa es que el botón quede habilitado y el
 * backend lo rechace con su motivo, que es lo que pasaba antes de existir esta
 * comprobación.
 */
const MINIMO_GIB = 20;

export const useInstalacionStore = defineStore('instalacion', () => {
	// ── Dónde estamos ──────────────────────────────────────────────────────
	const paso = ref<Paso>('bienvenida');
	/**
	 * Hasta qué paso se puede volver.
	 *
	 * Una vez que la instalación arrancó no hay vuelta atrás, y el asistente lo
	 * refleja en vez de dejar el botón puesto y que no haga nada.
	 */
	const navegacionBloqueada = ref(false);

	// ── Lo que se sondeó ───────────────────────────────────────────────────
	const sistema = ref<Sistema | null>(null);
	const discos = ref<Disco[]>([]);
	const catalogos = ref<Catalogos>({ zonas: [], idiomas: [], teclados: [] });
	const vistaPrevia = ref<VistaPrevia | null>(null);
	const complementos = ref<Complementos>({
		catalogo: [],
		categorias: [],
		hardware: { marcas: [], descripciones: [] },
		preseleccion: [],
		error: null,
	});
	const ayudanteListo = ref(false);
	const errorAyudante = ref<string | null>(null);

	// ── Lo que la persona eligió ───────────────────────────────────────────
	const eleccion = reactive({
		zonaHoraria: 'UTC',
		idiomaSistema: 'en_US',
		teclado: 'us',
		ntp: true,

		disco: '',
		sistemaArchivos: 'btrfs' as SistemaArchivos,
		cifrar: false,
		zram: true,

		nombreCompleto: '',
		usuario: '',
		hostname: 'vasak',
		administrador: true,
		rootHabilitado: false,

		/** Ids de los complementos elegidos. Ver `complementos.rs`. */
		complementos: [] as string[],
	});

	/**
	 * Las contraseñas, aparte del resto.
	 *
	 * Separadas para que sea evidente qué no se puede registrar ni serializar, y
	 * para que `olvidarSecretos()` tenga un único lugar que limpiar.
	 */
	const secretos = reactive({
		usuario: '',
		usuarioRepetida: '',
		root: '',
		rootRepetida: '',
		cifrado: '',
		cifradoRepetida: '',
	});

	function olvidarSecretos() {
		secretos.usuario = '';
		secretos.usuarioRepetida = '';
		secretos.root = '';
		secretos.rootRepetida = '';
		secretos.cifrado = '';
		secretos.cifradoRepetida = '';
	}

	// ── El progreso ────────────────────────────────────────────────────────
	const pasosInstalacion = ref<string[]>([]);
	const progreso = ref<Map<string, ProgresoPaso>>(new Map());
	const registro = ref<LineaRegistro[]>([]);
	const terminada = ref(false);
	const fallo = ref<string | null>(null);

	function anotarProgreso(p: ProgresoPaso) {
		// Un `Map` nuevo y no `map.set` sobre el mismo: Vue no rastrea las
		// mutaciones de un `Map` dentro de un `ref` sin `reactive`, y el estado
		// cambiaba sin que la vista se enterara.
		const copia = new Map(progreso.value);
		copia.set(p.paso, p);
		progreso.value = copia;
	}

	function anotarRegistro(linea: LineaRegistro) {
		registro.value.push(linea);
		if (registro.value.length > MAX_REGISTRO) {
			// Se descarta desde el principio: lo último es lo que dice dónde
			// falló, y es lo que hay que conservar.
			registro.value.splice(0, registro.value.length - MAX_REGISTRO);
		}
	}

	// ── Derivados ──────────────────────────────────────────────────────────

	const discoElegido = computed(() => discos.value.find((d) => d.ruta === eleccion.disco) ?? null);

	const discosUsables = computed(() => discos.value.filter((d) => !d.en_uso));

	/**
	 * El paso está completo y se puede avanzar.
	 *
	 * Está acá y no en cada vista para que el botón «Continuar» del marco lo
	 * consulte en un solo lugar. Con la comprobación repartida por las vistas,
	 * agregar un campo obligatorio significaba acordarse de tocar dos archivos.
	 */
	function puedeAvanzar(cual: Paso): boolean {
		switch (cual) {
			case 'bienvenida':
				return true;
			// La instalación baja todo de los repositorios: sin conexión no se
			// puede ni empezar, y dejar avanzar sólo posterga el fracaso hasta
			// después de haber formateado el disco.
			case 'red':
				return sistema.value?.hay_red === true;
			case 'region':
				return Boolean(eleccion.zonaHoraria && eleccion.idiomaSistema);
			case 'teclado':
				return Boolean(eleccion.teclado);
			case 'disco': {
				if (!discoElegido.value || discoElegido.value.en_uso) return false;
				// El disco preseleccionado es el más grande **de los que no están
				// en uso**, y eso no garantiza que entre el escritorio: en una
				// máquina con un solo disco de 16 GiB quedaba elegido, el botón
				// habilitado, y el rechazo llegaba recién al apretar Instalar.
				if (discoElegido.value.tamano_bytes < MINIMO_GIB * 1024 ** 3) return false;
				if (!eleccion.cifrar) return true;
				return secretos.cifrado.length > 0 && secretos.cifrado === secretos.cifradoRepetida;
			}
			case 'cuenta': {
				if (!eleccion.usuario || !eleccion.hostname) return false;
				if (!secretos.usuario || secretos.usuario !== secretos.usuarioRepetida) return false;
				if (eleccion.rootHabilitado) {
					return secretos.root.length > 0 && secretos.root === secretos.rootRepetida;
				}
				return true;
			}
			// Siempre se puede pasar: todo lo de este paso es opcional por
			// definición, y no elegir nada es una respuesta válida.
			case 'complementos':
				return true;
			case 'resumen':
				return ayudanteListo.value;
			default:
				return false;
		}
	}

	/** El plan tal como lo espera el backend. */
	function armarPlan() {
		return {
			disco: eleccion.disco,
			esquema: 'borrar_todo',
			sistema_archivos: eleccion.sistemaArchivos,
			cifrar: eleccion.cifrar,
			zram: eleccion.zram,
			zona_horaria: eleccion.zonaHoraria,
			idioma_sistema: eleccion.idiomaSistema,
			teclado: eleccion.teclado,
			ntp: eleccion.ntp,
			hostname: eleccion.hostname,
			nombre_completo: eleccion.nombreCompleto,
			usuario: eleccion.usuario,
			administrador: eleccion.administrador,
			root_habilitado: eleccion.rootHabilitado,
			complementos: [...eleccion.complementos],
			secretos: {
				usuario: secretos.usuario,
				// Cadena vacía y no `undefined` cuando no aplica: el backend
				// espera los tres campos siempre, y un campo ausente hace fallar
				// la deserialización con un error que no dice cuál falta.
				root: eleccion.rootHabilitado ? secretos.root : '',
				cifrado: eleccion.cifrar ? secretos.cifrado : '',
			},
		};
	}

	// ── Carga ──────────────────────────────────────────────────────────────

	async function cargarSondeo() {
		// Los cuatro sondeos salen juntos porque ninguno depende de otro, y uno
		// de ellos —`catalogos`— recorre `/usr/share/zoneinfo`, `SUPPORTED` de
		// glibc y el árbol de mapas de teclado: cientos de entradas de directorio
		// en un medio live, que arranca desde un squashfs comprimido. Encadenados,
		// la primera pantalla esperaba la suma de los cuatro; en paralelo espera
		// el más lento.
		//
		// `allSettled` y no `all`: que falle uno no puede dejar la ventana vacía.
		// Sin catálogos, los tres selectores caen a campos de texto —ya está
		// contemplado en las vistas— y el instalador sigue siendo usable.
		const [resSistema, resPasos, resCatalogos, , resComplementos] = await Promise.allSettled([
			invoke<Sistema>('sondear_sistema'),
			invoke<string[]>('pasos_de_instalacion'),
			invoke<Catalogos>('catalogos'),
			recargarDiscos(),
			// La detección de hardware son unas lecturas de `/sys` y el catálogo
			// un TOML chico, así que entra en el mismo lote sin costarle nada al
			// arranque — y así el paso de complementos abre con todo listo en
			// vez de sondear al llegar.
			invoke<Complementos>('complementos_disponibles'),
		]);

		if (resSistema.status === 'fulfilled') sistema.value = resSistema.value;
		if (resPasos.status === 'fulfilled') pasosInstalacion.value = resPasos.value;
		if (resCatalogos.status === 'fulfilled') catalogos.value = resCatalogos.value;
		if (resComplementos.status === 'fulfilled') {
			complementos.value = resComplementos.value;
			// La preselección se aplica una sola vez, al cargar. Reaplicarla al
			// volver al paso pisaría lo que la persona haya desmarcado.
			eleccion.complementos = [...resComplementos.value.preseleccion];
		}

		// Predeterminados que salen del sistema, no fijos. Un instalador que
		// abre siempre en `UTC`/`us` hace que casi todo el mundo tenga que
		// cambiar tres campos que ya estaban bien en el medio live.
		//
		// Va **después** de la espera: lee `catalogos` para comprobar que la zona
		// y el idioma del entorno existan en las listas del sistema.
		aplicarPredeterminadosDelEntorno();

		// Lo que sí es un error de verdad se propaga: sin poder leer el equipo, el
		// paso de red no sabe si hay conexión y el resumen no sabe el firmware.
		if (resSistema.status === 'rejected') {
			throw resSistema.reason;
		}
	}

	function aplicarPredeterminadosDelEntorno() {
		const idiomaNavegador = navigator.language.replaceAll('-', '_');
		const coincide = catalogos.value.idiomas.find(
			(l) => l === idiomaNavegador || l.startsWith(`${idiomaNavegador.split('_')[0]}_`)
		);
		if (coincide) {
			eleccion.idiomaSistema = coincide;
		}

		const zona = Intl.DateTimeFormat().resolvedOptions().timeZone;
		if (zona && catalogos.value.zonas.includes(zona)) {
			eleccion.zonaHoraria = zona;
		}
	}

	async function recargarDiscos() {
		// Con el ayudante levantado se piden los discos con el nombre de los
		// sistemas operativos que ya están instalados; sin él, la lista sale
		// igual pero sin esos nombres. Nunca se deja la lista vacía por no tener
		// privilegios: el paso del disco tiene que mostrar algo enseguida.
		try {
			discos.value = ayudanteListo.value
				? await invoke<Disco[]>('sondear_discos_con_sistemas')
				: await invoke<Disco[]>('sondear_discos');
		} catch {
			discos.value = await invoke<Disco[]>('sondear_discos');
		}

		if (!eleccion.disco) {
			// Se preselecciona el más grande que se pueda usar. Es lo que la
			// mayoría quiere y, sobre todo, evita que el primero de la lista
			// —que suele ser el pendrive— quede elegido por descuido.
			const candidato = [...discosUsables.value].sort((a, b) => b.tamano_bytes - a.tamano_bytes)[0];
			if (candidato) {
				eleccion.disco = candidato.ruta;
			}
		}
	}

	async function comprobarRed() {
		sistema.value = await invoke<Sistema>('sondear_sistema');
	}

	async function prepararAyudante(): Promise<boolean> {
		errorAyudante.value = null;
		try {
			await invoke('preparar_ayudante');
			ayudanteListo.value = await invoke<boolean>('ayudante_listo');
			if (ayudanteListo.value) {
				// Ahora sí se pueden nombrar los sistemas instalados.
				await recargarDiscos();
			}
			return ayudanteListo.value;
		} catch (error) {
			ayudanteListo.value = false;
			errorAyudante.value = String(error);
			return false;
		}
	}

	/**
	 * Contador de la vista previa en vuelo.
	 *
	 * Cambiar de sistema de archivos dos veces seguidas dispara dos consultas, y
	 * no hay ninguna garantía de que contesten en orden: la primera puede llegar
	 * última y dejar el resumen mostrando los subvolúmenes de btrfs para una
	 * instalación en ext4. Es la pantalla que alguien mira antes de aceptar el
	 * punto sin retorno, así que no puede mostrar una respuesta vieja.
	 */
	let vistaPreviaEnVuelo = 0;

	async function calcularVistaPrevia() {
		const mia = ++vistaPreviaEnVuelo;
		if (!eleccion.disco) {
			vistaPrevia.value = null;
			return;
		}
		try {
			const resultado = await invoke<VistaPrevia>('vista_previa_particionado', {
				disco: eleccion.disco,
				sistemaArchivos: eleccion.sistemaArchivos,
				cifrar: eleccion.cifrar,
			});
			if (mia !== vistaPreviaEnVuelo) return;
			vistaPrevia.value = resultado;
		} catch {
			if (mia !== vistaPreviaEnVuelo) return;
			// Un disco que no se puede planificar —demasiado chico, en uso— no
			// es un error de la aplicación: la tarjeta del disco ya lo dice, y
			// el resumen simplemente no muestra el detalle.
			vistaPrevia.value = null;
		}
	}

	/**
	 * Marca o desmarca un complemento.
	 *
	 * Los exclusivos de una categoría —los navegadores— se comportan como un
	 * grupo de uno solo: elegir uno saca al que estaba. Sin esto, dos navegadores
	 * marcados a la vez son un estado que el grupo de opciones no puede dibujar,
	 * y el plan instalaría los dos.
	 */
	function alternarComplemento(id: string) {
		const complemento = complementos.value.catalogo.find((c) => c.id === id);
		if (!complemento) return;

		const elegidos = new Set(eleccion.complementos);

		if (complemento.exclusivo) {
			// Fuera los demás exclusivos de su misma categoría, y adentro éste.
			for (const otro of complementos.value.catalogo) {
				if (otro.exclusivo && otro.categoria === complemento.categoria) {
					elegidos.delete(otro.id);
				}
			}
			elegidos.add(id);
		} else if (elegidos.has(id)) {
			elegidos.delete(id);
		} else {
			elegidos.add(id);
		}

		// El orden del catálogo, no el de marcado: así el resumen enumera siempre
		// igual y dos instalaciones con la misma elección se pueden comparar.
		eleccion.complementos = complementos.value.catalogo
			.filter((c) => elegidos.has(c.id))
			.map((c) => c.id);
	}

	/** Los complementos elegidos, en objetos, para el resumen. */
	const complementosElegidos = computed(() =>
		complementos.value.catalogo.filter((c) => eleccion.complementos.includes(c.id))
	);

	return {
		paso,
		navegacionBloqueada,
		complementos,
		complementosElegidos,
		alternarComplemento,
		sistema,
		discos,
		catalogos,
		vistaPrevia,
		ayudanteListo,
		errorAyudante,
		eleccion,
		secretos,
		pasosInstalacion,
		progreso,
		registro,
		terminada,
		fallo,
		discoElegido,
		discosUsables,
		puedeAvanzar,
		armarPlan,
		olvidarSecretos,
		anotarProgreso,
		anotarRegistro,
		cargarSondeo,
		recargarDiscos,
		comprobarRed,
		prepararAyudante,
		calcularVistaPrevia,
	};
});
