<script lang="ts" setup>
/**
 * La ventana del instalador.
 *
 * No dibuja nada propio: el borde, la esquina, el fondo y la barra salen de
 * `WindowFrame`, que es el mismo de todas las ventanas del escritorio. Estaba
 * copiado acá, y ya había derivado de las copias vecinas.
 *
 * # Sin los tres botones
 *
 * `:controls="[]"`. Minimizar o cerrar el instalador mientras está
 * particionando un disco deja el equipo a medio instalar, y el botón de la
 * barra no distingue en qué paso está.
 *
 * La salida va por `acciones`, que es donde la aplicación pone lo suyo: un
 * «salir» que pregunta antes, y que durante la instalación dice qué queda en el
 * disco. Sin él esto sería una ventana sin ninguna salida, que es peor que el
 * problema que se estaba evitando.
 */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { WindowFrame } from '@vasakgroup/vue-libvasak';

const { t } = useI18n();
</script>

<template>
  <WindowFrame :controls="[]" :title="t('app.nombre')">
    <template v-if="$slots.identidad" #identidad><slot name="identidad" /></template>
    <template v-if="$slots.titulo" #titulo><slot name="titulo" /></template>
    <template v-if="$slots.acciones" #acciones><slot name="acciones" /></template>

    <div class="flex min-h-0 min-w-0 flex-1">
      <slot />
    </div>
  </WindowFrame>
</template>
