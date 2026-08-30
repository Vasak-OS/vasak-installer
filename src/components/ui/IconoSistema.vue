<script setup lang="ts">
/**
 * Un icono del tema del sistema.
 *
 * Se llama `IconoSistema` y no `Icono` porque un nombre de componente de una
 * sola palabra puede chocar con un elemento HTML —hoy o cuando la especificación
 * agregue uno— y además dice menos: éste trae el icono **del tema del sistema**,
 * no un icono cualquiera.
 *
 * `tipo="icono"` trae la versión a color —la que el escritorio muestra en todas
 * partes— y `tipo="simbolo"` el glifo monocromo. La elección no es estética: un
 * navegador se reconoce por su logo, y en glifo monocromo Firefox, Chromium y
 * Brave son tres contornos indistinguibles. Al revés, un icono a color en la
 * barra lateral a 16 píxeles es una mancha.
 *
 * Siempre decorativo: `alt` vacío y `aria-hidden`. Un icono al lado de un texto
 * que dice lo mismo, anunciado por un lector de pantalla, es el texto repetido
 * dos veces. Cuando el icono **es** la única información —un botón sin texto—,
 * el nombre va en el `aria-label` del botón, no acá.
 */
import { toRef } from 'vue';
import { type TipoIcono, useIcono } from '@/composables/useIcono';

interface Props {
	nombre: string;
	tipo?: TipoIcono;
	/** Clases de tamaño. El tema dibuja en 16px de base, así que `size-4` es 1:1. */
	clase?: string;
}
const props = withDefaults(defineProps<Props>(), { tipo: 'simbolo', clase: 'size-4' });

const nombre = toRef(props, 'nombre');
const tipo = toRef(props, 'tipo');
const fuente = useIcono(
	() => nombre.value,
	() => tipo.value
);
</script>

<template>
  <!--
    El hueco se reserva aunque el icono todavía no haya resuelto o el tema no lo
    tenga: sin el `span` de respaldo, el texto de al lado salta unos píxeles
    cuando el icono llega, y con cuarenta iconos en pantalla eso es un temblor
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
