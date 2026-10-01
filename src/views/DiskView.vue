<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	Checkbox,
	ConfigSection,
	FormGroup,
	ListGroup,
	ListRow,
	OptionGroup,
	type OptionGroupOption,
	PageHeader,
	Panel,
	SelectField,
	SwitchRow,
	TextInput,
	ThemeIcon,
} from '@vasakgroup/vue-libvasak';
import { computed, onMounted, watch } from 'vue';
import DiskDetails from '@/components/disk/DiskDetails.vue';
import {
	type Disco,
	type EsquemaDisco,
	type ParticionExistente,
	type SistemaArchivos,
	useInstalacionStore,
} from '@/stores/instalacion';
import { formatearBytes } from '@/tools/formato';
// Los sistemas de archivos van sin icono a propósito: el tema dibuja igual todo
// lo que se les podría poner, y tres opciones excluyentes con el mismo icono no
// informan nada — sólo repiten. Lo que las distingue es el texto de al lado.
import { diskIcon, PARTITION_ROLE_ICONS, STEP_ICONS } from '@/tools/icons';

const { t, locale } = useI18n();
const store = useInstalacionStore();

/** El mínimo que exige el backend. Duplicado acá sólo para el texto del aviso. */
const MINIMUM_GIB = 20;

const SCHEMES: { value: EsquemaDisco; name: string; help: string }[] = [
	{ value: 'borrar_todo', name: 'disco.borrarTodoNombre', help: 'disco.borrarTodoAyuda' },
	{ value: 'junto_a_otro_sistema', name: 'disco.juntoNombre', help: 'disco.juntoAyuda' },
	{ value: 'sobre_una_particion', name: 'disco.sobreNombre', help: 'disco.sobreAyuda' },
	{ value: 'manual', name: 'disco.manualNombre', help: 'disco.manualAyuda' },
];

/** Cómo se llama cada rol en la vista previa. */
const PARTITION_ROLE: Record<string, string> = {
	esp: 'disco.rolEsp',
	raiz: 'disco.rolRaiz',
	datos: 'disco.rolDatos',
};

/** El GUID que GPT le da a la partición de sistema EFI. */
const ESP_GUID = 'c12a7328-f81f-11d2-ba4b-00a0c93ec93b';

/**
 * Por qué una partición no se puede usar como raíz, o `null` si se puede.
 *
 * El ESP no se puede: formatearlo deja el equipo sin partición de arranque y de
 * paso le borra el cargador al otro sistema. Y las que no llegan al mínimo
 * tampoco.
 *
 * Se muestran deshabilitadas y con el motivo, igual que los discos: una
 * partición que desaparece de la lista es alguien buscando la que sabe que
 * existe. El plan las rechaza igual — lo que impide un desastre no puede
 * depender de que la pantalla esté bien.
 */
function whyUnusable(partition: ParticionExistente): string | null {
	if (partition.tipo_particion?.toLowerCase() === ESP_GUID) return 'disco.esParticionDeArranque';
	if (partition.tamano_bytes < MINIMUM_GIB * 1024 ** 3) return 'disco.particionChica';
	return null;
}

/**
 * El selector sólo aparece si hay algo que conservar.
 *
 * En un disco vacío las dos opciones hacen lo mismo, y ofrecer una decisión que
 * no cambia nada es pedirle a alguien que piense de más. En cuanto el disco
 * tiene particiones, la decisión es la más importante de la pantalla.
 */
const mustChooseScheme = computed(() => (store.discoElegido?.particiones.length ?? 0) > 0);

/**
 * Instalar al lado es sólo UEFI, y hay que decirlo antes de que lo elijan.
 *
 * En BIOS la tabla es MBR: cuatro particiones primarias que un equipo con otro
 * sistema ya suele tener ocupadas, y ningún ESP que reusar. El backend lo
 * rechaza igual, pero enterarse recién después de elegir es peor que verlo.
 *
 * `firmware` viene de la vista previa, que es lo que el backend detectó. Si
 * todavía no llegó se asume que se puede: la opción se muestra y el error, si
 * lo hay, aparece abajo.
 */
