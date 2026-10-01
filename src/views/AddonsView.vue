<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	ConfigSection,
	OptionGroup,
	type OptionGroupOption,
	PageHeader,
	SwitchRow,
	ThemeIcon,
} from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import { type Complemento, useInstalacionStore } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';

const { t } = useI18n();
const store = useInstalacionStore();

/** Los complementos de una categoría, en el orden del catálogo. */
function ofCategory(category: string): Complemento[] {
	return store.complementos.catalogo.filter((c) => c.categoria === category);
}

/** Si la categoría se elige con opciones excluyentes en vez de interruptores. */
function isExclusive(category: string): boolean {
	const list = ofCategory(category);
	return list.length > 0 && list.every((c) => c.exclusivo);
}

function isChosen(id: string): boolean {
	return store.eleccion.complementos.includes(id);
}

/**
 * Las opciones de una categoría excluyente (los navegadores), con su logo a
 * color: en glifo monocromo Firefox, Chromium y Brave son tres contornos que
 * nadie distingue.
 */
function optionsOf(category: string): OptionGroupOption<string>[] {
	return ofCategory(category).map((addon) => ({
		value: addon.id,
		label: t(`complementos.items.${addon.id}.nombre`),
		description: t(`complementos.items.${addon.id}.descripcion`),
		icon: addon.icono,
		iconType: 'icon',
	}));
}

/**
 * El elegido de una categoría excluyente, o `null` si ninguno.
 *
 * El grupo muestra lo que dice el almacén y avisa del clic; quien decide qué
 * queda elegido sigue siendo `alternarComplemento`, que saca al otro de la
 * misma categoría. Así no hay dos lugares que sepan la regla.
 */
function chosenOf(category: string): string | null {
	return ofCategory(category).find((addon) => isChosen(addon.id))?.id ?? null;
}

/**
 * Si este complemento lo propuso el hardware detectado.
 *
 * Se marca en la interfaz porque cambia lo que significa la casilla: una
 * marcada «porque sí» y una marcada «porque encontramos tu placa» son cosas
 * distintas, y sin decirlo la segunda parece arbitraria.
 */
function proposedByHardware(addon: Complemento): boolean {
	return addon.detectar !== null && store.complementos.hardware.marcas.includes(addon.detectar);
}

/** Las categorías que tienen algo que mostrar. */
const categories = computed(() =>
	store.complementos.categorias.filter((c) => ofCategory(c).length > 0)
);

const hardwareDetected = computed(() => store.complementos.hardware.descripciones.length > 0);
</script>

<template>
  <div>
    <PageHeader
      class="mb-5"
      :icon="STEP_ICONS.complementos"
      icon-type="symbol"
      :title="t('complementos.titulo')"
      :description="t('complementos.intro')"
    />

    <div class="space-y-4">
      <!--
        Sin catálogo el paso no se rompe: se dice por qué está vacío y se sigue.
        Todo esto es opcional por definición, y una instalación sin complementos
        es un sistema que arranca y en el que se puede sumar todo después.
      -->
      <AlertMessage v-if="store.complementos.error" tone="warning" icon="auto" :title="t('complementos.sinCatalogoTitulo')">
        <p>{{ t('complementos.sinCatalogo') }}</p>
        <p class="mt-1 font-mono">{{ store.complementos.error }}</p>
      </AlertMessage>

      <!--
        Lo que se detectó, dicho antes de las casillas. Es lo que hace que una
        opción marcada de antemano se entienda en vez de parecer arbitraria.
      -->
      <ConfigSection v-if="hardwareDetected" :title="t('complementos.detectado')" as="h2">
        <ul class="space-y-1">
          <li
            v-for="descripcion in store.complementos.hardware.descripciones"
            :key="descripcion"
            class="flex items-center gap-2 text-sm"
          >
            <ThemeIcon name="computer-chip" type="symbol" :size="16" />
            {{ descripcion }}
          </li>
        </ul>
        <p class="mt-2 text-tx-muted text-xs">{{ t('complementos.detectadoAyuda') }}</p>
      </ConfigSection>

      <ConfigSection
        v-for="category in categories"
        :key="category"
        :title="t(`complementos.categorias.${category}.titulo`)"
        :description="t(`complementos.categorias.${category}.descripcion`)"
        as="h2"
      >
        <!--
          Excluyentes con `radiogroup`, el resto con interruptores. La diferencia
          no es estética: un lector de pantalla anuncia «opción 2 de 4» en el
          primer caso y «interruptor» en el segundo, que es exactamente lo que
          cada uno es.
        -->
        <OptionGroup
          v-if="isExclusive(category)"
          :model-value="chosenOf(category)"
          :options="optionsOf(category)"
          :label="t(`complementos.categorias.${category}.titulo`)"
          variant="card"
          @change="store.alternarComplemento($event)"
        />

        <div v-else class="space-y-1">
          <div v-for="addon in ofCategory(category)" :key="addon.id">
            <SwitchRow
              :model-value="isChosen(addon.id)"
              :label="t(`complementos.items.${addon.id}.nombre`)"
              :description="t(`complementos.items.${addon.id}.descripcion`)"
              :icon="addon.icono"
              @update:model-value="store.alternarComplemento(addon.id)"
            >
              <template v-if="proposedByHardware(addon)" #pie>
                <span class="mt-1 flex items-center gap-1.5 text-status-success text-xs">
                  <ThemeIcon name="object-select" type="symbol" :size="12" />
                  {{ t('complementos.propuestoPorHardware') }}
                </span>
              </template>
            </SwitchRow>
          </div>
        </div>
      </ConfigSection>

      <p class="text-tx-muted text-xs">{{ t('complementos.sePuedeDespues') }}</p>
    </div>
  </div>
</template>
