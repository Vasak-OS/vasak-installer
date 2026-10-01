<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	ConfigSection,
	PageHeader,
	type PropertyItem,
	PropertyList,
	ThemeIcon,
} from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import { PASOS, useInstalacionStore } from '@/stores/instalacion';
import { formatearBytes } from '@/tools/formato';
import { HARDWARE_ICONS, STEP_ICONS } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

/** Por debajo de esto el escritorio va justo y conviene decirlo. */
const TIGHT_MEMORY = 4 * 1024 * 1024 * 1024;

const memory = computed(() =>
	store.sistema ? formatearBytes(store.sistema.memoria_bytes, locale.value) : '—'
);
// Con `?.` y no `!== null`: si el sondeo contestó vacío, `sistema` queda en
// `undefined` y la comparación con `null` lo dejaba pasar a leer la memoria.
const lowMemory = computed(() => (store.sistema?.memoria_bytes ?? Infinity) < TIGHT_MEMORY);

/**
 * Las filas del equipo, para `PropertyList`.
 *
 * El `id` es la clave del icono de cada fila: la etiqueta la dibuja la lista y
 * el icono va por la ranura, junto al valor, porque `PropertyList` no lleva
 * icono en la etiqueta.
 */
const hardware = computed<PropertyItem[]>(() => {
	const system = store.sistema;
	if (!system) return [];
	const rows: PropertyItem[] = [
		{
			id: 'procesador',
			label: t('bienvenida.procesador'),
			value: `${system.cpu || '—'} · ${interpolar(t('bienvenida.hilos'), system.nucleos)}`,
		},
		{ id: 'memoria', label: t('bienvenida.memoria'), value: memory.value },
		{
			id: 'firmware',
			label: t('bienvenida.firmware'),
			value:
				system.firmware === 'uefi' ? t('bienvenida.firmwareUefi') : t('bienvenida.firmwareBios'),
		},
	];
	if (system.virtualizacion) {
		rows.push({
			id: 'virtualizacion',
			label: t('bienvenida.virtualizacion'),
			value: system.virtualizacion,
			mono: true,
		});
	}
	return rows;
});

function iconOf(item: PropertyItem): string {
	return HARDWARE_ICONS[item.id as keyof typeof HARDWARE_ICONS];
}
</script>

<template>
  <div>
    <PageHeader
      class="mb-5"
      :icon="STEP_ICONS.bienvenida"
      icon-type="symbol"
      :title="t('bienvenida.titulo')"
      :description="interpolar(t('bienvenida.intro'), PASOS.length)" />

    <ConfigSection :title="t('bienvenida.equipo')" as="h2">
      <PropertyList v-if="store.sistema" :items="hardware">
        <template #value="{ item }">
          <span class="inline-flex min-w-0 items-start gap-2">
            <ThemeIcon :name="iconOf(item)" type="symbol" :size="16" class="mt-0.5" />
            <span class="min-w-0 break-words">{{ item.value }}</span>
          </span>
        </template>
      </PropertyList>
      <p v-else class="text-tx-muted text-sm">{{ t('comun.cargando') }}</p>
    </ConfigSection>

    <div class="mt-4 space-y-3">
      <AlertMessage v-if="store.sistema?.virtualizacion" tone="info" icon="auto">
        {{ t('bienvenida.avisoVirtual') }}
      </AlertMessage>

      <AlertMessage v-if="lowMemory" tone="warning" icon="auto" :title="t('bienvenida.avisoMemoriaTitulo')">
        {{ interpolar(t('bienvenida.avisoMemoria'), memory) }}
      </AlertMessage>

      <AlertMessage
        v-if="store.sistema?.firmware === 'bios'"
        tone="warning"
        icon="auto"
        :title="t('bienvenida.avisoBiosTitulo')"
      >
        {{ t('bienvenida.avisoBios') }}
      </AlertMessage>
    </div>
  </div>
</template>
