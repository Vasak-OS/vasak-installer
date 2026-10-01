<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { ActionButton, AlertMessage, PageHeader, Panel } from '@vasakgroup/vue-libvasak';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();
const checking = ref(false);

/**
 * Cuánto se descarga, aproximadamente.
 *
 * Está escrito a mano y no calculado: calcularlo exigiría consultar los
 * repositorios antes de instalar, que es una llamada a la red para responder algo
 * que sólo tiene que dar el orden de magnitud. Lo que importa es que quien está
 * con datos móviles lo sepa antes de empezar.
 */
const APPROXIMATE_SIZE = '3 GB';

/**
 * Se vuelve a comprobar solo cada tanto.
 *
 * Es el único paso donde la persona tiene que irse a otra ventana —el panel de
 * red— y volver. Sin el sondeo automático, vuelve y el paso sigue diciendo que
 * no hay conexión hasta que descubre el botón.
 */
const INTERVAL_MS = 4000;
let timer: number | undefined;

const online = computed(() => store.sistema?.hay_red === true);

async function check() {
	checking.value = true;
	try {
		await store.comprobarRed();
	} finally {
		checking.value = false;
	}
}

onMounted(() => {
	check();
	// Un solo temporizador, y se detiene al salir del paso. Sin el `clearInterval`
	// el sondeo sigue corriendo durante toda la instalación, despertando el
	// proceso cada cuatro segundos para nada.
	timer = window.setInterval(() => {
		if (!online.value && !document.hidden) check();
	}, INTERVAL_MS);
});

onUnmounted(() => {
	if (timer !== undefined) window.clearInterval(timer);
});
</script>

<template>
  <div>
    <PageHeader class="mb-5" :icon="STEP_ICONS.red" icon-type="symbol" :title="t('red.titulo')" :description="t('red.intro')" />

    <Panel>
      <AlertMessage v-if="online" tone="success" icon="auto" :title="t('red.conectado')">
        {{ t('red.conectadoDetalle') }}
      </AlertMessage>
      <AlertMessage v-else tone="warning" icon="auto" :title="t('red.desconectado')">
        {{ t('red.desconectadoDetalle') }}
      </AlertMessage>

      <div class="mt-3">
        <ActionButton
          :label="checking ? t('comun.cargando') : t('red.volverAProbar')"
          variant="secondary"
          :loading="checking"
          @click="check"
        />
      </div>
    </Panel>

    <div class="mt-4 space-y-2 text-tx-muted text-xs">
      <p>{{ interpolar(t('red.descarga'), APPROXIMATE_SIZE) }}</p>
      <p>{{ t('red.cuidadoMedido') }}</p>
    </div>
  </div>
</template>
