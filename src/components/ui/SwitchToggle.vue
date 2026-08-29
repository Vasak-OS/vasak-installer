<script setup lang="ts">
interface Props {
	modelValue: boolean;
	label: string;
	descripcion?: string;
	disabled?: boolean;
}
const props = withDefaults(defineProps<Props>(), { disabled: false });
const emit = defineEmits<{ 'update:modelValue': [valor: boolean] }>();
</script>

<template>
  <!--
    Un `<button role="switch">` de verdad y no un `div` con un `@click`: es lo
    que hace que el control se anuncie como interruptor, responda a la barra
    espaciadora y reciba el foco con Tab sin `tabindex` a mano.
  -->
  <button
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :disabled="disabled"
    class="flex w-full items-start gap-3 rounded-corner p-2 text-left transition-colors hover:bg-ui-surface/60 disabled:cursor-not-allowed disabled:opacity-60"
    @click="emit('update:modelValue', !props.modelValue)"
  >
    <span
      class="mt-0.5 flex h-5 w-9 shrink-0 items-center rounded-full border border-ui-border-strong px-0.5 transition-colors"
      :class="modelValue ? 'bg-primary' : 'bg-ui-surface'"
      aria-hidden="true"
    >
      <span
        class="h-3.5 w-3.5 rounded-full bg-ui-bg transition-transform"
        :class="modelValue ? 'translate-x-4' : 'translate-x-0'"
      />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block font-medium text-sm">{{ label }}</span>
      <span v-if="descripcion" class="mt-0.5 block text-tx-muted text-xs">{{ descripcion }}</span>
    </span>
  </button>
</template>
