<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import IconoSistema from '@/components/ui/IconoSistema.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import ProgressBar from '@/components/ui/ProgressBar.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { ICONO_FALLADO, ICONO_HECHO, ICONO_PASO, ICONO_PASO_INSTALACION } from '@/tools/iconos';
import { comoLapso, segundosDesde } from '@/tools/transcurrido';

const { t } = useI18n();
const store = useInstalacionStore();

const mostrarRegistro = ref(false);
const confirmandoCancelar = ref(false);
const cajaRegistro = ref<HTMLElement | null>(null);

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
const inicio = ref(0);
const ahora = ref(0);
let reloj: ReturnType<typeof setInterval> | null = null;

const transcurrido = computed(() =>
	inicio.value === 0 ? '' : comoLapso(segundosDesde(inicio.value, ahora.value))
);

onMounted(() => {
	inicio.value = Date.now();
	ahora.value = inicio.value;
	// Cada segundo, aunque el texto cambie de minuto en minuto después del primero:
	// el tic es lo que hace que la pantalla se sienta viva, y un intervalo de un
	// segundo no cuesta nada al lado de un pacstrap.
	reloj = setInterval(() => {
		ahora.value = Date.now();
	}, 1000);
});

onUnmounted(() => {
	if (reloj !== null) clearInterval(reloj);
});

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

/**
 * Si el paso en curso no sabe cuánto le falta.
 *
 * Cuando no lo sabe se muestra una barra indefinida además de la general. La
 * general sigue diciendo la verdad —cuántos pasos van— y la indefinida dice «este
 * paso está andando», que es la pregunta que alguien se hace cuando nada se
 * mueve.
 */
const pasoSinFraccion = computed(
	() => pasoActual.value?.estado === 'en_curso' && pasoActual.value.fraccion === null
);

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
    <PageHeader :icono="ICONO_PASO.instalacion" :titulo="t('instalacion.titulo')" :descripcion="t('instalacion.intro')" />

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
            <div class="flex shrink-0 items-baseline gap-2 text-tx-muted text-xs">
              <!-- El reloj con cifras de ancho fijo: sin `tabular-nums` el número
                   cambia de ancho al pasar de 9 a 10 y el porcentaje de al lado se
                   corre, que es movimiento que no informa nada. -->
              <span v-if="transcurrido" class="tabular-nums">{{ transcurrido }}</span>
              <span v-if="avance !== null">{{ Math.round(avance * 100) }}%</span>
            </div>
          </div>

          <!-- La barra indefinida del paso, sólo cuando no informa fracción.
               Va **además** de la general y no en su lugar: la general dice
               cuántos pasos van, que es información real que no hay que tapar. -->
          <div v-if="pasoSinFraccion" class="mt-2">
            <ProgressBar :valor="null" :label="t('instalacion.trabajando')" />
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
            <!--
              El icono del paso siempre visible, y el estado como emblema encima.
              Reemplazar el icono por un tilde dejaba diez pasos terminados
              idénticos entre sí, sin ninguna pista de qué había hecho cada uno
              —que es justo lo que se mira cuando algo falló y hay que entender
              hasta dónde llegó.
            -->
            <span
              class="relative flex size-8 shrink-0 items-center justify-center rounded-corner border transition-colors"
              :class="
                estadoDe(clave) === 'hecho'
                  ? 'border-status-success/60 bg-status-success/15'
                  : estadoDe(clave) === 'en_curso'
                    ? 'border-secondary bg-primary/20 animate-pulse motion-reduce:animate-none'
                    : estadoDe(clave) === 'fallado'
                      ? 'border-status-error bg-status-error/20'
                      : 'border-ui-border-strong bg-ui-surface/30'
              "
              aria-hidden="true"
            >
              <IconoSistema :nombre="ICONO_PASO_INSTALACION[clave] ?? ''" clase="size-4" />
              <span
                v-if="estadoDe(clave) === 'hecho' || estadoDe(clave) === 'fallado'"
                class="-right-1 -bottom-1 absolute flex size-4 items-center justify-center rounded-full"
                :class="estadoDe(clave) === 'hecho' ? 'bg-status-success' : 'bg-status-error'"
              >
                <IconoSistema
                  :nombre="estadoDe(clave) === 'hecho' ? ICONO_HECHO : ICONO_FALLADO"
                  clase="size-3"
                />
              </span>
            </span>
            <span :class="estadoDe(clave) === 'pendiente' ? 'text-tx-muted' : ''">
              {{ t(`instalacion.pasos.${clave}`) }}
            </span>
            <!-- El número, chico y al final, igual que en la barra lateral. -->
            <span class="ml-auto shrink-0 text-tx-muted text-xs tabular-nums" aria-hidden="true">
              {{ indice + 1 }}
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
