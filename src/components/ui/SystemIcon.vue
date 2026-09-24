<script setup lang="ts">
/**
 * Un icono del tema del sistema.
 *
 * Es una capa fina sobre `ThemeIcon` de la librería, no una implementación: lo
 * único que agrega es el vocabulario del instalador —el tamaño por clases y el
 * papel decorativo— sobre lo que el componente compartido ya hace.
 *
 * **Antes no era así.** Acá vivían un composable propio y este componente, con
 * su caché, su oyente del cambio de tema y su propio criterio. Existían por una
 * razón que era cierta cuando se escribieron: `ThemeIcon` resolvía un icono por
 * instancia, y el instalador dibuja cerca de cuarenta al arrancar. Eso ya no
 * pasa — la librería memoriza por `tipo:nombre`, comparte el pedido en vuelo y
 * usa un solo oyente para toda la ventana—, así que la copia pasó a ser una
 * versión peor: nunca soltaba su oyente, y una resolución que volvía tarde se
 * memorizaba como si fuera del tema nuevo.
 *
 * `type="icon"` trae la versión a color —la que el escritorio muestra en todas
 * partes— y `type="symbol"` el glifo monocromo. La elección no es estética: un
 * navegador se reconoce por su logo, y en glifo monocromo Firefox, Chromium y
 * Brave son tres contornos indistinguibles. Al revés, un icono a color en la
 * barra lateral a 16 píxeles es una mancha.
 *
 * Siempre decorativo: `alt` vacío y `aria-hidden`. Un icono al lado de un texto
 * que dice lo mismo, anunciado por un lector de pantalla, es el texto repetido
 * dos veces. Cuando el icono **es** la única información —un botón sin texto—,
 * el nombre va en el `aria-label` del botón, no acá.
 */
import { ThemeIcon } from '@vasakgroup/vue-libvasak';

interface Props {
	name: string;
	type?: 'icon' | 'symbol';
	/**
	 * Clases de tamaño. El tema dibuja en 16px de base, así que `size-4` es 1:1.
	 *
	 * Se llama `sizeClass` y no `class`: `class` es un atributo reservado que
	 * Vue trata aparte —se fusiona con el de afuera en vez de llegar como
	 * propiedad—, así que declararlo sería pelearse con el marco. Es el mismo
	 * nombre que ya usa el icono del clima en `vasak-desktop`, por lo mismo.
	 */
	sizeClass?: string;
}
const props = withDefaults(defineProps<Props>(), { type: 'symbol', sizeClass: 'size-4' });
</script>

<template>
  <!--
    `size="auto"` es lo que hace que el tamaño lo pongan las clases: con un
    número, `ThemeIcon` escribe el alto y el ancho en línea y una regla en línea
    le gana a cualquier clase. Necesita `@vasakgroup/vue-libvasak` 1.5.0.

    El hueco mientras resuelve lo dibuja la propia librería, del mismo tamaño,
    así que el texto de al lado ya no salta cuando el icono llega — que es para
    lo que estaba el `span` de respaldo que había acá.

    `aria-hidden` va por `v-bind` y no como atributo suelto por un agujero de
    los tipos: `ThemeIcon` declara sus propiedades y con `strictTemplates`
    `vue-tsc` mide contra esa lista, así que rechaza cualquier atributo que no
    esté ahí. Llega igual —el componente no apaga `inheritAttrs`—; lo que no
    llega es al tipo.
  -->
  <ThemeIcon
    :name="props.name"
    :type="props.type"
    size="auto"
    alt=""
    v-bind="{ 'aria-hidden': 'true' }"
    :class="props.sizeClass"
  />
</template>
