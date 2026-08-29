<script setup lang="ts">
import { computed } from 'vue';
import Icono from '@/components/ui/Icono.vue';
import { ICONO_MENSAJE } from '@/tools/iconos';

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

const colorIcono = computed(() => {
	switch (props.tipo) {
		case 'aviso':
			return 'text-status-warning';
		case 'error':
			return 'text-status-error';
		case 'exito':
			return 'text-status-success';
		default:
			return 'text-tx-muted';
	}
});

const icono = computed(() => ICONO_MENSAJE[props.tipo]);

// `alert` para lo que interrumpe y `status` para lo que informa: un lector de
// pantalla anuncia el primero de inmediato y el segundo cuando termina lo que
// está diciendo. Marcar todo como `alert` hace que la interfaz interrumpa por
// cualquier cosa, y entonces nadie le presta atención a lo que importa.
const rol = computed(() => (props.tipo === 'error' ? 'alert' : 'status'));
</script>

<template>
  <div :role="rol" class="flex gap-3 rounded-corner border p-3" :class="clases">
    <!--
      El icono va arriba a la izquierda y alineado con la primera línea, no
      centrado en la caja: con un mensaje de cinco líneas, un icono centrado
      queda flotando a la mitad del párrafo y deja de leerse como su marca.
    -->
    <span class="mt-0.5" :class="colorIcono">
      <Icono :nombre="icono" clase="size-5" />
    </span>
    <div class="min-w-0 flex-1">
      <p v-if="titulo" class="font-semibold text-sm">{{ titulo }}</p>
      <div class="text-tx-muted text-xs" :class="titulo ? 'mt-1' : ''">
        <slot />
      </div>
    </div>
  </div>
</template>
