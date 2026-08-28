<script setup lang="ts">
interface Props {
	titulo: string;
	descripcion?: string;
}
defineProps<Props>();
</script>

<template>
  <header class="mb-5">
    <h1 class="font-semibold text-xl">{{ titulo }}</h1>
    <p v-if="descripcion" class="mt-1 max-w-prose text-tx-muted text-sm">{{ descripcion }}</p>
  </header>
</template>
