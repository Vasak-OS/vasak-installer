<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useConfigStore } from '@vasakgroup/plugin-config-manager';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import type { Store } from 'pinia';
import { computed, onMounted, onUnmounted, ref } from 'vue';
import PasosSidebar from '@/components/sidebar/PasosSidebar.vue';
import IconoSimbolo from '@/components/ui/IconoSimbolo.vue';
import WindowAppLayout from '@/layouts/WindowAppLayout.vue';
import {
	type LineaRegistro,
	PASOS,
	type Paso,
	type ProgresoPaso,
	useInstalacionStore,
} from '@/stores/instalacion';
import BienvenidaView from '@/views/BienvenidaView.vue';
import ComplementosView from '@/views/ComplementosView.vue';
import CuentaView from '@/views/CuentaView.vue';
import DiscoView from '@/views/DiscoView.vue';
import FinView from '@/views/FinView.vue';
import InstalacionView from '@/views/InstalacionView.vue';
import RedView from '@/views/RedView.vue';
import RegionView from '@/views/RegionView.vue';
import ResumenView from '@/views/ResumenView.vue';
import TecladoView from '@/views/TecladoView.vue';

const { t } = useI18n();
const store = useInstalacionStore();

const vistas = {
	bienvenida: BienvenidaView,
	red: RedView,
	region: RegionView,
	teclado: TecladoView,
	disco: DiscoView,
	cuenta: CuentaView,
	complementos: ComplementosView,
	resumen: ResumenView,
	instalacion: InstalacionView,
	fin: FinView,
} as const;

const contenido = ref<HTMLElement | null>(null);
const errorAlArrancar = ref<string | null>(null);
const desuscribir = ref<UnlistenFn[]>([]);

const indice = computed(() => PASOS.indexOf(store.paso));
const esUltimo = computed(() => store.paso === 'fin');
const enInstalacion = computed(() => store.paso === 'instalacion');

/**
 * El botón «Continuar» se muestra sólo mientras hay algo que responder.
 *
 * Durante la instalación y en la pantalla final no hay ningún «después» al que
 * ir: dejarlo puesto y deshabilitado sugiere que en algún momento se va a poder
 * apretar.
 */
const muestraNavegacion = computed(() => !enInstalacion.value && !esUltimo.value);

function irA(paso: Paso) {
	store.paso = paso;
	// El foco y el desplazamiento vuelven arriba al cambiar de paso. Sin esto,
	// alguien que venía del final de una página larga aterriza en el medio de la
	// siguiente, y quien usa lector de pantalla se queda donde estaba, oyendo el
	// contenido anterior.
	contenido.value?.scrollTo({ top: 0 });
	contenido.value?.focus();
}

function atras() {
	const anterior = PASOS[indice.value - 1];
	if (anterior && !store.navegacionBloqueada) irA(anterior);
}

async function siguiente() {
	if (store.paso === 'resumen') {
		await arrancarInstalacion();
		return;
	}
	const proximo = PASOS[indice.value + 1];
	if (proximo) irA(proximo);
}

async function arrancarInstalacion() {
	errorAlArrancar.value = null;
	try {
		await invoke('instalar', { plan: store.armarPlan() });
		// A partir de acá no hay vuelta atrás: el ayudante ya está escribiendo.
		store.navegacionBloqueada = true;
		irA('instalacion');
		// Las contraseñas ya viajaron y se convirtieron en hash del otro lado; no
		// hay ninguna razón para que sigan en memoria de la ventana durante la
		// media hora que dura la instalación.
		store.olvidarSecretos();
	} catch (error) {
		errorAlArrancar.value = String(error);
	}
}

async function cancelar() {
	try {
		await invoke('cancelar_instalacion');
	} catch (error) {
		store.anotarRegistro({ nivel: 'error', linea: String(error) });
	}
}