const uefiOnly = computed(() => store.vistaPrevia?.firmware === 'bios');

/** Si hay que mostrar la lista de particiones para elegir una. */
const choosesPartition = computed(() => store.eleccion.esquema === 'sobre_una_particion');

/** Si hay que mostrar la tabla de asignaciones. */
const isManual = computed(() => store.eleccion.esquema === 'manual');

/**
 * El punto de montaje elegido para una partición, o `''` si ninguno.
 *
 * `''` y no `null` porque es lo que un `<select>` devuelve cuando se elige la
 * opción vacía, y traducir en los dos sentidos en la plantilla la vuelve
 * ilegible.
 */
function mountPointOf(path: string): string {
	return store.asignacionDe(path)?.punto_montaje ?? '';
}

function isFormatted(path: string): boolean {
	return store.asignacionDe(path)?.formatear ?? false;
}

function chooseMountPoint(path: string, point: string) {
	store.asignar(path, point === '' ? null : point, isFormatted(path));
}

function toggleFormat(path: string, format: boolean) {
	const point = mountPointOf(path);
	if (point === '') return;
	store.asignar(path, point, format);
}

const FILESYSTEMS: { value: SistemaArchivos; name: string; help: string }[] = [
	{ value: 'btrfs', name: 'disco.btrfsNombre', help: 'disco.btrfsAyuda' },
	{ value: 'ext4', name: 'disco.ext4Nombre', help: 'disco.ext4Ayuda' },
	{ value: 'xfs', name: 'disco.xfsNombre', help: 'disco.xfsAyuda' },
];

function size(bytes: number) {
	return formatearBytes(bytes, locale.value);
}

function tooSmall(disk: Disco) {
	return disk.tamano_bytes < MINIMUM_GIB * 1024 ** 3;
}

const passphrasesDiffer = computed(
	() =>
		store.eleccion.cifrar &&
		store.secretos.cifradoRepetida.length > 0 &&
		store.secretos.cifrado !== store.secretos.cifradoRepetida
);

/**
 * Los discos, como opciones del grupo.
 *
 * El valor es **la ruta del disco** —lo mismo que guarda `eleccion.disco` y lo
 * mismo que viaja en el plan—, así que lo que se marca, lo que dice el resumen
 * y lo que recibe el backend son el mismo texto, sin traducir en el medio. El
 * disco en uso y el demasiado chico se muestran igual, deshabilitados y con el
 * motivo escrito: ocultarlos haría que alguien busque un disco que sabe que
 * existe y no lo encuentre.
 *
 * Nada se elige acá: el grupo muestra lo que diga `eleccion.disco`, que sólo
 * cambia con un clic (o con las flechas, que es como se elige en un grupo de
 * radios) y con la preselección del almacén, que es la de siempre.
 */
const diskOptions = computed<OptionGroupOption<string>[]>(() =>
	store.discos.map((disk) => ({
		value: disk.ruta,
		label: disk.modelo,
		icon: diskIcon(disk),
		iconType: 'icon',
		disabled: disk.en_uso || tooSmall(disk),
	}))
);

/** El disco de cada opción, para la ranura que dibuja su detalle. */
function diskOf(path: string): Disco | undefined {
	return store.discos.find((disk) => disk.ruta === path);
}

const schemeOptions = computed<OptionGroupOption<EsquemaDisco>[]>(() =>
	SCHEMES.map((scheme) => ({
		value: scheme.value,
		label: t(scheme.name),
		description: t(scheme.help),
		disabled: scheme.value === 'junto_a_otro_sistema' && uefiOnly.value,
	}))
);

