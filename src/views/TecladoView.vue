<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { SearchSelect, TextInput } from '@vasakgroup/vue-libvasak';
import { computed, ref } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { ICONO_PASO } from '@/tools/iconos';

const { t } = useI18n();
const store = useInstalacionStore();

/**
 * El campo de prueba.
 *
 * No se guarda en el almacén a propósito: es un borrador, y dejarlo en el estado
 * del asistente sería llevar hasta el resumen algo que nadie eligió.
 */
const prueba = ref('');

const opciones = computed(() =>
	store.catalogos.teclados.map((teclado) => ({ valor: teclado, etiqueta: teclado }))
);
const sinTeclados = computed(() => store.catalogos.teclados.length === 0);
</script>

<template>
  <div>
    <PageHeader :icono="ICONO_PASO.teclado" :titulo="t('teclado.titulo')" :descripcion="t('teclado.intro')" />

    <div class="space-y-4">
      <SectionCard :titulo="t('teclado.distribucion')">
        <TextInput
          v-if="sinTeclados"
          v-model="store.eleccion.teclado"
          :ariaLabel="t('teclado.distribucion')"
          mono
          placeholder="la-latin1" />
        <SearchSelect
          v-else
          v-model="store.eleccion.teclado"
          :label="t('teclado.distribucion')"
          :options="opciones"
          :search-placeholder="t('comun.buscar')"
          :empty-text="t('comun.sinResultados')"
        />
      </SectionCard>

      <SectionCard :titulo="t('teclado.prueba')">
        <TextInput v-model="prueba" :placeholder="t('teclado.pruebaPlaceholder')" />
      </SectionCard>

      <!--
        Honestidad sobre lo que este campo puede y no puede probar: la
        distribución elegida se aplica al sistema instalado, no al compositor que
        está dibujando esta ventana. Sin decirlo, alguien tipea, ve que sale
        `us`, y cree que el instalador ignoró su elección.
      -->
      <AlertMessage tone="info">{{ t('teclado.avisoNoAplica') }}</AlertMessage>
    </div>
  </div>
</template>
