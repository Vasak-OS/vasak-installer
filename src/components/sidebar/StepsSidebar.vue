<script setup lang="ts">
/**
 * La barra lateral de los pasos.
 *
 * El marco es el de `@vasakgroup/vue-libvasak`, el mismo que usan Configuración,
 * el monitor, la tienda y el gestor de archivos: mismo borde, misma esquina,
 * mismo fondo de superficie y mismo plegado. Que el instalador se lea como parte
 * del escritorio importa acá más que en ninguna otra ventana, porque es la
 * primera que alguien ve del sistema. Los pasos los dibuja `StepList`.
 *
 * # Sin área de título
 *
 * El nombre de la aplicación ya está en la barra superior de la ventana.
 */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { SideBar } from '@vasakgroup/vue-libvasak';
import StepList from '@/components/sidebar/StepList.vue';
import type { Paso } from '@/stores/instalacion';

interface Props {
	current: Paso;
	/** Falso una vez que se empezó a escribir en el disco. */
	navigable: boolean;
}
defineProps<Props>();
defineEmits<{ go: [step: Paso] }>();

const { t } = useI18n();
</script>

<template>
  <SideBar
    :collapse-label="t('barraLateral.plegar')"
    :expand-label="t('barraLateral.desplegar')">
    <template #default="{ collapsed }">
      <StepList
        :current="current"
        :navigable="navigable"
        :collapsed="collapsed"
        @go="$emit('go', $event)" />
    </template>
  </SideBar>
</template>
