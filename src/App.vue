<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useConfigStore } from '@vasakgroup/plugin-config-manager';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { ActionButton, type ControlDeVentana, ThemeIcon } from '@vasakgroup/vue-libvasak';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import StepList from '@/components/sidebar/StepList.vue';
import StepsSidebar from '@/components/sidebar/StepsSidebar.vue';
import WindowAppLayout from '@/layouts/WindowAppLayout.vue';
import {
	type LineaRegistro,
	PASOS,
	type Paso,
	type ProgresoPaso,
	useInstalacionStore,
} from '@/stores/instalacion';
import { APP_ICON, BACK_ICON, STEP_LIST_ICON } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';
import AccountView from '@/views/AccountView.vue';
import AddonsView from '@/views/AddonsView.vue';
import DiskView from '@/views/DiskView.vue';
import FinishView from '@/views/FinishView.vue';
import InstallationView from '@/views/InstallationView.vue';
import KeyboardView from '@/views/KeyboardView.vue';
import NetworkView from '@/views/NetworkView.vue';
import RegionView from '@/views/RegionView.vue';
import SummaryView from '@/views/SummaryView.vue';
import WelcomeView from '@/views/WelcomeView.vue';

const { t } = useI18n();
const store = useInstalacionStore();

const views = {
	bienvenida: WelcomeView,
	red: NetworkView,
	region: RegionView,
	teclado: KeyboardView,
	disco: DiskView,
	cuenta: AccountView,
	complementos: AddonsView,
	resumen: SummaryView,
	instalacion: InstallationView,
	fin: FinishView,
} as const;

const content = ref<HTMLElement | null>(null);
const startError = ref<string | null>(null);
const unlisteners = ref<UnlistenFn[]>([]);

const index = computed(() => PASOS.indexOf(store.paso));
const isLast = computed(() => store.paso === 'fin');
const inInstallation = computed(() => store.paso === 'instalacion');

/**
 * El botón «Continuar» se muestra sólo mientras hay algo que responder.
 *
 * Durante la instalación y en la pantalla final no hay ningún «después» al que
 * ir: dejarlo puesto y deshabilitado sugiere que en algún momento se va a poder
 * apretar.
 */
const showsNavigation = computed(() => !inInstallation.value && !isLast.value);

/**
 * La ficha de pasos de la ventana angosta está abierta.
 *
 * Sólo cuenta por debajo de 30rem de ancho (ver la plantilla): ahí no entran la
 * barra y el contenido lado a lado, y va una columna por vez, como en una
 * aplicación de celular — el contenido del paso, o la lista de pasos con un
 * «volver». En una ventana ancha esto no cambia nada: la barra está siempre.
 */
const showSteps = ref(false);

function goTo(step: Paso) {
	showSteps.value = false;
	store.paso = step;
	// El foco y el desplazamiento vuelven arriba al cambiar de paso. Sin esto,
	// alguien que venía del final de una página larga aterriza en el medio de la
	// siguiente, y quien usa lector de pantalla se queda donde estaba, oyendo el
	// contenido anterior.
	content.value?.scrollTo({ top: 0 });
	content.value?.focus();
}

function back() {
	const previous = PASOS[index.value - 1];
	if (previous && !store.navegacionBloqueada) goTo(previous);
}

async function next() {
	if (store.paso === 'resumen') {
		await startInstallation();
		return;
	}
	const following = PASOS[index.value + 1];
	if (following) goTo(following);
}

async function startInstallation() {
	startError.value = null;
	try {
		await invoke('instalar', { plan: store.armarPlan() });
		// A partir de acá no hay vuelta atrás: el ayudante ya está escribiendo.
		store.navegacionBloqueada = true;
		goTo('instalacion');
		// Las contraseñas ya viajaron y se convirtieron en hash del otro lado; no
		// hay ninguna razón para que sigan en memoria de la ventana durante la
		// media hora que dura la instalación.
		store.olvidarSecretos();
	} catch (error) {
		startError.value = String(error);
	}
}

