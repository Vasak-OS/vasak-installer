<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	ActionButton,
	AlertMessage,
	CodeBlock,
	type CodeLine,
	Disclosure,
	IconTile,
	type IconTileStatus,
	type IconTileTone,
	PageHeader,
	Panel,
	ProgressBar,
} from '@vasakgroup/vue-libvasak';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { INSTALL_STEP_ICONS, STEP_ICONS } from '@/tools/icons';
import { comoLapso, segundosDesde } from '@/tools/transcurrido';

const { t } = useI18n();
const store = useInstalacionStore();

const showLog = ref(false);
const confirmingCancel = ref(false);

/**
 * El reloj de la instalación.
 *
 * Está acá por un motivo concreto: **la barra se queda quieta**. `pacstrap` puede
 * tardar veinte minutos sin informar fracción, así que el avance general no se
 * mueve ni un píxel, y alguien mirando eso no puede distinguir «está trabajando»
 * de «se colgó».
 *
 * Un reloj que corre contesta eso sin inventar nada: no dice cuánto falta —no se
 * sabe— pero se mueve cada segundo y el movimiento viene de algo real. Una
 * animación que finja avance sería peor que la barra quieta, porque mentiría con
 * más convicción.
 */
const start = ref(0);
const now = ref(0);
let clock: ReturnType<typeof setInterval> | null = null;

const elapsed = computed(() =>
	start.value === 0 ? '' : comoLapso(segundosDesde(start.value, now.value))
);

onMounted(() => {
	start.value = Date.now();
	now.value = start.value;
	// Cada segundo, aunque el texto cambie de minuto en minuto después del primero:
	// el tic es lo que hace que la pantalla se sienta viva, y un intervalo de un
	// segundo no cuesta nada al lado de un pacstrap.
	clock = setInterval(() => {
		now.value = Date.now();
	}, 1000);
});

onUnmounted(() => {
	if (clock !== null) clearInterval(clock);
});

const emit = defineEmits<{ cancelar: [] }>();

/**
 * El avance general, de 0 a 100.
 *
 * En por ciento y no en fracción porque es lo que esperan sus dos lectores: la
 * barra de la librería y el número de al lado. Tenerlo en fracción obligaba a
 * multiplicar por cien en los dos lugares.
 *
 * Se cuentan los pasos terminados y se suma la fracción del que está en curso.
 * No pondera: `pacstrap` tarda diez veces más que escribir el fstab, así que la
 * barra avanza a saltos desiguales. Ponderar exigiría saber cuánto pesa cada
 * paso, y eso depende de la conexión y de la máquina — una estimación fija
 * mentiría con más precisión aparente.
 */
const progress = computed(() => {
	const total = store.pasosInstalacion.length;
	if (total === 0) return null;

	let done = 0;
	let partial = 0;
	for (const key of store.pasosInstalacion) {
		const p = store.progreso.get(key);
		if (p?.estado === 'hecho') done++;
		else if (p?.estado === 'en_curso' && p.fraccion !== null) partial = p.fraccion;
	}
	return ((done + partial) / total) * 100;
});

const currentStep = computed(() => {
	for (const key of store.pasosInstalacion) {
		const p = store.progreso.get(key);
		if (p?.estado === 'en_curso' || p?.estado === 'fallado') return p;
	}
	return null;
});

function stateOf(key: string) {
	return store.progreso.get(key)?.estado ?? 'pendiente';
}

/**
 * Si el paso en curso no sabe cuánto le falta.
 *
 * Cuando no lo sabe se muestra una barra indefinida además de la general. La
 * general sigue diciendo la verdad —cuántos pasos van— y la indefinida dice «este
 * paso está andando», que es la pregunta que alguien se hace cuando nada se
 * mueve.
 */
const stepWithoutFraction = computed(
	() => currentStep.value?.estado === 'en_curso' && currentStep.value.fraccion === null
);

/**
 * El estado de cada paso, como recuadro de `IconTile`.
 *
 * El icono del paso siempre visible y el estado como emblema encima: reemplazar
 * el icono por un tilde dejaba diez pasos terminados idénticos, sin pista de qué
 * había hecho cada uno —que es lo que se mira cuando algo falló y hay que
 * entender hasta dónde llegó—. El que está en curso lleva el velo de lo elegido.
 */
const STEP_TONE: Record<string, IconTileTone> = {
	hecho: 'success',
	en_curso: 'selected',
	fallado: 'error',
	pendiente: 'neutral',
};
const STEP_STATUS: Record<string, IconTileStatus | null> = {
	hecho: 'success',
	fallado: 'error',
	en_curso: null,
	pendiente: null,
};

/**
 * El registro, como líneas de `CodeBlock`.
 *
 * `CodeBlock variant="log"` con `follow` es lo que antes se hacía a mano acá:
 * se desplaza solo hasta el final, pero **sólo si ya estaba abajo** —quien sube a
 * leer una línea de error no es arrastrado hacia abajo con la siguiente—. El
 * tono de cada línea sale de su nivel.
 */
