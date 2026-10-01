<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { ActionButton, AlertMessage, PageHeader, Panel } from '@vasakgroup/vue-libvasak';
import { ref } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();
const error = ref<string | null>(null);

async function run(command: 'reiniciar' | 'apagar') {
	error.value = null;
	try {
		await invoke(command);
	} catch (e) {
		// Que `systemctl` falle no invalida la instalación, que ya terminó. Se
		// muestra el error y la persona apaga a mano; decirle que algo falló sin
		// aclarar que el sistema quedó instalado sería alarmante y falso.
		error.value = String(e);
	}
}
</script>

<template>
  <div>
    <PageHeader class="mb-5" :icon="STEP_ICONS.fin" icon-type="symbol" :title="t('fin.titulo')" :description="t('fin.intro')" />

    <div class="space-y-4">
      <AlertMessage tone="success" icon="auto">
        {{ interpolar(t('fin.primerInicio'), store.eleccion.usuario) }}
      </AlertMessage>

      <AlertMessage v-if="store.eleccion.cifrar" tone="info" icon="auto">
        {{ t('fin.cifradoRecordatorio') }}
      </AlertMessage>

      <Panel>
        <div class="flex flex-wrap gap-2">
          <ActionButton :label="t('fin.reiniciar')" size="lg" @click="run('reiniciar')" />
          <ActionButton :label="t('fin.apagar')" variant="secondary" size="lg" @click="run('apagar')" />
        </div>
        <p class="mt-3 text-tx-muted text-xs">{{ t('fin.seguirEnVivoAyuda') }}</p>
      </Panel>

      <AlertMessage v-if="error" tone="error" icon="auto">{{ error }}</AlertMessage>
    </div>
  </div>
</template>