async function cancel() {
	try {
		await invoke('cancelar_instalacion');
	} catch (error) {
		store.anotarRegistro({ nivel: 'error', linea: String(error) });
	}
}

/**
 * Cuáles de los tres botones lleva la ventana.
 *
 * Los tres, salvo **cerrar mientras el ayudante está escribiendo el disco**:
 * ahí cerrar deja el equipo a medio instalar, sin sistema nuevo y sin lo que
 * había antes. Minimizar y maximizar se quedan siempre, que es lo que evita lo
 * contrario —perder la ventana de vista y no poder traerla de vuelta—.
 *
 * Cuando la instalación termina, falla o se cancela, cerrar vuelve: ahí ya no
 * hay nada escribiendo. Y mientras corre, la salida es el «cancelar» de la
 * pantalla de instalación, que pregunta y detiene al ayudante antes.
 */
const installing = computed(
	() => store.navegacionBloqueada && !store.terminada && store.fallo === null
);

const windowControls = computed<ControlDeVentana[]>(() =>
	installing.value ? ['minimize', 'maximize'] : ['minimize', 'maximize', 'close']
);

onMounted(async () => {
	// El tema y los iconos del escritorio, como cualquier aplicación de VasakOS.
	// Importa más acá que en otras: esta es la primera pantalla que alguien ve
	// del sistema, y si no se parece al escritorio que la rodea, parece ajena.
	try {
		const configStore = useConfigStore();
		await configStore.loadConfig();
		unlisteners.value.push(
			await listen('config-changed', () => {
				document.startViewTransition(() => configStore.loadConfig());
			})
		);
	} catch (error) {
		console.error('no se pudo cargar la configuración', error);
	}

	unlisteners.value.push(
		await listen<ProgresoPaso>('instalacion://progreso', (evento) => {
			store.anotarProgreso(evento.payload);
		}),
		await listen<LineaRegistro>('instalacion://registro', (evento) => {
			store.anotarRegistro(evento.payload);
		}),
		await listen<{ ok: boolean; error: string | null }>('instalacion://fin', (evento) => {
			if (evento.payload.ok) {
				store.terminada = true;
				goTo('fin');
			} else {
				store.fallo = evento.payload.error ?? t('errores.desconocido');
			}
		}),
		await listen('instalacion://ayudante-caido', () => {
			store.ayudanteListo = false;
			// Sólo es un fallo si la instalación estaba en marcha. El ayudante
			// también se cierra normalmente al terminar bien, y ahí marcar fallo
			// convertiría una instalación exitosa en una pantalla de error.
			if (store.paso === 'instalacion' && !store.terminada && !store.fallo) {
				store.fallo = t('instalacion.ayudanteCaido');
			}
		})
	);

	try {
		await store.cargarSondeo();
	} catch (error) {
		startError.value = String(error);
	}
});

onUnmounted(() => {
	for (const fn of unlisteners.value) fn();
	// Por si la ventana se cierra antes de instalar: las contraseñas no tienen
	// por qué sobrevivir al componente.
	store.olvidarSecretos();
});
</script>

