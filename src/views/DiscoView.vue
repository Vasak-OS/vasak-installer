<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, onMounted, watch } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import IconoSistema from '@/components/ui/IconoSistema.vue';
import OpcionRadio from '@/components/ui/OpcionRadio.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SwitchToggle from '@/components/ui/SwitchToggle.vue';
import TextInput from '@/components/ui/TextInput.vue';
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
import { ICONO_PASO, ICONO_ROL_PARTICION, iconoDeDisco } from '@/tools/iconos';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

/** El mínimo que exige el backend. Duplicado acá sólo para el texto del aviso. */
const MINIMO_GIB = 20;

const esquemas: { valor: EsquemaDisco; nombre: string; ayuda: string }[] = [
	{
		valor: 'borrar_todo',
		nombre: 'disco.borrarTodoNombre',
		ayuda: 'disco.borrarTodoAyuda',
	},
	{ valor: 'junto_a_otro_sistema', nombre: 'disco.juntoNombre', ayuda: 'disco.juntoAyuda' },
	{ valor: 'sobre_una_particion', nombre: 'disco.sobreNombre', ayuda: 'disco.sobreAyuda' },
];

/** Cómo se llama cada rol en la vista previa. */
const ROL_PARTICION: Record<string, string> = {
	esp: 'disco.rolEsp',
	raiz: 'disco.rolRaiz',
	datos: 'disco.rolDatos',
};

/** El GUID que GPT le da a la partición de sistema EFI. */
const GUID_ESP = 'c12a7328-f81f-11d2-ba4b-00a0c93ec93b';

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
function noSePuedeUsar(particion: ParticionExistente): string | null {
	if (particion.tipo_particion?.toLowerCase() === GUID_ESP) return 'disco.esParticionDeArranque';
	if (particion.tamano_bytes < MINIMO_GIB * 1024 ** 3) return 'disco.particionChica';
	return null;
}

/**
 * El selector sólo aparece si hay algo que conservar.
 *
 * En un disco vacío las dos opciones hacen lo mismo, y ofrecer una decisión que
 * no cambia nada es pedirle a alguien que piense de más. En cuanto el disco
 * tiene particiones, la decisión es la más importante de la pantalla.
 */
const hayQueElegirEsquema = computed(() => (store.discoElegido?.particiones.length ?? 0) > 0);

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
const soloUefi = computed(() => store.vistaPrevia?.firmware === 'bios');

/** Si hay que mostrar la lista de particiones para elegir una. */
const eligeParticion = computed(() => store.eleccion.esquema === 'sobre_una_particion');

const sistemasDeArchivos: { valor: SistemaArchivos; nombre: string; ayuda: string }[] = [
	{ valor: 'btrfs', nombre: 'disco.btrfsNombre', ayuda: 'disco.btrfsAyuda' },
	{ valor: 'ext4', nombre: 'disco.ext4Nombre', ayuda: 'disco.ext4Ayuda' },
	{ valor: 'xfs', nombre: 'disco.xfsNombre', ayuda: 'disco.xfsAyuda' },
];

function tamano(bytes: number) {
	return formatearBytes(bytes, locale.value);
}

function muyChico(disco: Disco) {
	return disco.tamano_bytes < MINIMO_GIB * 1024 ** 3;
}

const frasesDistintas = computed(
	() =>
		store.eleccion.cifrar &&
		store.secretos.cifradoRepetida.length > 0 &&
		store.secretos.cifrado !== store.secretos.cifradoRepetida
);

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
	// Acá es donde por primera vez hace falta root, y donde ya se entiende para
	// qué. Si la autorización se rechaza, el paso sigue funcionando —la lista de
	// discos sale igual, sin los nombres de los sistemas instalados— y el aviso
	// aparece recién en el resumen, que es donde bloquea.
	await store.prepararAyudante();
});
</script>

