<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import IconoSistema from '@/components/ui/IconoSistema.vue';
import OpcionRadio from '@/components/ui/OpcionRadio.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SwitchToggle from '@/components/ui/SwitchToggle.vue';
import { type Complemento, useInstalacionStore } from '@/stores/instalacion';
import { ICONO_PASO } from '@/tools/iconos';

const { t } = useI18n();
const store = useInstalacionStore();

/** Los complementos de una categoría, en el orden del catálogo. */
function deCategoria(categoria: string): Complemento[] {
	return store.complementos.catalogo.filter((c) => c.categoria === categoria);
}

/** Si la categoría se elige con opciones excluyentes en vez de interruptores. */
function esExclusiva(categoria: string): boolean {
	const lista = deCategoria(categoria);
	return lista.length > 0 && lista.every((c) => c.exclusivo);
}

function estaElegido(id: string): boolean {
	return store.eleccion.complementos.includes(id);
}

/**
 * Si este complemento lo propuso el hardware detectado.
 *
 * Se marca en la interfaz porque cambia lo que significa la casilla: una
 * marcada «porque sí» y una marcada «porque encontramos tu placa» son cosas
 * distintas, y sin decirlo la segunda parece arbitraria.
 */
function loPropusoElHardware(complemento: Complemento): boolean {
	return (
		complemento.detectar !== null &&
		store.complementos.hardware.marcas.includes(complemento.detectar)
	);
}

/** Las categorías que tienen algo que mostrar. */
const categorias = computed(() =>
	store.complementos.categorias.filter((c) => deCategoria(c).length > 0)
);

const hayHardwareDetectado = computed(() => store.complementos.hardware.descripciones.length > 0);
</script>

<template>
  <div>
    <PageHeader
      :icono="ICONO_PASO.complementos"
      :titulo="t('complementos.titulo')"
      :descripcion="t('complementos.intro')"
    />

    <div class="space-y-4">
      <!--
        Sin catálogo el paso no se rompe: se dice por qué está vacío y se sigue.
        Todo esto es opcional por definición, y una instalación sin complementos
        es un sistema que arranca y en el que se puede sumar todo después.
      -->
      <AlertMessage v-if="store.complementos.error" tipo="aviso" :titulo="t('complementos.sinCatalogoTitulo')">
        <p>{{ t('complementos.sinCatalogo') }}</p>
        <p class="mt-1 font-mono">{{ store.complementos.error }}</p>
      </AlertMessage>

      <!--
        Lo que se detectó, dicho antes de las casillas. Es lo que hace que una
        opción marcada de antemano se entienda en vez de parecer arbitraria.
      -->
      <SectionCard v-if="hayHardwareDetectado" :titulo="t('complementos.detectado')">
        <ul class="space-y-1">
          <li
            v-for="descripcion in store.complementos.hardware.descripciones"
            :key="descripcion"
            class="flex items-center gap-2 text-sm"
          >
            <IconoSistema nombre="computer-chip" />
            {{ descripcion }}
          </li>
        </ul>
        <p class="mt-2 text-tx-muted text-xs">{{ t('complementos.detectadoAyuda') }}</p>
      </SectionCard>

      <SectionCard
        v-for="categoria in categorias"
        :key="categoria"
        :titulo="t(`complementos.categorias.${categoria}.titulo`)"
        :descripcion="t(`complementos.categorias.${categoria}.descripcion`)"
      >
        <!--
          Excluyentes con `radiogroup`, el resto con interruptores. La diferencia
          no es estética: un lector de pantalla anuncia «opción 2 de 4» en el
          primer caso y «interruptor» en el segundo, que es exactamente lo que
          cada uno es.
        -->
        <div
          v-if="esExclusiva(categoria)"
          role="radiogroup"
          :aria-label="t(`complementos.categorias.${categoria}.titulo`)"
          class="space-y-2"
        >
          <OpcionRadio
            v-for="complemento in deCategoria(categoria)"
            :key="complemento.id"
            :seleccionada="estaElegido(complemento.id)"
            :label="t(`complementos.items.${complemento.id}.nombre`)"
            :descripcion="t(`complementos.items.${complemento.id}.descripcion`)"
            :icono="complemento.icono"
            @elegir="store.alternarComplemento(complemento.id)"
          />
        </div>

        <div v-else class="space-y-1">
          <div v-for="complemento in deCategoria(categoria)" :key="complemento.id">
            <SwitchToggle
              :model-value="estaElegido(complemento.id)"
              :label="t(`complementos.items.${complemento.id}.nombre`)"
              :descripcion="t(`complementos.items.${complemento.id}.descripcion`)"
              :icono="complemento.icono"
              @update:model-value="store.alternarComplemento(complemento.id)"
            >
              <template v-if="loPropusoElHardware(complemento)" #pie>
                <span class="mt-1 flex items-center gap-1.5 text-status-success text-xs">
                  <IconoSistema nombre="object-select" clase="size-3" />
                  {{ t('complementos.propuestoPorHardware') }}
                </span>
              </template>
            </SwitchToggle>
          </div>
        </div>
      </SectionCard>

      <p class="text-tx-muted text-xs">{{ t('complementos.sePuedeDespues') }}</p>
    </div>
  </div>
</template>
