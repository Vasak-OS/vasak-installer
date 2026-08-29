<script setup lang="ts">
/**
 * Un selector con búsqueda, para listas largas.
 *
 * Las zonas horarias son más de cuatrocientas y los mapas de teclado más de
 * doscientos. Un `<select>` con eso adentro obliga a desplazar a ciegas: no hay
 * forma de llegar a «America/Argentina/Buenos_Aires» sin recorrer medio
 * abecedario. Y `<input list>` con `<datalist>` sería lo natural, pero WebKitGTK
 * lo renderiza de forma inconsistente, así que va a mano.
 *
 * No se usa `Reka UI` ni ningún componente de terceros por la misma razón que el
 * resto del instalador no los usa: corre en un medio live donde cada dependencia
 * es un paquete más en la ISO.
 */
import { computed, nextTick, ref, watch } from 'vue';
import { buscarOpciones, type Opcion } from '@/tools/buscar';

interface Props {
	modelValue: string;
	opciones: Opcion[];
	id?: string;
	placeholderBusqueda: string;
	textoSinResultados: string;
	disabled?: boolean;
	/** Cuántas opciones se renderizan como máximo. Ver el comentario de abajo. */
	tope?: number;
}

const props = withDefaults(defineProps<Props>(), { disabled: false, tope: 60 });
const emit = defineEmits<{ 'update:modelValue': [valor: string] }>();

const busqueda = ref('');
const abierto = ref(false);
const campo = ref<HTMLInputElement | null>(null);

const seleccionada = computed(
	() => props.opciones.find((o) => o.valor === props.modelValue) ?? null
);

/**
 * Las coincidencias, ordenadas por qué tan bien coinciden y recortadas al tope.
 *
 * El recorte no es una optimización prematura: sin él, con la búsqueda vacía se
 * renderizan cuatrocientos nodos, y como este cálculo corre en cada tecla, cada
 * letra tipeada recrea la lista entera.
 *
 * **El orden es lo que hace que el recorte sea seguro.** Recortar una lista
 * alfabética esconde justo lo que se busca: escribiendo `la` para el teclado
 * latinoamericano, `la-latin1` quedaba detrás de `be-latin1`, `br-latin1-abnt2`,
 * `cz-lat2`, `de-latin1`… porque todos contienen «latin». La lógica del orden y
 * sus pruebas están en `tools/buscar.ts`.
 */
const ordenadas = computed(() => buscarOpciones(props.opciones, busqueda.value));

const coincidencias = computed(() => ordenadas.value.slice(0, props.tope));

// Cuántas quedaron afuera del recorte, para poder decirlo en vez de que
// desaparezcan en silencio. Antes se comparaba contra la lista **ya recortada**,
// así que el total nunca superaba el tope y el aviso no aparecía jamás.
const hayMas = computed(() => ordenadas.value.length > props.tope);

function elegir(valor: string) {
	emit('update:modelValue', valor);
	abierto.value = false;
	busqueda.value = '';
}

async function abrir() {
	if (props.disabled) return;
	abierto.value = true;
	// El foco va al campo de búsqueda al abrir. Sin esto hay que hacer un clic
	// más para poder escribir, que es justamente lo que este componente venía a
	// evitar.
	await nextTick();
	campo.value?.focus();
}

// Escape cierra sin elegir. Es lo que se espera de cualquier desplegable, y sin
// él la única salida es hacer clic afuera.
watch(abierto, (esta) => {
	if (!esta) busqueda.value = '';
});
</script>

<template>
  <div class="relative">
    <button
      :id="id"
      type="button"
      :disabled="disabled"
      class="flex w-full items-center justify-between gap-2 rounded-corner border border-ui-border-strong bg-ui-bg/60 px-3 py-2 text-left text-sm transition-colors hover:bg-ui-surface/60 disabled:cursor-not-allowed disabled:opacity-50"
      :aria-expanded="abierto"
      aria-haspopup="listbox"
      @click="abierto ? (abierto = false) : abrir()"
    >
      <span class="min-w-0 flex-1 truncate">
        <span v-if="seleccionada">{{ seleccionada.etiqueta }}</span>
        <span v-else class="text-tx-muted">{{ modelValue || '—' }}</span>
        <span v-if="seleccionada?.detalle" class="ml-2 text-tx-muted text-xs">
          {{ seleccionada.detalle }}
        </span>
      </span>
      <span class="shrink-0 text-tx-muted text-xs" aria-hidden="true">▾</span>
    </button>

    <div
      v-if="abierto"
      class="absolute z-20 mt-1 w-full rounded-corner border border-ui-border-strong bg-ui-bg shadow-lg"
      @keydown.escape="abierto = false"
    >
      <div class="border-ui-border border-b p-2">
        <input
          ref="campo"
          v-model="busqueda"
          type="search"
          :placeholder="placeholderBusqueda"
          class="w-full rounded-corner border border-ui-border-strong bg-ui-surface/40 px-2 py-1.5 text-sm focus:border-primary focus:outline-none"
        />
      </div>
      <ul role="listbox" class="max-h-56 overflow-y-auto p-1">
        <li v-if="coincidencias.length === 0" class="p-3 text-center text-tx-muted text-xs">
          {{ textoSinResultados }}
        </li>
        <li v-for="opcion in coincidencias" :key="opcion.valor">
          <button
            type="button"
            role="option"
            :aria-selected="opcion.valor === modelValue"
            class="flex w-full items-baseline gap-2 rounded-corner px-2 py-1.5 text-left text-sm hover:bg-ui-surface"
            :class="opcion.valor === modelValue ? 'bg-primary/15' : ''"
            @click="elegir(opcion.valor)"
          >
            <span class="min-w-0 flex-1 truncate">{{ opcion.etiqueta }}</span>
            <span v-if="opcion.detalle" class="shrink-0 text-tx-muted text-xs">
              {{ opcion.detalle }}
            </span>
          </button>
        </li>
        <li v-if="hayMas" class="px-2 py-1.5 text-tx-muted text-xs">…</li>
      </ul>
    </div>
  </div>
</template>
