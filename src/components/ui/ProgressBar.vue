<script setup lang="ts">
interface Props {
	/** 0 a 1. `null` para una barra indefinida, cuando no se puede saber. */
	valor: number | null;
	label: string;
}
defineProps<Props>();
</script>

<template>
  <div
    role="progressbar"
    :aria-valuenow="valor === null ? undefined : Math.round(valor * 100)"
    aria-valuemin="0"
    aria-valuemax="100"
    :aria-label="label"
    class="h-2 w-full overflow-hidden rounded-full bg-ui-surface"
  >
    <div
      v-if="valor !== null"
      class="h-full rounded-full bg-primary transition-[width] duration-300"
      :style="{ width: `${Math.max(0, Math.min(1, valor)) * 100}%` }"
    />
    <!--
      Sin fracción conocida, una banda que se desliza. La animación es infinita,
      así que `prefers-reduced-motion` la detiene desde main.css — sin eso, una
      barra que se mueve sin parar durante media hora es exactamente lo que
      marea a alguien con trastorno vestibular.
    -->
    <div v-else class="h-full w-1/3 animate-[indefinida_1.4s_ease-in-out_infinite] rounded-full bg-primary" />
  </div>
</template>
