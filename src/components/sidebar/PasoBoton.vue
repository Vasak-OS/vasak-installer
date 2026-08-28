<script setup lang="ts">
interface Props {
	numero: number;
	titulo: string;
	descripcion: string;
	estado: 'hecho' | 'actual' | 'pendiente';
	/** Se puede volver a este paso. Falso una vez que la instalación arrancó. */
	navegable: boolean;
}
defineProps<Props>();
defineEmits<{ click: [] }>();
</script>

<template>
  <button
    type="button"
    :disabled="!navegable"
    :aria-current="estado === 'actual' ? 'step' : undefined"
    class="flex w-full items-start gap-3 rounded-corner border px-3 py-2 text-left transition-colors"
    :class="[
      estado === 'actual'
        ? 'border-secondary bg-primary/15'
        : 'border-transparent hover:border-ui-border hover:bg-ui-surface/60',
      navegable ? 'cursor-pointer' : 'cursor-default',
      estado === 'pendiente' ? 'opacity-60' : '',
    ]"
    @click="$emit('click')"
  >
    <span
      class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border font-semibold text-xs"
      :class="
        estado === 'hecho'
          ? 'border-status-success bg-status-success/20'
          : estado === 'actual'
            ? 'border-secondary bg-primary/25'
            : 'border-ui-border-strong'
      "
      aria-hidden="true"
    >
      <!-- El número se reemplaza por una marca cuando el paso ya está: el
           estado tiene que verse sin depender del color, que es lo que pide
           WCAG 1.4.1 y lo que hace que se entienda en escala de grises. -->
      <template v-if="estado === 'hecho'">✓</template>
      <template v-else>{{ numero }}</template>
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate font-medium text-sm">{{ titulo }}</span>
      <span class="block truncate text-tx-muted text-xs">{{ descripcion }}</span>
    </span>
  </button>
</template>