const LOG_TONE = { error: 'error', warn: 'warning', info: 'muted' } as const;
const logLines = computed<CodeLine[]>(() =>
	store.registro.map((entry) => ({ text: entry.linea, tone: LOG_TONE[entry.nivel] }))
);

// Al fallar, el registro se abre solo: es donde está la explicación, y pedir un
// clic más para verla en el momento en que algo salió mal es hacerlo esconder.
watch(
	() => store.fallo,
	(failed) => {
		if (failed) showLog.value = true;
	}
);
</script>

<template>
  <div>
    <PageHeader
      class="mb-5"
      :icon="STEP_ICONS.instalacion"
      icon-type="symbol"
      :title="t('instalacion.titulo')"
      :description="t('instalacion.intro')" />

    <div class="space-y-4">
      <AlertMessage v-if="store.fallo" tone="error" icon="auto" :title="t('instalacion.falloTitulo')">
        <p>{{ t('instalacion.falloTexto') }}</p>
        <p class="mt-2 font-medium">{{ t('instalacion.falloDetalleTitulo') }}</p>
        <p class="mt-1 font-mono break-words">{{ store.fallo }}</p>
        <p class="mt-2">{{ t('instalacion.falloRegistroCompleto') }}</p>
      </AlertMessage>

      <template v-else>
        <Panel>
          <ProgressBar :value="progress" :label="t('instalacion.titulo')" />
          <div class="mt-3 flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
            <p class="min-w-0 break-words font-medium text-sm">
              {{ currentStep ? t(`instalacion.pasos.${currentStep.paso}`) : t('comun.cargando') }}
            </p>
            <div class="flex shrink-0 items-baseline gap-2 text-tx-muted text-xs">
              <!-- El reloj con cifras de ancho fijo: sin `tabular-nums` el número
                   cambia de ancho al pasar de 9 a 10 y el porcentaje de al lado se
                   corre, que es movimiento que no informa nada. -->
              <span v-if="elapsed" class="tabular-nums">{{ elapsed }}</span>
              <span v-if="progress !== null">{{ Math.round(progress) }}%</span>
            </div>
          </div>

          <!-- La barra indefinida del paso, sólo cuando no informa fracción.
               Va **además** de la general y no en su lugar: la general dice
               cuántos pasos van, que es información real que no hay que tapar. -->
          <div v-if="stepWithoutFraction" class="mt-2">
            <ProgressBar :value="null" :label="t('instalacion.trabajando')" />
          </div>
          <p v-if="currentStep?.detalle" class="mt-1 truncate font-mono text-tx-muted text-xs" :title="currentStep.detalle">
            {{ currentStep.detalle }}
          </p>
        </Panel>

        <AlertMessage tone="warning" icon="auto">{{ t('instalacion.noApagues') }}</AlertMessage>
      </template>

      <Panel>
        <ol class="space-y-1.5">
          <li
            v-for="(key, position) in store.pasosInstalacion"
            :key="key"
            class="flex min-w-0 items-center gap-3 text-sm"
            :data-install-step="key"
            :data-state="stateOf(key)"
          >
            <IconTile
              :name="INSTALL_STEP_ICONS[key] ?? ''"
              type="symbol"
              size="sm"
              :tone="STEP_TONE[stateOf(key)]"
              :status="STEP_STATUS[stateOf(key)]"
              :class="stateOf(key) === 'en_curso' ? 'animate-pulse motion-reduce:animate-none' : ''"
            />
            <span class="min-w-0 break-words" :class="stateOf(key) === 'pendiente' ? 'text-tx-muted' : ''">
              {{ t(`instalacion.pasos.${key}`) }}
            </span>
            <!-- El número, chico y al final, igual que en la barra lateral. -->
            <span class="ml-auto shrink-0 text-tx-muted text-xs tabular-nums" aria-hidden="true">
              {{ position + 1 }}
            </span>
          </li>
        </ol>
      </Panel>

      <Disclosure
        v-model:open="showLog"
        :title="t('instalacion.registro')"
        variant="card"
      >
        <CodeBlock
          :lines="logLines"
          variant="log"
          follow
          max-height="16rem"
          :label="t('instalacion.registro')"
        />
      </Disclosure>

      <div v-if="!store.fallo && !store.terminada">
        <ActionButton
          v-if="!confirmingCancel"
          :label="t('comun.cancelar')"
          variant="secondary"
          @click="confirmingCancel = true"
        />

        <!--
          Cancelar pide confirmación y dice qué queda: el disco ya está
          modificado, así que «cancelar» no devuelve nada al estado anterior. Un
          botón que sólo dice «Cancelar» hace creer que sí.
        -->
        <AlertMessage v-else tone="error" icon="auto" :title="t('instalacion.cancelarTitulo')">
          <p>{{ t('instalacion.cancelarTexto') }}</p>
          <template #actions>
            <ActionButton :label="t('instalacion.cancelarConfirmar')" variant="danger" @click="emit('cancelar')" />
            <ActionButton :label="t('instalacion.cancelarVolver')" variant="secondary" @click="confirmingCancel = false" />
          </template>
        </AlertMessage>
      </div>
    </div>
  </div>
</template>
