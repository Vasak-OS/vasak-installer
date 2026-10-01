<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	ActionButton,
	AlertMessage,
	ConfigSection,
	PageHeader,
	type PropertyItem,
	PropertyList,
	ThemeIcon,
} from '@vasakgroup/vue-libvasak';
import { computed, onMounted } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { formatearBytes, nombreDeIdioma, nombreDeZona } from '@/tools/formato';
import { STEP_ICONS } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

const zone = computed(() => {
	const { region, ciudad } = nombreDeZona(store.eleccion.zonaHoraria);
	return region ? `${ciudad} (${region})` : ciudad;
});

const language = computed(() => nombreDeIdioma(store.eleccion.idiomaSistema, locale.value));

/**
 * Si el plan destruye algo.
 *
 * Sale de `se_pierde`, que el backend calcula **desde el plan**, y no del
 * esquema elegido. La diferencia importa: si algún día un esquema que se
 * presenta como no destructivo empezara a destruir algo, el cartel se pondría
 * rojo solo en vez de seguir diciendo que no pasa nada.
 */
const losesSomething = computed(() => (store.vistaPrevia?.se_pierde.length ?? 0) > 0);

const SCHEME_NAME = {
	borrar_todo: 'resumen.campoEsquemaBorrarTodo',
	junto_a_otro_sistema: 'resumen.campoEsquemaJunto',
	sobre_una_particion: 'resumen.campoEsquemaSobre',
	manual: 'resumen.campoEsquemaManual',
} as const;

const schemeName = computed(() => SCHEME_NAME[store.eleccion.esquema]);

const diskSize = computed(() =>
	store.discoElegido ? formatearBytes(store.discoElegido.tamano_bytes, locale.value) : '—'
);

const yesNo = (value: boolean) => (value ? t('comun.si') : t('comun.no'));

/**
 * Las filas del disco. La primera es **el disco elegido**: su valor es
 * `eleccion.disco`, el mismo texto que se marcó en el paso del disco y el que
 * viaja en el plan, y la ranura le suma el modelo y el tamaño al lado.
 */
const diskRows = computed<PropertyItem[]>(() => [
	// Sin `mono` en la fila: la ruta va en mono desde la ranura, y el modelo
	// que va al lado, no.
	{ id: 'disk', label: t('resumen.campoDisco'), value: store.eleccion.disco },
	{ id: 'scheme', label: t('resumen.campoEsquema'), value: t(schemeName.value) },
	{
		id: 'filesystem',
		label: t('resumen.campoSistemaArchivos'),
		value: store.eleccion.sistemaArchivos,
		mono: true,
	},
	{ id: 'encryption', label: t('resumen.campoCifrado'), value: yesNo(store.eleccion.cifrar) },
	{ id: 'zram', label: t('resumen.campoZram'), value: yesNo(store.eleccion.zram) },
	{
		id: 'bootloader',
		label: t('resumen.campoArranque'),
		value: `GRUB · ${store.vistaPrevia?.firmware.toUpperCase() ?? '—'}`,
	},
]);

const regionRows = computed<PropertyItem[]>(() => [
	{ label: t('resumen.campoZona'), value: zone.value },
	{ label: t('resumen.campoIdioma'), value: language.value },
	{ label: t('resumen.campoTeclado'), value: store.eleccion.teclado, mono: true },
]);

const accountRows = computed<PropertyItem[]>(() => [
	{ label: t('resumen.campoNombreCompleto'), value: store.eleccion.nombreCompleto || '—' },
	{ label: t('resumen.campoUsuario'), value: store.eleccion.usuario, mono: true },
	{ label: t('resumen.campoEquipo'), value: store.eleccion.hostname, mono: true },
	{ label: t('resumen.campoAdministrador'), value: yesNo(store.eleccion.administrador) },
	{
		label: t('resumen.campoRoot'),
		value: store.eleccion.rootHabilitado ? t('resumen.rootHabilitada') : t('resumen.rootBloqueada'),
	},
]);

onMounted(() => {
	// Se recalcula al entrar y no sólo al cambiar el disco: si alguien volvió
	// atrás y cambió el sistema de archivos, el resumen tiene que mostrar el plan
	// nuevo, no el que se calculó la primera vez.
	store.calcularVistaPrevia();
});