onMounted(async () => {
	// El tema y los iconos del escritorio, como cualquier aplicación de VasakOS.
	// Importa más acá que en otras: esta es la primera pantalla que alguien ve
	// del sistema, y si no se parece al escritorio que la rodea, parece ajena.
	try {
		const configStore = useConfigStore() as Store<
			'config',
			{ config: unknown; loadConfig: () => Promise<void> }
		>;
		await configStore.loadConfig();
		desuscribir.value.push(
			await listen('config-changed', () => {
				document.startViewTransition(() => configStore.loadConfig());
			})
		);
	} catch (error) {
		console.error('no se pudo cargar la configuración', error);
	}

	desuscribir.value.push(
		await listen<ProgresoPaso>('instalacion://progreso', (evento) => {
			store.anotarProgreso(evento.payload);
		}),
		await listen<LineaRegistro>('instalacion://registro', (evento) => {
			store.anotarRegistro(evento.payload);
		}),
		await listen<{ ok: boolean; error: string | null }>('instalacion://fin', (evento) => {
			if (evento.payload.ok) {
				store.terminada = true;
				irA('fin');
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
		errorAlArrancar.value = String(error);
	}
});

onUnmounted(() => {
	for (const fn of desuscribir.value) fn();
	// Por si la ventana se cierra antes de instalar: las contraseñas no tienen
	// por qué sobrevivir al componente.
	store.olvidarSecretos();
});
</script>

<template>
  <WindowAppLayout>
    <!--
      La barra de título propia: icono a la izquierda, nombre al medio. Sin esto
      quedaba con los tres botones de la ventana flotando sobre nada — y como la
      ventana no tiene decoración del compositor, el nombre de la aplicación no
      aparecía en ningún otro lado.
    -->
    <template #identidad>
      <IconoSimbolo nombre="system-software-install" clase="size-5" />
    </template>
    <template #titulo>
      <span class="truncate font-medium text-sm">{{ t('app.nombre') }}</span>
    </template>

    <div class="flex min-h-0 w-full flex-1">
      <PasosSidebar
        :actual="store.paso"
        :navegable="!store.navegacionBloqueada"
        @ir="irA"
      />

      <div class="flex min-w-0 flex-1 flex-col">
        <!--
          `tabindex="-1"` para poder mover el foco acá al cambiar de paso sin
          meter el contenedor en el orden de tabulación. Es lo que hace que un
          lector de pantalla anuncie la página nueva en vez de seguir donde
          estaba.
        -->
        <main ref="contenido" tabindex="-1" class="min-h-0 flex-1 overflow-y-auto p-6 outline-none">
          <component :is="vistas[store.paso]" @cancelar="cancelar" />

          <p v-if="errorAlArrancar" role="alert" class="mt-4 text-status-error text-sm">
            {{ errorAlArrancar }}
          </p>
        </main>

        <footer
          v-if="muestraNavegacion"
          class="flex shrink-0 items-center justify-between gap-3 border-ui-border border-t p-4"
        >
          <button
            type="button"
            :disabled="indice === 0 || store.navegacionBloqueada"
            class="rounded-corner border border-ui-border-strong px-4 py-2 text-sm transition-colors hover:bg-ui-surface disabled:cursor-not-allowed disabled:opacity-40"
            @click="atras"
          >
            {{ t('comun.atras') }}
          </button>

          <button
            type="button"
            :disabled="!store.puedeAvanzar(store.paso)"
            class="rounded-corner px-4 py-2 font-medium text-sm transition-opacity disabled:cursor-not-allowed disabled:opacity-40"
            :class="
              store.paso === 'resumen'
                ? 'bg-status-error text-ui-bg hover:opacity-90'
                : 'bg-primary text-tx-on-primary hover:opacity-90'
            "
            @click="siguiente"
          >
            {{ store.paso === 'resumen' ? t('resumen.confirmar') : t('comun.siguiente') }}
          </button>
        </footer>
      </div>
    </div>
  </WindowAppLayout>
</template>
