<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { SearchSelect, SwitchRow, TextInput } from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { nombreDeIdioma, nombreDeZona } from '@/tools/formato';
import { ICONO_PASO } from '@/tools/iconos';

const { t, locale } = useI18n();
const store = useInstalacionStore();

const opcionesZona = computed(() =>
	store.catalogos.zonas.map((zona) => {
		const { region, ciudad } = nombreDeZona(zona);
		return { valor: zona, etiqueta: ciudad, detalle: region };
	})
);

const opcionesIdioma = computed(() =>
	store.catalogos.idiomas.map((local) => ({
		valor: local,
		etiqueta: nombreDeIdioma(local, locale.value),
		detalle: local,
	}))
);

// Sin catálogo no se puede ofrecer una lista, así que se deja escribir a mano.
// Un desplegable vacío deja el paso sin salida; un campo de texto al menos
// permite seguir con el valor que la persona sepa.
const sinZonas = computed(() => store.catalogos.zonas.length === 0);
const sinIdiomas = computed(() => store.catalogos.idiomas.length === 0);
</script>

<template>
  <div>
    <PageHeader :icono="ICONO_PASO.region" :titulo="t('region.titulo')" />

    <div class="space-y-4">
      <SectionCard :titulo="t('region.zonaHoraria')" :descripcion="t('region.zonaHorariaAyuda')">
        <TextInput
          v-if="sinZonas"
          v-model="store.eleccion.zonaHoraria"
          :ariaLabel="t('region.zonaHoraria')"
          mono
          :placeholder="'America/Argentina/Buenos_Aires'"
        />
        <SearchSelect
          v-else
          v-model="store.eleccion.zonaHoraria"
          :label="t('region.zonaHoraria')"
          :options="opcionesZona"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
        <p v-if="sinZonas" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </SectionCard>

      <SectionCard :titulo="t('region.idiomaSistema')" :descripcion="t('region.idiomaSistemaAyuda')">
        <TextInput
          v-if="sinIdiomas"
          v-model="store.eleccion.idiomaSistema"
          :ariaLabel="t('region.idiomaSistema')"
          mono
          :placeholder="'es_AR'"
        />
        <SearchSelect
          v-else
          v-model="store.eleccion.idiomaSistema"
          :label="t('region.idiomaSistema')"
          :options="opcionesIdioma"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
        <p v-if="sinIdiomas" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </SectionCard>

      <SectionCard>
        <SwitchRow
          v-model="store.eleccion.ntp"
          :label="t('region.ntp')"
          :description="t('region.ntpAyuda')"
        />
      </SectionCard>

      <AlertMessage v-if="sinZonas || sinIdiomas" tone="warning">
        {{ t('region.sinCatalogo') }}
      </AlertMessage>
    </div>
  </div>
</template>
