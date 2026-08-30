<script setup lang="ts">
import IconoSistema from '@/components/ui/IconoSistema.vue';
import { ICONO_HECHO } from '@/tools/iconos';

interface Props {
	numero: number;
	titulo: string;
	descripcion: string;
	icono: string;
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
    class="group flex w-full items-center gap-3 rounded-corner border px-3 py-2 text-left transition-colors"
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
      class="relative flex size-9 shrink-0 items-center justify-center rounded-corner border transition-colors"
      :class="
        estado === 'hecho'
          ? 'border-status-success/60 bg-status-success/15'
          : estado === 'actual'
            ? 'border-secondary bg-primary/25'
            : 'border-ui-border-strong bg-ui-surface/40'
      "
      aria-hidden="true"
    >
      <IconoSistema :nombre="icono" clase="size-5" />

      <!--
        La marca de terminado es un emblema encima del icono del paso, no un
        reemplazo. Reemplazarlo dejaba nueve pasos completados con el mismo
        tilde: la barra perdía de un plumazo la única pista visual de qué era
        cada paso, justo cuando sirve para volver a uno.
        ·
        Es un glifo y no sólo un color, que es lo que pide WCAG 1.4.1: en escala
        de grises el verde y el violeta son el mismo gris.
      -->
      <span
        v-if="estado === 'hecho'"
        class="-right-1 -bottom-1 absolute flex size-4 items-center justify-center rounded-full bg-status-success"
      >
        <IconoSistema :nombre="ICONO_HECHO" clase="size-3" />
      </span>
    </span>

    <span class="min-w-0 flex-1">
      <span class="block truncate font-medium text-sm">{{ titulo }}</span>
      <span class="block truncate text-tx-muted text-xs">{{ descripcion }}</span>
    </span>

    <!-- El número queda, chico y al costado: dice cuántos faltan sin competir
         con el icono por el lugar principal. -->
    <span class="shrink-0 text-tx-muted text-xs tabular-nums" aria-hidden="true">
      {{ numero }}
    </span>
  </button>
</template>
