<script setup lang="ts">
/**
 * La lista de pasos de la instalación.
 *
 * El marco es el de `@vasakgroup/vue-libvasak`, el mismo que usan Configuración,
 * el monitor, la tienda y el gestor de archivos: mismo borde, misma esquina,
 * mismo fondo de superficie y mismo plegado. Que el instalador se lea como parte
 * del escritorio importa acá más que en ninguna otra ventana, porque es la
 * primera que alguien ve del sistema.
 *
 * Lo de adentro **no** son botones de la librería. Un paso no es un lugar al que
 * se va: tiene número, estado —hecho, actual, pendiente—, descripción, un
 * emblema de terminado y se apaga en cuanto se empieza a escribir en el disco.
 * Nada de eso lo tiene `SideButton`, así que el contenido va por la ranura y lo
 * dibuja `PasoBoton`, que es de acá.
 *
 * # Sin área de título
 *
 * El nombre de la aplicación ya está en la barra superior de la ventana.
 */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { SideBar } from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import PasoBoton from '@/components/sidebar/PasoBoton.vue';
import { PASOS, type Paso } from '@/stores/instalacion';
import { ICONO_PASO } from '@/tools/iconos';

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
  <SideBar
    :collapse-label="t('barraLateral.plegar')"
    :expand-label="t('barraLateral.desplegar')">
    <template #default="{ collapsed }">
      <!--
        Una lista ordenada con su nombre: para un lector de pantalla esto es
        «navegación de pasos, lista de 9 elementos, elemento 5», que es
        exactamente la información que la barra le da a quien la ve.
      -->
      <ol :aria-label="t('pasos.bienvenida.titulo')" class="flex flex-col gap-1">
        <li v-for="(paso, indice) in PASOS" :key="paso">
          <PasoBoton
            :numero="indice + 1"
            :titulo="t(`pasos.${paso}.titulo`)"
            :descripcion="t(`pasos.${paso}.descripcion`)"
            :icono="ICONO_PASO[paso]"
            :estado="estado(indice)"
            :plegado="collapsed"
            :navegable="navegable && indice < indiceActual"
            @click="$emit('ir', paso)"
          />
        </li>
      </ol>
    </template>
  </SideBar>
</template>
