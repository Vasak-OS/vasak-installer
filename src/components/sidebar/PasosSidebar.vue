<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed } from 'vue';
import PasoBoton from '@/components/sidebar/PasoBoton.vue';
import { PASOS, type Paso } from '@/stores/instalacion';

interface Props {
	actual: Paso;
	/** Falso una vez que se empezó a escribir en el disco. */
	navegable: boolean;
}
const props = defineProps<Props>();
defineEmits<{ ir: [paso: Paso] }>();

const { t } = useI18n();

const indiceActual = computed(() => PASOS.indexOf(props.actual));

function estado(indice: number): 'hecho' | 'actual' | 'pendiente' {
	if (indice < indiceActual.value) return 'hecho';
	if (indice === indiceActual.value) return 'actual';
	return 'pendiente';
}
</script>

<template>
  <!--
    `<nav>` con su nombre y una lista ordenada adentro: para un lector de
    pantalla esto es «navegación de pasos, lista de 9 elementos, elemento 5», que
    es exactamente la información que la barra le da a quien la ve.
  -->
  <nav
    :aria-label="t('pasos.bienvenida.titulo')"
    class="w-64 shrink-0 overflow-y-auto border-ui-border border-r p-3"
  >
    <ol class="space-y-1">
      <li v-for="(paso, indice) in PASOS" :key="paso">
        <PasoBoton
          :numero="indice + 1"
          :titulo="t(`pasos.${paso}.titulo`)"
          :descripcion="t(`pasos.${paso}.descripcion`)"
          :estado="estado(indice)"
          :navegable="navegable && indice < indiceActual"
          @click="$emit('ir', paso)"
        />
      </li>
    </ol>
  </nav>
</template>
