<script setup lang="ts">
/**
 * Un símbolo del tema de iconos.
 *
 * Siempre decorativo: `alt` vacío y `aria-hidden`. Un icono al lado de un texto
 * que dice lo mismo, anunciado por un lector de pantalla, es el texto repetido
 * dos veces. Cuando el icono **es** la única información —un botón sin texto—,
 * el nombre va en el `aria-label` del botón, no acá.
 */
import { toRef } from 'vue';
import { useSimbolo } from '@/composables/useSimbolo';

interface Props {
	nombre: string;
	/** Clases de tamaño. El tema dibuja en 16px de base, así que `size-4` es 1:1. */
	clase?: string;
}
const props = withDefaults(defineProps<Props>(), { clase: 'size-4' });

const nombre = toRef(props, 'nombre');
const fuente = useSimbolo(() => nombre.value);
</script>

<template>
  <!--
    El hueco se reserva aunque el icono todavía no haya resuelto o el tema no lo
    tenga: sin el `span` de respaldo, el texto de al lado salta unos píxeles
    cuando el símbolo llega, y con cuarenta iconos en pantalla eso es un temblor
    general al abrir.
  -->
  <img
    v-if="fuente"
    :src="fuente"
    :class="clase"
    class="shrink-0 object-contain"
    alt=""
    aria-hidden="true"
  />
  <span v-else :class="clase" class="shrink-0" aria-hidden="true" />
</template>
