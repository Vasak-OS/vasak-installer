<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, nextTick, ref, watch } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import ProgressBar from '@/components/ui/ProgressBar.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';

const { t } = useI18n();
const store = useInstalacionStore();

const mostrarRegistro = ref(false);
const confirmandoCancelar = ref(false);
const cajaRegistro = ref<HTMLElement | null>(null);

const emit = defineEmits<{ cancelar: [] }>();

/**
 * El avance general.
 *
 * Se cuentan los pasos terminados y se suma la fracción del que está en curso.
 * No pondera: `pacstrap` tarda diez veces más que escribir el fstab, así que la
 * barra avanza a saltos desiguales. Ponderar exigiría saber cuánto pesa cada
 * paso, y eso depende de la conexión y de la máquina — una estimación fija
 * mentiría con más precisión aparente.
 */
const avance = computed(() => {
	const total = store.pasosInstalacion.length;
	if (total === 0) return null;

	let hechos = 0;
	let parcial = 0;
	for (const clave of store.pasosInstalacion) {
		const p = store.progreso.get(clave);
		if (p?.estado === 'hecho') hechos++;
		else if (p?.estado === 'en_curso' && p.fraccion !== null) parcial = p.fraccion;
	}
	return (hechos + parcial) / total;
});

const pasoActual = computed(() => {
	for (const clave of store.pasosInstalacion) {
		const p = store.progreso.get(clave);
		if (p?.estado === 'en_curso' || p?.estado === 'fallado') return p;
	}
	return null;
});

function estadoDe(clave: string) {
	return store.progreso.get(clave)?.estado ?? 'pendiente';
}

// El registro se desplaza solo hasta el final, pero **sólo si ya estaba abajo**.
// Sin esa condición, alguien que sube a leer una línea de error es arrastrado
// hacia abajo en la siguiente línea que llega, y no puede leer nada.
watch(
	() => store.registro.length,
	async () => {
		const caja = cajaRegistro.value;
		if (!caja) return;
		const estabaAlFinal = caja.scrollHeight - caja.scrollTop - caja.clientHeight < 40;
		if (!estabaAlFinal) return;
		await nextTick();
		caja.scrollTop = caja.scrollHeight;
	}
);

// Al fallar, el registro se abre solo: es donde está la explicación, y pedir un
// clic más para verla en el momento en que algo salió mal es hacerlo esconder.
watch(
	() => store.fallo,
	(hayFallo) => {
		if (hayFallo) mostrarRegistro.value = true;
	}
);
</script>

<template>
  <div>
    <PageHeader :titulo="t('instalacion.titulo')" :descripcion="t('instalacion.intro')" />

    <div class="space-y-4">
      <AlertMessage v-if="store.fallo" tipo="error" :titulo="t('instalacion.falloTitulo')">
        <p>{{ t('instalacion.falloTexto') }}</p>
        <p class="mt-2 font-medium">{{ t('instalacion.falloDetalleTitulo') }}</p>
        <p class="mt-1 font-mono break-words">{{ store.fallo }}</p>
        <p class="mt-2">{{ t('instalacion.falloRegistroCompleto') }}</p>
      </AlertMessage>

      <template v-else>
        <SectionCard>
          <ProgressBar :valor="avance" :label="t('instalacion.titulo')" />
          <div class="mt-3 flex items-baseline justify-between gap-3">
            <p class="font-medium text-sm">
              {{ pasoActual ? t(`instalacion.pasos.${pasoActual.paso}`) : t('comun.cargando') }}
            </p>
            <p v-if="avance !== null" class="shrink-0 text-tx-muted text-xs">
              {{ Math.round(avance * 100) }}%
            </p>
          </div>
          <p v-if="pasoActual?.detalle" class="mt-1 truncate font-mono text-tx-muted text-xs">
            {{ pasoActual.detalle }}
          </p>
        </SectionCard>

        <AlertMessage tipo="aviso">{{ t('instalacion.noApagues') }}</AlertMessage>
      </template>

      <SectionCard>
        <ol class="space-y-1.5">
          <li
            v-for="(clave, indice) in store.pasosInstalacion"
            :key="clave"
            class="flex items-center gap-3 text-sm"
          >
            <span
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-xs"
              :class="
                estadoDe(clave) === 'hecho'
                  ? 'border-status-success bg-status-success/20'
                  : estadoDe(clave) === 'en_curso'
                    ? 'border-secondary bg-primary/20'
                    : estadoDe(clave) === 'fallado'
                      ? 'border-status-error bg-status-error/20'
                      : 'border-ui-border-strong'
              "
              aria-hidden="true"
            >
              <template v-if="estadoDe(clave) === 'hecho'">✓</template>
              <template v-else-if="estadoDe(clave) === 'fallado'">✕</template>
              <template v-else>{{ indice + 1 }}</template>
            </span>
            <span :class="estadoDe(clave) === 'pendiente' ? 'text-tx-muted' : ''">
              {{ t(`instalacion.pasos.${clave}`) }}
            </span>
          </li>
        </ol>
      </SectionCard>

      <SectionCard>
        <button
          type="button"
          class="text-sm underline"
          :aria-expanded="mostrarRegistro"
          @click="mostrarRegistro = !mostrarRegistro"
        >
          {{ mostrarRegistro ? t('comun.ocultarDetalles') : t('comun.mostrarDetalles') }}
          — {{ t('instalacion.registro') }}
        </button>

        <div
          v-if="mostrarRegistro"
          ref="cajaRegistro"
          class="mt-2 max-h-64 overflow-y-auto rounded-corner border border-ui-border bg-ui-bg/60 p-2 font-mono text-xs"
        >
          <p
            v-for="(linea, indice) in store.registro"
            :key="indice"
            class="whitespace-pre-wrap break-words"
            :class="
              linea.nivel === 'error'
                ? 'text-status-error'
                : linea.nivel === 'warn'
                  ? 'text-status-warning'
                  : 'text-tx-muted'
            "
          >
            {{ linea.linea }}
          </p>
        </div>
      </SectionCard>

      <div v-if="!store.fallo && !store.terminada">
        <button
          v-if="!confirmandoCancelar"
          type="button"
          class="rounded-corner border border-status-error px-3 py-1.5 text-sm transition-colors hover:bg-status-error/10"
          @click="confirmandoCancelar = true"
        >
          {{ t('comun.cancelar') }}
        </button>

        <!--
          Cancelar pide confirmación y dice qué queda: el disco ya está
          modificado, así que «cancelar» no devuelve nada al estado anterior. Un
          botón que sólo dice «Cancelar» hace creer que sí.
        -->
        <AlertMessage v-else tipo="error" :titulo="t('instalacion.cancelarTitulo')">
          <p>{{ t('instalacion.cancelarTexto') }}</p>
          <div class="mt-3 flex gap-2">
            <button
              type="button"
              class="rounded-corner border border-status-error px-3 py-1.5 text-sm transition-colors hover:bg-status-error/10"
              @click="emit('cancelar')"
            >
              {{ t('instalacion.cancelarConfirmar') }}
            </button>
            <button
              type="button"
              class="rounded-corner border border-ui-border-strong px-3 py-1.5 text-sm transition-colors hover:bg-ui-surface"
              @click="confirmandoCancelar = false"
            >
              {{ t('instalacion.cancelarVolver') }}
            </button>
          </div>
        </AlertMessage>
      </div>
    </div>
  </div>
</template>
