<script setup lang="ts">
import IconoSistema from '@/components/ui/IconoSistema.vue';

interface Props {
	titulo: string;
	descripcion?: string;
	/** Símbolo del tema. Sin él, el encabezado sale sólo con texto. */
	icono?: string;
}
defineProps<Props>();
</script>

<template>
  <header class="mb-5 flex items-start gap-3">
    <!--
      El icono repite el de la barra lateral a propósito: es lo que ata la
      pantalla al paso en el que estás. Va más grande porque acá es la marca de
      la página, no un elemento de una lista.
    -->
    <span
      v-if="icono"
      class="mt-0.5 flex size-10 shrink-0 items-center justify-center rounded-corner border border-ui-border-strong bg-ui-surface/50"
    >
      <IconoSistema :nombre="icono" clase="size-6" />
    </span>
    <div class="min-w-0">
      <h1 class="font-semibold text-xl">{{ titulo }}</h1>
      <p v-if="descripcion" class="mt-1 max-w-prose text-tx-muted text-sm">{{ descripcion }}</p>
    </div>
  </header>
</template>
