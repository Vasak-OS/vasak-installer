<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();
const comprobando = ref(false);

/**
 * Cuánto se descarga, aproximadamente.
 *
 * Está escrito a mano y no calculado: calcularlo exigiría consultar los
 * repositorios antes de instalar, que es una llamada a la red para responder algo
 * que sólo tiene que dar el orden de magnitud. Lo que importa es que quien está
 * con datos móviles lo sepa antes de empezar.
 */
const TAMANO_APROXIMADO = '3 GB';

/**
 * Se vuelve a comprobar solo cada tanto.
 *
 * Es el único paso donde la persona tiene que irse a otra ventana —el panel de
 * red— y volver. Sin el sondeo automático, vuelve y el paso sigue diciendo que
 * no hay conexión hasta que descubre el botón.
 */
const INTERVALO_MS = 4000;
let temporizador: number | undefined;

const hayRed = computed(() => store.sistema?.hay_red === true);

async function comprobar() {
	comprobando.value = true;
	try {
		await store.comprobarRed();
	} finally {
		comprobando.value = false;
	}
}

onMounted(() => {
	comprobar();
	// Un solo temporizador, y se detiene al salir del paso. Sin el `clearInterval`
	// el sondeo sigue corriendo durante toda la instalación, despertando el
	// proceso cada cuatro segundos para nada.
	temporizador = window.setInterval(() => {
		if (!hayRed.value && !document.hidden) comprobar();
	}, INTERVALO_MS);
});

onUnmounted(() => {
	if (temporizador !== undefined) window.clearInterval(temporizador);
});
</script>

<template>
  <div>
    <PageHeader :titulo="t('red.titulo')" :descripcion="t('red.intro')" />

    <SectionCard>
      <AlertMessage v-if="hayRed" tipo="exito" :titulo="t('red.conectado')">
        {{ t('red.conectadoDetalle') }}
      </AlertMessage>
      <AlertMessage v-else tipo="aviso" :titulo="t('red.desconectado')">
        {{ t('red.desconectadoDetalle') }}
      </AlertMessage>

      <button
        type="button"
        :disabled="comprobando"
        class="mt-3 rounded-corner border border-ui-border-strong px-3 py-1.5 text-sm transition-colors hover:bg-ui-surface disabled:opacity-60"
        @click="comprobar"
      >
        {{ comprobando ? t('comun.cargando') : t('red.volverAProbar') }}
      </button>
    </SectionCard>

    <div class="mt-4 space-y-2 text-tx-muted text-xs">
      <p>{{ interpolar(t('red.descarga'), TAMANO_APROXIMADO) }}</p>
      <p>{{ t('red.cuidadoMedido') }}</p>
    </div>
  </div>
</template>
