<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, onMounted } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { formatearBytes, nombreDeIdioma, nombreDeZona } from '@/tools/formato';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

const zona = computed(() => {
	const { region, ciudad } = nombreDeZona(store.eleccion.zonaHoraria);
	return region ? `${ciudad} (${region})` : ciudad;
});

const idioma = computed(() => nombreDeIdioma(store.eleccion.idiomaSistema, locale.value));

const tamanoDisco = computed(() =>
	store.discoElegido ? formatearBytes(store.discoElegido.tamano_bytes, locale.value) : '—'
);

onMounted(() => {
	// Se recalcula al entrar y no sólo al cambiar el disco: si alguien volvió
	// atrás y cambió el sistema de archivos, el resumen tiene que mostrar el plan
	// nuevo, no el que se calculó la primera vez.
	store.calcularVistaPrevia();
});

async function autorizar() {
	await store.prepararAyudante();
}
</script>

<template>
  <div>
    <PageHeader :titulo="t('resumen.titulo')" :descripcion="t('resumen.intro')" />

    <div class="space-y-4">
      <!--
        El aviso va **arriba de todo** y con el nombre del disco adentro. Es el
        único momento en que la persona puede detenerse, y un aviso al pie de una
        página con scroll es un aviso que no se lee.
      -->
      <AlertMessage
        tipo="error"
        :titulo="interpolar(t('resumen.avisoTitulo'), store.eleccion.disco)"
      >
        <p>{{ t('resumen.aviso') }}</p>
        <template v-if="store.vistaPrevia?.se_pierde.length">
          <p class="mt-2 font-medium">{{ t('disco.seVaAPerder') }}</p>
          <ul class="mt-1 space-y-0.5 font-mono">
            <li v-for="linea in store.vistaPrevia.se_pierde" :key="linea">{{ linea }}</li>
          </ul>
        </template>
        <p v-else class="mt-2">{{ t('disco.seVaAPerderVacio') }}</p>
      </AlertMessage>

      <SectionCard :titulo="t('resumen.disco')">
        <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          <dt class="text-tx-muted">{{ t('resumen.campoDisco') }}</dt>
          <dd>
            <span class="font-mono">{{ store.eleccion.disco }}</span>
            <span class="ml-2 text-tx-muted">{{ store.discoElegido?.modelo }} · {{ tamanoDisco }}</span>
          </dd>

          <dt class="text-tx-muted">{{ t('resumen.campoEsquema') }}</dt>
          <dd>{{ t('resumen.campoEsquemaBorrarTodo') }}</dd>

          <dt class="text-tx-muted">{{ t('resumen.campoSistemaArchivos') }}</dt>
          <dd class="font-mono">{{ store.eleccion.sistemaArchivos }}</dd>

          <dt class="text-tx-muted">{{ t('resumen.campoCifrado') }}</dt>
          <dd>{{ store.eleccion.cifrar ? t('comun.si') : t('comun.no') }}</dd>

          <dt class="text-tx-muted">{{ t('resumen.campoZram') }}</dt>
          <dd>{{ store.eleccion.zram ? t('comun.si') : t('comun.no') }}</dd>

          <dt class="text-tx-muted">{{ t('resumen.campoArranque') }}</dt>
          <dd>GRUB · {{ store.vistaPrevia?.firmware.toUpperCase() ?? '—' }}</dd>
        </dl>
      </SectionCard>

      <SectionCard :titulo="t('resumen.region')">
        <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          <dt class="text-tx-muted">{{ t('resumen.campoZona') }}</dt>
          <dd>{{ zona }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoIdioma') }}</dt>
          <dd>{{ idioma }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoTeclado') }}</dt>
          <dd class="font-mono">{{ store.eleccion.teclado }}</dd>
        </dl>
      </SectionCard>

      <SectionCard :titulo="t('resumen.cuenta')">
        <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          <dt class="text-tx-muted">{{ t('resumen.campoNombreCompleto') }}</dt>
          <dd>{{ store.eleccion.nombreCompleto || '—' }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoUsuario') }}</dt>
          <dd class="font-mono">{{ store.eleccion.usuario }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoEquipo') }}</dt>
          <dd class="font-mono">{{ store.eleccion.hostname }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoAdministrador') }}</dt>
          <dd>{{ store.eleccion.administrador ? t('comun.si') : t('comun.no') }}</dd>
          <dt class="text-tx-muted">{{ t('resumen.campoRoot') }}</dt>
          <dd>
            {{ store.eleccion.rootHabilitado ? t('resumen.rootHabilitada') : t('resumen.rootBloqueada') }}
          </dd>
        </dl>
      </SectionCard>

      <AlertMessage
        v-if="!store.ayudanteListo"
        tipo="aviso"
        :titulo="t('resumen.autorizacionTitulo')"
      >
        <p>{{ t('resumen.autorizacion') }}</p>
        <p v-if="store.errorAyudante" class="mt-2 font-mono">{{ store.errorAyudante }}</p>
        <button
          type="button"
          class="mt-2 rounded-corner border border-ui-border-strong px-3 py-1.5 text-sm transition-colors hover:bg-ui-surface"
          @click="autorizar"
        >
          {{ t('resumen.autorizar') }}
        </button>
      </AlertMessage>
    </div>
  </div>
</template>