const partitionOptions = computed<OptionGroupOption<string>[]>(() =>
	(store.discoElegido?.particiones ?? []).map((partition) => {
		const reason = whyUnusable(partition);
		return {
			value: partition.ruta,
			label: `${partition.ruta} · ${size(partition.tamano_bytes)}`,
			description: reason
				? t(reason)
				: (partition.sistema_operativo ?? partition.sistema_archivos ?? t('disco.sinFormato')),
			disabled: Boolean(reason),
		};
	})
);

const filesystemOptions = computed<OptionGroupOption<SistemaArchivos>[]>(() =>
	FILESYSTEMS.map((fs) => ({ value: fs.value, label: t(fs.name), description: t(fs.help) }))
);

/** Lo que se puede elegir como punto de montaje, con «no usar» primero. */
const mountPointOptions = computed(() => [
	{ label: t('disco.manualNoUsar'), value: '' },
	...store.puntosDeMontaje.map((point) => ({ label: point, value: point })),
]);

/**
 * Lo que se elige en cada grupo va directo a `eleccion`, sin pasar por nada.
 *
 * `OptionGroup` habla en `T | null` —«ninguna elegida»—, pero sólo emite
 * cuando alguien elige una opción, así que el `null` no llega nunca: se
 * descarta por las dudas en vez de escribir un disco vacío en el plan.
 */
function chooseDisk(path: string | null) {
	if (path !== null) store.eleccion.disco = path;
}
function chooseScheme(scheme: EsquemaDisco | null) {
	if (scheme !== null) store.eleccion.esquema = scheme;
}
function chooseTargetPartition(path: string | null) {
	if (path !== null) store.eleccion.particionDestino = path;
}
function chooseFilesystem(filesystem: SistemaArchivos | null) {
	if (filesystem !== null) store.eleccion.sistemaArchivos = filesystem;
}

// La vista previa se recalcula cuando cambia cualquier cosa que la afecte. Es lo
// que garantiza que el resumen muestre el plan de verdad y no uno viejo: sin
// esto, cambiar de btrfs a ext4 dejaba los subvolúmenes listados en el resumen.
watch(
	// Tres fuentes y no un getter que arma un arreglo: un arreglo nuevo en cada
	// evaluación nunca es igual al anterior, así que el observador se dispara
	// aunque no haya cambiado nada de lo que mira.
	[
		() => store.eleccion.disco,
		() => store.eleccion.esquema,
		() => store.eleccion.sistemaArchivos,
		() => store.eleccion.cifrar,
	],
	() => store.calcularVistaPrevia(),
	{ immediate: true }
);

onMounted(async () => {
	// La lista de puntos de montaje, que vive en el backend para que no haya
	// dos copias. No bloquea: sin ella el modo manual queda sin opciones y los
	// otros tres siguen andando.
	await store.cargarPuntosDeMontaje();
	// Acá es donde por primera vez hace falta root, y donde ya se entiende para
	// qué. Si la autorización se rechaza, el paso sigue funcionando —la lista de
	// discos sale igual, sin los nombres de los sistemas instalados— y el aviso
	// aparece recién en el resumen, que es donde bloquea.
	await store.prepararAyudante();
});
</script>

