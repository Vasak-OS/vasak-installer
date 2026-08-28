<script setup lang="ts">
import { computed } from 'vue';

interface Props {
	modelValue: string;
	type?: 'text' | 'password' | 'search';
	id?: string;
	placeholder?: string;
	disabled?: boolean;
	invalid?: boolean;
	mono?: boolean;
	autocomplete?: string;
	/** Descripción del error o la ayuda, para `aria-describedby`. */
	describedBy?: string;
}

const props = withDefaults(defineProps<Props>(), {
	type: 'text',
	disabled: false,
	invalid: false,
	mono: false,
});

const emit = defineEmits<{ 'update:modelValue': [valor: string] }>();

const clases = computed(() => [
	'w-full rounded-corner border bg-ui-bg/60 px-3 py-2 text-sm transition-colors',
	'focus:border-primary focus:outline-none focus:ring-2 focus:ring-primary/20',
	'disabled:cursor-not-allowed disabled:opacity-50',
	// `--ui-border-strong` y no `--ui-border`: el borde suave da 1,17 de
	// contraste contra el fondo, contra el mínimo de 3,0 que pide WCAG 1.4.11
	// para lo que delimita un control. A 1,17 el contorno de un campo no se ve.
	props.invalid ? 'border-status-error' : 'border-ui-border-strong',
	props.mono ? 'font-mono' : '',
]);
</script>

<template>
  <input
    :id="id"
    :type="type"
    :value="modelValue"
    :placeholder="placeholder"
    :disabled="disabled"
    :autocomplete="autocomplete"
    :aria-invalid="invalid || undefined"
    :aria-describedby="describedBy"
    :class="clases"
    @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
  />
</template>
