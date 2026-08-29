<script setup lang="ts">
/**
 * Una opción de un grupo excluyente, con icono.
 *
 * Reemplaza el uso de `SwitchToggle` para elegir el sistema de archivos, que
 * estaba mal de dos maneras. Semánticamente: tres interruptores dicen «podés
 * encender los que quieras» y acá sólo puede haber uno, así que un lector de
 * pantalla anunciaba tres controles independientes. Y visualmente: apagar uno
 * apretando otro es un comportamiento que un interruptor no tiene, y hacía
 * dudar de si algo se había roto.
 *
 * `role="radio"` dentro de un `radiogroup` es lo que corresponde: se anuncia
 * como «opción 2 de 3» y el estado del grupo entero se entiende de una.
 */
import Icono from '@/components/ui/Icono.vue';

interface Props {
	seleccionada: boolean;
	label: string;
	descripcion?: string;
	icono?: string;
	/**
	 * Con qué versión del tema se dibuja el icono.
	 *
	 * A color donde el icono **es** la cosa que se elige —un navegador, un
	 * sistema de archivos—, que es el caso de casi todas las opciones.
	 */
	tipoIcono?: 'icono' | 'simbolo';
	disabled?: boolean;
}
withDefaults(defineProps<Props>(), { disabled: false, tipoIcono: 'icono' });
defineEmits<{ elegir: [] }>();
</script>

<template>
  <button
    type="button"
    role="radio"
    :aria-checked="seleccionada"
    :disabled="disabled"
    class="flex w-full items-start gap-3 rounded-corner border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60"
    :class="
      seleccionada
        ? 'border-secondary bg-primary/10'
        : 'border-ui-border-strong hover:bg-ui-surface/60'
    "
    @click="$emit('elegir')"
  >
    <span
      v-if="icono"
      class="flex size-10 shrink-0 items-center justify-center rounded-corner border"
      :class="seleccionada ? 'border-secondary bg-primary/20' : 'border-ui-border bg-ui-surface/40'"
      aria-hidden="true"
    >
      <Icono :nombre="icono" :tipo="tipoIcono" clase="size-6" />
    </span>

    <span class="min-w-0 flex-1">
      <span class="block font-medium text-sm">{{ label }}</span>
      <span v-if="descripcion" class="mt-0.5 block text-tx-muted text-xs">{{ descripcion }}</span>
    </span>

    <!-- El punto de radio, además del color del borde: en escala de grises el
         violeta del seleccionado y el gris del resto son el mismo tono. -->
    <span
      class="mt-1 flex size-4 shrink-0 items-center justify-center rounded-full border"
      :class="seleccionada ? 'border-secondary' : 'border-ui-border-strong'"
      aria-hidden="true"
    >
      <span v-if="seleccionada" class="size-2 rounded-full bg-primary" />
    </span>
  </button>
</template>
