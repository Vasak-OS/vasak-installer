<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SelectorBuscable from '@/components/ui/SelectorBuscable.vue';
import SwitchToggle from '@/components/ui/SwitchToggle.vue';
import TextInput from '@/components/ui/TextInput.vue';
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
          mono
          :placeholder="'America/Argentina/Buenos_Aires'"
        />
        <SelectorBuscable
          v-else
          v-model="store.eleccion.zonaHoraria"
          :opciones="opcionesZona"
          :placeholder-busqueda="t('comun.buscar')"
          :texto-sin-resultados="t('comun.sinResultados')"
        />
        <p v-if="sinZonas" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </SectionCard>

      <SectionCard :titulo="t('region.idiomaSistema')" :descripcion="t('region.idiomaSistemaAyuda')">
        <TextInput
          v-if="sinIdiomas"
          v-model="store.eleccion.idiomaSistema"
          mono
          :placeholder="'es_AR'"
        />
        <SelectorBuscable
          v-else
          v-model="store.eleccion.idiomaSistema"
          :opciones="opcionesIdioma"
          :placeholder-busqueda="t('comun.buscar')"
          :texto-sin-resultados="t('comun.sinResultados')"
        />
        <p v-if="sinIdiomas" class="mt-2 text-tx-muted text-xs">{{ t('region.sinCatalogo') }}</p>
      </SectionCard>

      <SectionCard>
        <SwitchToggle
          v-model="store.eleccion.ntp"
          :label="t('region.ntp')"
          :descripcion="t('region.ntpAyuda')"
        />
      </SectionCard>

      <AlertMessage v-if="sinZonas || sinIdiomas" tipo="aviso">
        {{ t('region.sinCatalogo') }}
      </AlertMessage>
    </div>
  </div>
</template>