async function authorize() {
	await store.prepararAyudante();
}
</script>

<template>
  <div>
    <PageHeader
      class="mb-5"
      :icon="STEP_ICONS.resumen"
      icon-type="symbol"
      :title="t('resumen.titulo')"
      :description="t('resumen.intro')" />

    <div class="space-y-4">
      <!--
        El aviso va **arriba de todo** y con el nombre del disco adentro. Es el
        único momento en que la persona puede detenerse, y un aviso al pie de una
        página con scroll es un aviso que no se lee.

        Rojo sólo cuando se pierde algo. Un cartel rojo de «esto no se puede
        deshacer» arriba de una instalación que no borra nada no es prudencia:
        es la manera de que el rojo deje de significar algo. La condición mira
        `se_pierde`, que sale del plan, y no el esquema elegido: si algún día un
        esquema «no destructivo» empezara a destruir algo, el cartel se pondría
        rojo solo.
      -->
      <AlertMessage
        v-if="losesSomething"
        tone="error"
        icon="auto"
        :title="interpolar(t('resumen.avisoTitulo'), store.eleccion.disco)"
      >
        <p>{{ t('resumen.aviso') }}</p>
        <p class="mt-2 font-medium">{{ t('disco.seVaAPerder') }}</p>
        <ul class="mt-1 space-y-0.5 break-words font-mono">
          <li v-for="line in store.vistaPrevia?.se_pierde ?? []" :key="line">{{ line }}</li>
        </ul>
      </AlertMessage>

      <!-- Borrando un disco que no tiene nada: no hay nada que enumerar, pero
           la tabla se rehace igual y eso sigue siendo el punto sin retorno. -->
      <AlertMessage
        v-else-if="store.eleccion.esquema === 'borrar_todo'"
        tone="error"
        icon="auto"
        :title="interpolar(t('resumen.avisoTitulo'), store.eleccion.disco)"
      >
        <p>{{ t('resumen.aviso') }}</p>
        <p class="mt-2">{{ t('disco.seVaAPerderVacio') }}</p>
      </AlertMessage>

      <AlertMessage
        v-else
        tone="info"
        icon="auto"
        :title="interpolar(t('resumen.avisoJuntoTitulo'), store.eleccion.disco)"
      >
        {{ t('resumen.avisoJunto') }}
      </AlertMessage>

      <ConfigSection :title="t('resumen.disco')" as="h2">
        <PropertyList :items="diskRows">
          <template #value="{ item }">
            <template v-if="item.id === 'disk'">
              <span class="break-all font-mono" data-chosen-disk>{{ item.value }}</span>
              <span class="ml-2 break-words text-tx-muted">{{ store.discoElegido?.modelo }} · {{ diskSize }}</span>
            </template>
            <template v-else>{{ item.value }}</template>
          </template>
        </PropertyList>
      </ConfigSection>

      <ConfigSection :title="t('resumen.region')" as="h2">
        <PropertyList :items="regionRows" />
      </ConfigSection>

      <ConfigSection :title="t('pasos.complementos.titulo')" as="h2">
        <ul v-if="store.complementosElegidos.length" class="space-y-1.5">
          <li
            v-for="addon in store.complementosElegidos"
            :key="addon.id"
            class="flex min-w-0 items-center gap-2 text-sm"
          >
            <ThemeIcon :name="addon.icono" type="icon" :size="20" />
            <span class="min-w-0 break-words">{{ t(`complementos.items.${addon.id}.nombre`) }}</span>
          </li>
        </ul>
        <p v-else class="text-tx-muted text-sm">{{ t('resumen.ningunComplemento') }}</p>
      </ConfigSection>

      <ConfigSection :title="t('resumen.cuenta')" as="h2">
        <PropertyList :items="accountRows" />
      </ConfigSection>

      <AlertMessage
        v-if="!store.ayudanteListo"
        tone="warning"
        icon="auto"
        :title="t('resumen.autorizacionTitulo')"
      >
        <p>{{ t('resumen.autorizacion') }}</p>
        <p v-if="store.errorAyudante" class="mt-2 break-words font-mono">{{ store.errorAyudante }}</p>
        <template #actions>
          <ActionButton :label="t('resumen.autorizar')" variant="secondary" @click="authorize" />
        </template>
      </AlertMessage>
    </div>
  </div>
</template>
