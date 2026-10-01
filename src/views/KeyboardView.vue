<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	ConfigSection,
	PageHeader,
	SearchSelect,
	TextInput,
} from '@vasakgroup/vue-libvasak';
import { computed, ref } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';

const { t } = useI18n();
const store = useInstalacionStore();

/**
 * El campo de prueba.
 *
 * No se guarda en el almacén a propósito: es un borrador, y dejarlo en el estado
 * del asistente sería llevar hasta el resumen algo que nadie eligió.
 */
const sample = ref('');

const options = computed(() =>
	store.catalogos.teclados.map((layout) => ({ valor: layout, etiqueta: layout }))
);
const noLayouts = computed(() => store.catalogos.teclados.length === 0);
</script>

<template>
  <div>
    <PageHeader class="mb-5" :icon="STEP_ICONS.teclado" icon-type="symbol" :title="t('teclado.titulo')" :description="t('teclado.intro')" />

    <div class="space-y-4">
      <ConfigSection :title="t('teclado.distribucion')" as="h2">
        <TextInput
          v-if="noLayouts"
          v-model="store.eleccion.teclado"
          :ariaLabel="t('teclado.distribucion')"
          mono
          placeholder="la-latin1" />
        <SearchSelect
          v-else
          v-model="store.eleccion.teclado"
          :label="t('teclado.distribucion')"
          :options="options"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
      </ConfigSection>

      <ConfigSection :title="t('teclado.prueba')" as="h2">
        <TextInput v-model="sample" :placeholder="t('teclado.pruebaPlaceholder')" />
      </ConfigSection>

      <!--
        Honestidad sobre lo que este campo puede y no puede probar: la
        distribución elegida se aplica al sistema instalado, no al compositor que
        está dibujando esta ventana. Sin decirlo, alguien tipea, ve que sale
        `us`, y cree que el instalador ignoró su elección.
      -->
      <AlertMessage tone="info" icon="auto">{{ t('teclado.avisoNoAplica') }}</AlertMessage>
    </div>
  </div>
</template>
