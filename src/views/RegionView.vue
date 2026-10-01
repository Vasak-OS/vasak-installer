<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	ConfigSection,
	PageHeader,
	Panel,
	SearchSelect,
	SwitchRow,
	TextInput,
} from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { nombreDeIdioma, nombreDeZona } from '@/tools/formato';
import { STEP_ICONS } from '@/tools/icons';

const { t, locale } = useI18n();
const store = useInstalacionStore();

const zoneOptions = computed(() =>
	store.catalogos.zonas.map((zone) => {
		const { region, ciudad } = nombreDeZona(zone);
		return { valor: zone, etiqueta: ciudad, detalle: region };
	})
);

const languageOptions = computed(() =>
	store.catalogos.idiomas.map((code) => ({
		valor: code,
		etiqueta: nombreDeIdioma(code, locale.value),
		detalle: code,
	}))
);

// Sin catálogo no se puede ofrecer una lista, así que se deja escribir a mano.
// Un desplegable vacío deja el paso sin salida; un campo de texto al menos
// permite seguir con el valor que la persona sepa.
const noZones = computed(() => store.catalogos.zonas.length === 0);
const noLanguages = computed(() => store.catalogos.idiomas.length === 0);
</script>

<template>
  <div>
    <PageHeader class="mb-5" :icon="STEP_ICONS.region" icon-type="symbol" :title="t('region.titulo')" />

    <div class="space-y-4">
      <ConfigSection :title="t('region.zonaHoraria')" :description="t('region.zonaHorariaAyuda')" as="h2">
        <TextInput
          v-if="noZones"
          v-model="store.eleccion.zonaHoraria"
          :ariaLabel="t('region.zonaHoraria')"
          mono
          :placeholder="'America/Argentina/Buenos_Aires'"
        />
        <SearchSelect
          v-else
          v-model="store.eleccion.zonaHoraria"
          :label="t('region.zonaHoraria')"
          :options="zoneOptions"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
        <p v-if="noZones" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </ConfigSection>

      <ConfigSection :title="t('region.idiomaSistema')" :description="t('region.idiomaSistemaAyuda')" as="h2">
        <TextInput
          v-if="noLanguages"
          v-model="store.eleccion.idiomaSistema"
          :ariaLabel="t('region.idiomaSistema')"
          mono
          :placeholder="'es_AR'"
        />
        <SearchSelect
          v-else
          v-model="store.eleccion.idiomaSistema"
          :label="t('region.idiomaSistema')"
          :options="languageOptions"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
        <p v-if="noLanguages" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </ConfigSection>

      <Panel>
        <SwitchRow
          v-model="store.eleccion.ntp"
          :label="t('region.ntp')"
          :description="t('region.ntpAyuda')"
        />
      </Panel>

      <AlertMessage v-if="noZones || noLanguages" tone="warning" icon="auto">
        {{ t('region.sinCatalogo') }}
      </AlertMessage>
    </div>
  </div>
</template>
