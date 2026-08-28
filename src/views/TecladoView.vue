<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, ref } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SelectorBuscable from '@/components/ui/SelectorBuscable.vue';
import TextInput from '@/components/ui/TextInput.vue';
import { useInstalacionStore } from '@/stores/instalacion';

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
    <PageHeader :titulo="t('teclado.titulo')" :descripcion="t('teclado.intro')" />

    <div class="space-y-4">
      <SectionCard :titulo="t('teclado.distribucion')">
        <TextInput v-if="sinTeclados" v-model="store.eleccion.teclado" mono placeholder="la-latin1" />
        <SelectorBuscable
          v-else
          v-model="store.eleccion.teclado"
          :opciones="opciones"
          :placeholder-busqueda="t('comun.buscar')"
          :texto-sin-resultados="t('comun.sinResultados')"
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
      <AlertMessage tipo="info">{{ t('teclado.avisoNoAplica') }}</AlertMessage>
    </div>
  </div>
</template>