<template>
  <WindowAppLayout :controls="windowControls">
    <!--
      La barra de título propia: icono a la izquierda, nombre al medio. Sin esto
      quedaba con los tres botones de la ventana flotando sobre nada — y como la
      ventana no tiene decoración del compositor, el nombre de la aplicación no
      aparecía en ningún otro lado.
    -->
    <template #identidad>
      <ThemeIcon :name="APP_ICON" type="icon" :size="20" />
    </template>
    <template #titulo>
      <span class="truncate font-medium text-sm">{{ t('app.nombre') }}</span>
    </template>

    <!--
      `p-1` y `gap-1`: la barra lateral es una tarjeta con borde y esquina
      redondeada, y pegada al borde de la ventana se le come el redondeo. Es la
      misma distancia que separa todo en el resto de las ventanas.

      `@container/window` es la fila de la ventana: lo que cambia con el ancho
      se decide por **su** ancho y no por el de la pantalla (WebKitGTK no avisa
      de `resize`). Por debajo de 30rem no entran dos columnas y va una por vez:
      la barra se esconde y su lugar lo toma una franja con el paso actual, que
      abre la ficha de pasos. De 30rem para arriba, como siempre.
    -->
    <div class="@container/window flex min-h-0 w-full min-w-0 flex-1">
    <div class="flex min-h-0 w-full min-w-0 flex-1 gap-1 p-1">
      <div class="flex min-h-0 @max-[30rem]/window:hidden" data-steps-rail>
        <StepsSidebar
          :current="store.paso"
          :navigable="!store.navegacionBloqueada"
          @go="goTo"
        />
      </div>

      <!-- La ficha de pasos: la misma lista que la barra, a todo el ancho, con
           un «volver» arriba. Sólo existe angosta y cuando se la pidió. -->
      <div
        v-if="showSteps"
        class="hidden min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-corner-l border border-ui-line bg-ui-surface/70 @max-[30rem]/window:flex"
        data-steps-sheet
      >
        <div class="flex shrink-0 items-center border-ui-line-weak border-b p-2">
          <ActionButton
            :label="t('barraLateral.backToStep')"
            :icon="BACK_ICON"
            variant="ghost"
            @click="showSteps = false"
          />
        </div>
        <div class="min-h-0 flex-1 overflow-y-auto p-2">
          <StepList :current="store.paso" :navigable="!store.navegacionBloqueada" @go="goTo" />
        </div>
      </div>

      <!-- El contenido también es un panel apoyado sobre la ventana, así que
           va en superficie: `--ui-background` es el token de **la ventana**, y
           con el fondo de ventana puesto acá el escritorio se ve a través del
           paso que se está completando. Es la tarjeta de Once UI, la misma que
           la barra de al lado: `rounded-corner-l` y el canto `ui-line`. -->
      <div
        class="flex min-w-0 flex-1 flex-col overflow-hidden rounded-corner-l border border-ui-line bg-ui-surface/70"
        :class="showSteps ? '@max-[30rem]/window:hidden' : ''"
        data-step-content
      >
        <!-- La franja del paso actual, sólo angosta: es lo que queda de la
             barra cuando no entra, y la puerta a la ficha de pasos. -->
        <div class="hidden shrink-0 border-ui-line border-b p-2 @max-[30rem]/window:flex" data-steps-bar>
          <ActionButton
            :label="interpolar(t('barraLateral.showSteps'), index + 1, PASOS.length, t(`pasos.${store.paso}.titulo`))"
            :icon="STEP_LIST_ICON"
            variant="ghost"
            full-width
            @click="showSteps = true"
          />
        </div>
        <!--
          `tabindex="-1"` para poder mover el foco acá al cambiar de paso sin
          meter el contenedor en el orden de tabulación. Es lo que hace que un
          lector de pantalla anuncie la página nueva en vez de seguir donde
          estaba.
        -->
        <main ref="content" tabindex="-1" class="min-h-0 flex-1 overflow-y-auto p-6 outline-none @max-[30rem]/window:p-3">
          <component :is="views[store.paso]" @cancelar="cancel" />

          <p v-if="startError" role="alert" class="mt-4 text-status-error text-sm">
            {{ startError }}
          </p>
        </main>

        <footer
          v-if="showsNavigation"
          class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-ui-line border-t p-4 @max-[30rem]/window:p-3"
        >
          <ActionButton
            :label="t('comun.atras')"
            variant="secondary"
            size="lg"
            :disabled="index === 0 || store.navegacionBloqueada"
            @click="back"
          />

          <!-- En el resumen el botón es el punto sin retorno: `danger`, el
               rojo del esquema con su texto calculado para el contraste. Antes
               iba `text-ui-bg` sobre el rojo, el fondo de la ventana usado
               como color de letra. -->
          <ActionButton
            :label="store.paso === 'resumen' ? t('resumen.confirmar') : t('comun.siguiente')"
            :variant="store.paso === 'resumen' ? 'danger' : 'primary'"
            size="lg"
            :disabled="!store.puedeAvanzar(store.paso)"
            @click="next"
          />
        </footer>
      </div>
    </div>
    </div>
  </WindowAppLayout>
</template>
