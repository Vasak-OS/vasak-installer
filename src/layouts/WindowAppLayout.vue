<script lang="ts" setup>
import TopBarComponent from '@/components/topbar/TopBarComponent.vue';
</script>
<template>
  <div
    class="flex h-screen w-screen flex-col overflow-hidden rounded-corner-window border border-ui-border bg-ui-bg/80">
    <!-- El `slot` de la barra superior se reexpone acá. `TopBarComponent` ya
         tenía uno para la izquierda de la barra, pero el layout no lo pasaba,
         así que desde una aplicación no había forma de llegar a él: la barra
         quedaba con los tres botones de la ventana flotando sobre nada. Y como
         la ventana no lleva decoración del compositor, el nombre de la
         aplicación no aparecía en ningún otro lado. -->
    <TopBarComponent>
      <template #identidad><slot name="identidad" /></template>
      <template #titulo><slot name="titulo" /></template>
    </TopBarComponent>
    <!-- El `slot` es lo que hace usable este layout.
         Sin él, `<WindowAppLayout>…</WindowAppLayout>` descartaba en silencio todo
         lo que se le pusiera dentro y la ventana abría vacía con el relleno de la
         plantilla todavía puesto. En vasak-monitor costó una compilación y una
         captura darse cuenta, porque no hay ningún error: simplemente no aparece
         nada. -->
    <div class="flex min-h-0 flex-1">
      <slot>
        <p class="p-4 text-tx-muted text-sm">
          Poné el contenido de la aplicación dentro de
          <code>&lt;WindowAppLayout&gt;</code>.
        </p>
      </slot>
    </div>
  </div>
</template>
