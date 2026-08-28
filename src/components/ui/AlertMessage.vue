<script setup lang="ts">
import { computed } from 'vue';

interface Props {
	tipo?: 'info' | 'aviso' | 'error' | 'exito';
	titulo?: string;
}
const props = withDefaults(defineProps<Props>(), { tipo: 'info' });

const clases = computed(() => {
	switch (props.tipo) {
		case 'aviso':
			return 'border-status-warning bg-status-warning/10';
		case 'error':
			return 'border-status-error bg-status-error/10';
		case 'exito':
			return 'border-status-success bg-status-success/10';
		default:
			return 'border-ui-border-strong bg-ui-surface/50';
	}
});

// `alert` para lo que interrumpe y `status` para lo que informa: un lector de
// pantalla anuncia el primero de inmediato y el segundo cuando termina lo que
// está diciendo. Marcar todo como `alert` hace que la interfaz interrumpa por
// cualquier cosa, y entonces nadie le presta atención a lo que importa.
const rol = computed(() => (props.tipo === 'error' ? 'alert' : 'status'));
</script>

<template>
  <div :role="rol" class="rounded-corner border p-3" :class="clases">
    <p v-if="titulo" class="font-semibold text-sm">{{ titulo }}</p>
    <div class="text-tx-muted text-xs" :class="titulo ? 'mt-1' : ''">
      <slot />
    </div>
  </div>
</template>
