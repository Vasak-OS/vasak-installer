<script setup lang="ts">
/**
 * El aviso del asistente: el de la librería, con su icono.
 *
 * Lo que decide el color, y que un error **interrumpa** a un lector de pantalla
 * mientras lo demás espera su turno, es de la librería y no se repite acá. Lo
 * único que pone el instalador es el icono, que de las seis copias de aviso que
 * había en el sistema era el único que lo tenía: acá un aviso puede estar
 * diciendo que se va a borrar un disco, y el icono es lo que lo hace mirar.
 *
 * Quedan dieciséis líneas de las sesenta y cuatro que eran.
 */
import { AlertMessage, type TonoDelAviso } from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import { ICONO_MENSAJE } from '@/tools/iconos';

const props = withDefaults(defineProps<{ tone?: TonoDelAviso; title?: string }>(), {
	tone: 'info',
});

const icono = computed(() => ICONO_MENSAJE[props.tone]);
</script>

<template>
  <AlertMessage :tone="tone" :title="title" :icon="icono"><slot /></AlertMessage>
</template>