<template>
  <div>
    <PageHeader :icono="ICONO_PASO.disco" :titulo="t('disco.titulo')" :descripcion="t('disco.intro')" />

    <div class="space-y-4">
      <div v-if="store.discos.length === 0">
        <AlertMessage tipo="error" :titulo="t('disco.sinDiscos')">
          {{ t('disco.sinDiscosDetalle') }}
        </AlertMessage>
      </div>

      <ul v-else class="space-y-2">
        <li v-for="disco in store.discos" :key="disco.ruta">
          <!--
            El disco en uso y el demasiado chico se muestran igual, deshabilitados
            y con el motivo escrito. Ocultarlos haría que alguien busque un disco
            que sabe que existe y no lo encuentre, sin ninguna explicación.
          -->
          <button
            type="button"
            :disabled="disco.en_uso || muyChico(disco)"
            class="w-full rounded-corner border p-3 text-left transition-colors"
            :class="[
              store.eleccion.disco === disco.ruta
                ? 'border-secondary bg-primary/10'
                : 'border-ui-border-strong hover:bg-ui-surface/50',
              disco.en_uso || muyChico(disco) ? 'cursor-not-allowed opacity-60' : '',
            ]"
            @click="store.eleccion.disco = disco.ruta"
          >
            <div class="flex items-center gap-3">
              <span
                class="flex size-11 shrink-0 items-center justify-center rounded-corner border"
                :class="
                  store.eleccion.disco === disco.ruta
                    ? 'border-secondary bg-primary/20'
                    : 'border-ui-border bg-ui-surface/40'
                "
                aria-hidden="true"
              >
                <IconoSistema :nombre="iconoDeDisco(disco)" tipo="icono" clase="size-7" />
              </span>
              <span class="min-w-0 flex-1 truncate font-medium text-sm">{{ disco.modelo }}</span>
              <span class="shrink-0 font-mono text-sm">{{ tamano(disco.tamano_bytes) }}</span>
            </div>
            <div class="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-tx-muted text-xs">
              <span class="font-mono">{{ disco.ruta }}</span>
              <span v-if="disco.nvme">NVMe</span>
              <span v-else-if="disco.rotacional">HDD</span>
              <span v-else>SSD</span>
              <span>
                {{
                  disco.particiones.length === 0
                    ? t('disco.vacio')
                    : interpolar(t('disco.conParticiones'), disco.particiones.length)
                }}
              </span>
            </div>

            <p v-if="disco.en_uso" class="mt-2 text-status-warning text-xs">
              {{ t('disco.enUso') }} — {{ t('disco.enUsoDetalle') }}
            </p>
            <p v-else-if="muyChico(disco)" class="mt-2 text-status-warning text-xs">
              {{ interpolar(t('disco.muyChico'), MINIMO_GIB) }}
            </p>

            <!--
              Lo que hay adentro, con el nombre del sistema operativo cuando se
              pudo averiguar. «Windows 11» hace que alguien se detenga a mirar;
              «ntfs» no.
            -->
            <ul
              v-else-if="disco.particiones.length > 0"
              class="mt-2 space-y-0.5 text-tx-muted text-xs"
            >
              <li v-for="particion in disco.particiones" :key="particion.ruta" class="truncate">
                <span class="font-mono">{{ particion.ruta }}</span>
                <span class="mx-1">·</span>
                <span>{{ tamano(particion.tamano_bytes) }}</span>
                <template v-if="particion.sistema_operativo">
                  <span class="mx-1">·</span>
                  <span class="font-medium">{{ particion.sistema_operativo }}</span>
                </template>
                <template v-else-if="particion.sistema_archivos">
                  <span class="mx-1">·</span>
                  <span>{{ particion.sistema_archivos }}</span>
                </template>
                <template v-else>
                  <span class="mx-1">·</span>
                  <span>{{ t('disco.sinFormato') }}</span>
                </template>
              </li>
            </ul>
          </button>
        </li>
      </ul>

      <SectionCard v-if="hayQueElegirEsquema" :titulo="t('disco.esquema')">
        <div role="radiogroup" :aria-label="t('disco.esquema')" class="space-y-2">
          <OpcionRadio
            v-for="esquema in esquemas"
            :key="esquema.valor"
            :seleccionada="store.eleccion.esquema === esquema.valor"
            :label="t(esquema.nombre)"
            :descripcion="t(esquema.ayuda)"
            :disabled="esquema.valor === 'junto_a_otro_sistema' && soloUefi"
            @elegir="store.eleccion.esquema = esquema.valor"
          />
        </div>

        <p v-if="soloUefi" class="mt-2 text-tx-muted text-xs">
          {{ t('disco.juntoSoloUefi') }}
        </p>

        <!-- Cuál se formatea. Sólo con ese esquema: en los otros dos no hay
             nada que elegir, y una lista de más es una lista que alguien lee
             creyendo que decide algo. -->
        <div v-if="eligeParticion" class="mt-3">
          <p class="mb-2 font-medium text-sm">{{ t('disco.elegirParticion') }}</p>
          <div role="radiogroup" :aria-label="t('disco.elegirParticion')" class="space-y-2">
            <OpcionRadio
              v-for="particion in store.discoElegido?.particiones ?? []"
              :key="particion.ruta"
              :seleccionada="store.eleccion.particionDestino === particion.ruta"
              :label="`${particion.ruta} · ${tamano(particion.tamano_bytes)}`"
              :descripcion="
                noSePuedeUsar(particion)
                  ? t(noSePuedeUsar(particion) as string)
                  : particion.sistema_operativo ??
                    particion.sistema_archivos ??
                    t('disco.sinFormato')
              "
              :disabled="Boolean(noSePuedeUsar(particion))"
              @elegir="store.eleccion.particionDestino = particion.ruta"
            />
          </div>
        </div>

        <!--
          El motivo por el que el esquema elegido no se puede aplicar: que no
          haya hueco libre, o que la partición EFI que ya está sea demasiado
          chica. Va acá y no en el resumen porque acá es donde se elige, y sin
          esto la opción quedaba marcada sin que pasara nada.
        -->
        <AlertMessage
          v-if="store.errorVistaPrevia"
          tipo="aviso"
          :titulo="t('disco.esquemaNoSePuede')"
          class="mt-3"
        >
          {{ store.errorVistaPrevia }}
        </AlertMessage>
      </SectionCard>

      <SectionCard :titulo="t('disco.sistemaArchivos')">
        <!-- `radiogroup` y no tres interruptores sueltos: sólo puede haber uno
             elegido, y un lector de pantalla tiene que anunciarlo como «opción 1
             de 3» y no como tres controles independientes. -->
        <div role="radiogroup" :aria-label="t('disco.sistemaArchivos')" class="space-y-2">
          <OpcionRadio
            v-for="fs in sistemasDeArchivos"
            :key="fs.valor"
            :seleccionada="store.eleccion.sistemaArchivos === fs.valor"
            :label="t(fs.nombre)"
            :descripcion="t(fs.ayuda)"
            @elegir="store.eleccion.sistemaArchivos = fs.valor"
          />
        </div>
      </SectionCard>

      <SectionCard>
        <SwitchToggle
          v-model="store.eleccion.zram"
          :label="t('disco.zram')"
          :descripcion="t('disco.zramAyuda')"
        />
        <SwitchToggle
          v-model="store.eleccion.cifrar"
          :label="t('disco.cifrar')"
          :descripcion="t('disco.cifrarAyuda')"
        />

        <div v-if="store.eleccion.cifrar" class="mt-3 space-y-3">
          <AlertMessage tipo="aviso" :titulo="t('disco.cifrarAvisoTitulo')">
            {{ t('disco.cifrarAviso') }}
          </AlertMessage>

          <div>
            <label for="frase" class="mb-1 block text-sm">{{ t('disco.frase') }}</label>
            <TextInput
              id="frase"
              v-model="store.secretos.cifrado"
              type="password"
              autocomplete="new-password"
              :placeholder="t('disco.frasePlaceholder')"
            />
          </div>
          <div>
            <label for="frase2" class="mb-1 block text-sm">{{ t('disco.fraseRepetir') }}</label>
            <TextInput
              id="frase2"
              v-model="store.secretos.cifradoRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="frasesDistintas"
              described-by="frase-error"
            />
            <p v-if="frasesDistintas" id="frase-error" class="mt-1 text-status-error text-xs">
              {{ t('disco.frasesDistintas') }}
            </p>
          </div>
        </div>
      </SectionCard>

      <SectionCard v-if="store.vistaPrevia" :titulo="t('disco.detalleParticionado')">
        <ul class="space-y-2 text-sm">
          <li
            v-for="(particion, indice) in store.vistaPrevia.particiones"
            :key="indice"
            class="rounded-corner border border-ui-border p-2"
          >
            <div class="flex items-center justify-between gap-2">
              <span class="flex items-center gap-2 font-medium">
                <IconoSistema :nombre="ICONO_ROL_PARTICION[particion.rol]" clase="size-4" />
                {{ t(ROL_PARTICION[particion.rol] ?? 'disco.rolRaiz') }}
              </span>
              <span class="font-mono text-xs">{{ tamano(particion.tamano_bytes) }}</span>
            </div>
            <div class="mt-1 flex flex-wrap gap-x-3 text-tx-muted text-xs">
              <span v-if="particion.sistema_archivos" class="font-mono">
                {{ particion.sistema_archivos }}
              </span>
              <span v-if="particion.punto_montaje" class="font-mono">
                {{ particion.punto_montaje }}
              </span>
              <span v-if="particion.cifrada" class="text-status-warning">
                {{ t('disco.cifradaEtiqueta') }}
              </span>
            </div>
            <p v-if="particion.opciones_montaje.length" class="mt-1 text-tx-muted text-xs">
              {{ t('disco.opcionesMontaje') }}:
              <span class="font-mono">{{ particion.opciones_montaje.join(',') }}</span>
            </p>
            <p v-if="particion.subvolumenes.length" class="mt-1 text-tx-muted text-xs">
              {{ t('disco.subvolumenes') }}:
              <span class="font-mono">{{ particion.subvolumenes.join('   ') }}</span>
            </p>
          </li>
        </ul>
      </SectionCard>
    </div>
  </div>
</template>