<template>
  <div>
    <PageHeader
      class="mb-5"
      :icon="STEP_ICONS.disco"
      icon-type="symbol"
      :title="t('disco.titulo')"
      :description="t('disco.intro')" />

    <div class="space-y-4">
      <div v-if="store.discos.length === 0">
        <AlertMessage tone="error" icon="auto" :title="t('disco.sinDiscos')">
          {{ t('disco.sinDiscosDetalle') }}
        </AlertMessage>
      </div>

      <!--
        Los discos, como grupo de radios en tarjeta: es lo que son —uno solo—, y
        un lector de pantalla anuncia «disco 1 de 4, elegido». La tarjeta de la
        librería dibuja el icono en su recuadro y el punto de radio; lo de
        adentro (modelo, tamaño, ruta, tipo, particiones y el motivo de los
        que no se pueden usar) lo dibuja `DiskDetails` en la ranura `option`.
      -->
      <OptionGroup
        v-else
        :model-value="store.eleccion.disco"
        @update:model-value="chooseDisk"
        :options="diskOptions"
        :label="t('disco.titulo')"
        variant="card"
      >
        <template #option="{ option }">
          <DiskDetails
            v-if="diskOf(option.value)"
            :disk="diskOf(option.value) as Disco"
            :too-small="tooSmall(diskOf(option.value) as Disco)"
            :minimum-gib="MINIMUM_GIB" />
        </template>
      </OptionGroup>

      <ConfigSection v-if="mustChooseScheme" :title="t('disco.esquema')" as="h2">
        <div>
          <OptionGroup
            :model-value="store.eleccion.esquema"
            @update:model-value="chooseScheme"
            :options="schemeOptions"
            :label="t('disco.esquema')"
            variant="card"
          />

          <p v-if="uefiOnly" class="mt-2 text-tx-muted text-xs">
            {{ t('disco.juntoSoloUefi') }}
          </p>

          <!--
            El modo manual: una fila por partición, con dónde se monta y si se
            formatea. Se muestran todas, incluida la de arranque EFI: en este
            modo hay que poder elegirla, que es lo que la distingue de los otros.
          -->
          <div v-if="isManual" class="mt-3 space-y-2" data-manual>
            <div
              v-for="partition in store.discoElegido?.particiones ?? []"
              :key="partition.ruta"
              class="rounded-corner-m border border-ui-line p-2"
              :data-partition="partition.ruta"
            >
              <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                <span class="font-mono">{{ partition.ruta }}</span>
                <span class="text-tx-muted">{{ size(partition.tamano_bytes) }}</span>
                <span class="text-tx-muted text-xs">
                  {{ partition.sistema_operativo ?? partition.sistema_archivos ?? t('disco.sinFormato') }}
                </span>
              </div>

              <div class="mt-2 flex flex-wrap items-center gap-3">
                <span class="flex min-w-0 items-center gap-2 text-sm">
                  <label :for="`mount-${partition.ruta}`" class="text-tx-muted">{{ t('disco.elegirParticion') }}</label>
                  <!-- El `id` va por `v-bind`: `SelectField` lo reenvía a su
                       `<select>` (lo lee de `$attrs`), pero no lo declara como
                       propiedad, y `strictTemplates` mide contra esa lista. -->
                  <SelectField
                    v-bind="{ id: `mount-${partition.ruta}` }"
                    :model-value="mountPointOf(partition.ruta)"
                    :options="mountPointOptions"
                    class="font-mono"
                    @update:model-value="chooseMountPoint(partition.ruta, String($event))"
                  />
                </span>

                <!-- Sin punto de montaje no hay nada que formatear: la partición
                     no se usa, y ofrecer la casilla sugeriría que sí. -->
                <Checkbox
                  v-if="mountPointOf(partition.ruta) !== ''"
                  :model-value="isFormatted(partition.ruta)"
                  :label="t('disco.manualFormatear')"
                  @update:model-value="toggleFormat(partition.ruta, $event)"
                />
                <span
                  v-if="mountPointOf(partition.ruta) !== '' && !isFormatted(partition.ruta)"
                  class="text-tx-muted text-xs"
                >
                  {{ t('disco.manualComoEsta') }}
                </span>
              </div>
            </div>
          </div>

          <!-- Cuál se formatea. Sólo con ese esquema: en los otros dos no hay
               nada que elegir, y una lista de más es una lista que alguien lee
               creyendo que decide algo. -->
          <div v-if="choosesPartition" class="mt-3">
            <p class="mb-2 font-medium text-sm">{{ t('disco.elegirParticion') }}</p>
            <OptionGroup
              :model-value="store.eleccion.particionDestino"
              @update:model-value="chooseTargetPartition"
              :options="partitionOptions"
              :label="t('disco.elegirParticion')"
              variant="card"
            />
          </div>

          <!--
            El motivo por el que el esquema elegido no se puede aplicar: que no
            haya hueco libre, o que la partición EFI que ya está sea demasiado
            chica. Va acá y no en el resumen porque acá es donde se elige, y sin
            esto la opción quedaba marcada sin que pasara nada.
          -->
          <AlertMessage
            v-if="store.errorVistaPrevia"
            tone="warning"
            icon="auto"
            :title="t('disco.esquemaNoSePuede')"
            class="mt-3"
          >
            {{ store.errorVistaPrevia }}
          </AlertMessage>
        </div>
      </ConfigSection>

      <ConfigSection :title="t('disco.sistemaArchivos')" as="h2">
        <!-- Un grupo de radios y no tres interruptores sueltos: sólo puede haber
             uno elegido, y un lector de pantalla tiene que anunciarlo como
             «opción 1 de 3» y no como tres controles independientes. -->
        <OptionGroup
          :model-value="store.eleccion.sistemaArchivos"
          @update:model-value="chooseFilesystem"
          :options="filesystemOptions"
          :label="t('disco.sistemaArchivos')"
          variant="card"
        />
      </ConfigSection>

      <Panel>
        <SwitchRow
          v-model="store.eleccion.zram"
          :label="t('disco.zram')"
          :description="t('disco.zramAyuda')"
        />
        <SwitchRow
          v-model="store.eleccion.cifrar"
          :label="t('disco.cifrar')"
          :description="t('disco.cifrarAyuda')"
        />

        <div v-if="store.eleccion.cifrar" class="mt-3 space-y-3">
          <AlertMessage tone="warning" icon="auto" :title="t('disco.cifrarAvisoTitulo')">
            {{ t('disco.cifrarAviso') }}
          </AlertMessage>

          <FormGroup :label="t('disco.frase')" html-for="frase">
            <TextInput
              id="frase"
              v-model="store.secretos.cifrado"
              type="password"
              autocomplete="new-password"
              :placeholder="t('disco.frasePlaceholder')"
            />
          </FormGroup>
          <FormGroup
            v-slot="{ id, describedBy, invalid }"
            :label="t('disco.fraseRepetir')"
            html-for="frase2"
            :error="passphrasesDiffer ? t('disco.frasesDistintas') : ''"
          >
            <TextInput
              :id="id"
              v-model="store.secretos.cifradoRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </FormGroup>
        </div>
      </Panel>

      <ConfigSection v-if="store.vistaPrevia" :title="t('disco.detalleParticionado')" as="h2">
        <ListGroup :divided="true">
          <ListRow
            v-for="(partition, position) in store.vistaPrevia.particiones"
            :key="position"
            :meta="size(partition.tamano_bytes)"
          >
            <template #leading>
              <ThemeIcon :name="PARTITION_ROLE_ICONS[partition.rol] ?? ''" type="symbol" :size="16" />
            </template>
            <span class="font-medium text-label-m">{{ t(PARTITION_ROLE[partition.rol] ?? 'disco.rolRaiz') }}</span>
            <span class="flex min-w-0 flex-wrap gap-x-3 text-tx-muted text-xs">
              <span v-if="partition.sistema_archivos" class="font-mono">
                {{ partition.sistema_archivos }}
              </span>
              <span v-if="partition.punto_montaje" class="font-mono">
                {{ partition.punto_montaje }}
              </span>
              <span v-if="partition.cifrada" class="text-status-warning">
                {{ t('disco.cifradaEtiqueta') }}
              </span>
            </span>
            <span v-if="partition.opciones_montaje.length" class="break-words text-tx-muted text-xs">
              {{ t('disco.opcionesMontaje') }}:
              <span class="font-mono">{{ partition.opciones_montaje.join(',') }}</span>
            </span>
            <span v-if="partition.subvolumenes.length" class="break-words text-tx-muted text-xs">
              {{ t('disco.subvolumenes') }}:
              <span class="font-mono">{{ partition.subvolumenes.join('   ') }}</span>
            </span>
          </ListRow>
        </ListGroup>
      </ConfigSection>
    </div>
  </div>
</template>
