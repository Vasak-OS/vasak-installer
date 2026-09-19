<script lang="ts" setup>
/**
 * La ventana del instalador.
 *
 * No dibuja nada propio: el borde, la esquina, el fondo, la barra y los botones
 * salen de `WindowFrame`, que es el mismo de todas las ventanas del escritorio.
 *
 * # Los botones
 *
 * Quién los decide es `App.vue`, que es quien sabe en qué paso está: los tres,
 * salvo **cerrar mientras el ayudante está escribiendo el disco**. Minimizar y
 * maximizar se quedan siempre —perder la ventana de vista y no poder traerla de
 * vuelta sería el problema contrario—, y mientras la instalación corre la
 * salida es el «cancelar» de esa pantalla, que pregunta y detiene al ayudante
 * antes.
 */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { type ControlDeVentana, LOS_TRES_CONTROLES, WindowFrame } from '@vasakgroup/vue-libvasak';

withDefaults(defineProps<{ controls?: ControlDeVentana[] }>(), {
	controls: () => LOS_TRES_CONTROLES,
});

const { t } = useI18n();
</script>

<template>
  <WindowFrame
    :controls="controls"
    :title="t('app.nombre')"
    :minimize-label="t('ventana.minimizar')"
    :maximize-label="t('ventana.maximizar')"
    :close-label="t('ventana.cerrar')">
    <template v-if="$slots.identidad" #identidad><slot name="identidad" /></template>
    <template v-if="$slots.titulo" #titulo><slot name="titulo" /></template>
    <template v-if="$slots.acciones" #acciones><slot name="acciones" /></template>

    <div class="flex min-h-0 min-w-0 flex-1">
      <slot />
    </div>
  </WindowFrame>
</template>
