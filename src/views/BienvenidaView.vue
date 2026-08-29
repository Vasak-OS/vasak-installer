<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import Icono from '@/components/ui/Icono.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { PASOS, useInstalacionStore } from '@/stores/instalacion';
import { formatearBytes } from '@/tools/formato';
import { ICONO_EQUIPO, ICONO_PASO } from '@/tools/iconos';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

/** Por debajo de esto el escritorio va justo y conviene decirlo. */
const MEMORIA_JUSTA = 4 * 1024 * 1024 * 1024;

const memoria = computed(() =>
	store.sistema ? formatearBytes(store.sistema.memoria_bytes, locale.value) : '—'
);
const pocaMemoria = computed(
	() => store.sistema !== null && store.sistema.memoria_bytes < MEMORIA_JUSTA
);
</script>

<template>
  <div>
    <PageHeader :icono="ICONO_PASO.bienvenida" :titulo="t('bienvenida.titulo')" :descripcion="interpolar(t('bienvenida.intro'), PASOS.length)" />

    <SectionCard :titulo="t('bienvenida.equipo')">
      <dl v-if="store.sistema" class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
        <dt class="flex items-center gap-2 text-tx-muted">
          <Icono :nombre="ICONO_EQUIPO.procesador" />
          {{ t('bienvenida.procesador') }}
        </dt>
        <dd>
          {{ store.sistema.cpu || '—' }}
          <span class="text-tx-muted text-xs">
            · {{ interpolar(t('bienvenida.hilos'), store.sistema.nucleos) }}
          </span>
        </dd>

        <dt class="flex items-center gap-2 text-tx-muted">
          <Icono :nombre="ICONO_EQUIPO.memoria" />
          {{ t('bienvenida.memoria') }}
        </dt>
        <dd>{{ memoria }}</dd>

        <dt class="flex items-center gap-2 text-tx-muted">
          <Icono :nombre="ICONO_EQUIPO.firmware" />
          {{ t('bienvenida.firmware') }}
        </dt>
        <dd>
          {{ store.sistema.firmware === 'uefi' ? t('bienvenida.firmwareUefi') : t('bienvenida.firmwareBios') }}
        </dd>

        <template v-if="store.sistema.virtualizacion">
          <dt class="flex items-center gap-2 text-tx-muted">
          <Icono :nombre="ICONO_EQUIPO.virtualizacion" />
          {{ t('bienvenida.virtualizacion') }}
        </dt>
          <dd class="font-mono text-xs">{{ store.sistema.virtualizacion }}</dd>
        </template>
      </dl>
      <p v-else class="text-tx-muted text-sm">{{ t('comun.cargando') }}</p>
    </SectionCard>

    <div class="mt-4 space-y-3">
      <AlertMessage
        v-if="store.sistema?.virtualizacion"
        tipo="info"
      >
        {{ t('bienvenida.avisoVirtual') }}
      </AlertMessage>

      <AlertMessage v-if="pocaMemoria" tipo="aviso" :titulo="t('bienvenida.avisoMemoriaTitulo')">
        {{ interpolar(t('bienvenida.avisoMemoria'), memoria) }}
      </AlertMessage>

      <AlertMessage
        v-if="store.sistema?.firmware === 'bios'"
        tipo="aviso"
        :titulo="t('bienvenida.avisoBiosTitulo')"
      >
        {{ t('bienvenida.avisoBios') }}
      </AlertMessage>
    </div>
  </div>
</template>
