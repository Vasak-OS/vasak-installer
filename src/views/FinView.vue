<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { ref } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { ICONO_PASO } from '@/tools/iconos';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();
const error = ref<string | null>(null);

async function accion(comando: 'reiniciar' | 'apagar') {
	error.value = null;
	try {
		await invoke(comando);
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
    <PageHeader :icono="ICONO_PASO.fin" :titulo="t('fin.titulo')" :descripcion="t('fin.intro')" />

    <div class="space-y-4">
      <AlertMessage tipo="exito">
        {{ interpolar(t('fin.primerInicio'), store.eleccion.usuario) }}
      </AlertMessage>

      <AlertMessage v-if="store.eleccion.cifrar" tipo="info">
        {{ t('fin.cifradoRecordatorio') }}
      </AlertMessage>

      <SectionCard>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            class="rounded-corner bg-primary px-4 py-2 font-medium text-sm text-tx-on-primary transition-opacity hover:opacity-90"
            @click="accion('reiniciar')"
          >
            {{ t('fin.reiniciar') }}
          </button>
          <button
            type="button"
            class="rounded-corner border border-ui-border-strong px-4 py-2 text-sm transition-colors hover:bg-ui-surface"
            @click="accion('apagar')"
          >
            {{ t('fin.apagar') }}
          </button>
        </div>
        <p class="mt-3 text-tx-muted text-xs">{{ t('fin.seguirEnVivoAyuda') }}</p>
      </SectionCard>

      <AlertMessage v-if="error" tipo="error">{{ error }}</AlertMessage>
    </div>
  </div>
</template>
