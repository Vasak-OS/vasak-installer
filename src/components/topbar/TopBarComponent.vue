<script lang="ts" setup>
import ActionControlsComponent from '@/components/topbar/ActionControlsComponent.vue';
</script>
<template>
  <!--
    Tres zonas: identidad a la izquierda, título al medio, controles a la
    derecha.
    ·
    El título va **centrado en la ventana**, no centrado en el espacio que sobra.
    Por eso las tres zonas son `flex-1` con el mismo peso: si el título fuera un
    hermano flexible entre dos bloques de ancho distinto, se correría a la
    izquierda —los controles ocupan más que el icono— y quedaría desalineado del
    centro de la ventana, que es donde el ojo lo busca.
    ·
    `pointer-events-none` en el título con `pointer-events-auto` en los extremos:
    la franja del medio tiene que seguir arrastrando la ventana, y un texto que
    intercepta el clic deja una banda muerta justo donde la gente agarra para
    mover.
  -->
  <div
    class="flex shrink-0 items-center gap-2 border-ui-border border-b p-2"
    data-tauri-drag-region
  >
    <div class="flex flex-1 items-center gap-2" data-tauri-drag-region>
      <slot name="identidad" />
    </div>

    <div
      class="pointer-events-none flex min-w-0 items-center justify-center"
      data-tauri-drag-region
    >
      <slot name="titulo" />
    </div>

    <div class="flex flex-1 items-center justify-end" data-tauri-drag-region>
      <slot />
      <ActionControlsComponent />
    </div>
  </div>
</template>
