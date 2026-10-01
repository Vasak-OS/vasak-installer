<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import {
	AlertMessage,
	FormGroup,
	PageHeader,
	Panel,
	ProgressBar,
	SwitchRow,
	TextInput,
} from '@vasakgroup/vue-libvasak';
import { computed, ref, watch } from 'vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();

/** El motivo que devolvió el backend, o `null` si el nombre es válido. */
type Reason = { motivo: string; maximo?: number; cual?: string } | null;

const userError = ref<Reason>(null);
const hostError = ref<Reason>(null);
const strength = ref<'vacia' | 'debil' | 'aceptable' | 'buena'>('vacia');

/**
 * Traduce el motivo que devolvió el backend.
 *
 * Los mensajes específicos van primero: «tiene que empezar con una letra
 * minúscula» dice qué hacer, y «no puede empezar así» sólo dice que está mal.
 */
function errorText(reason: Reason, field: 'usuario' | 'equipo'): string {
	if (!reason) return '';
	switch (reason.motivo) {
		case 'vacio':
			return t('errores.vacio');
		case 'largo':
			return interpolar(t('errores.largo'), reason.maximo ?? 0);
		case 'caracter':
			return interpolar(t('errores.caracter'), reason.cual ?? '');
		case 'empieza_mal':
			return field === 'usuario' ? t('errores.usuarioEmpiezaMal') : t('errores.equipoEmpiezaMal');
		case 'termina_mal':
			return t('errores.equipoTerminaMal');
		case 'reservado':
			return t('errores.reservado');
		default:
			return t('errores.desconocido');
	}
}

/**
 * Valida contra el backend en vez de repetir las reglas en TypeScript.
 *
 * Las reglas son las de `useradd`, y **la comprobación que decide es la del
 * backend**: un nombre inválido que pasara de largo lo rechaza `useradd` en
 * medio de la instalación, con el disco ya formateado. Repetirlas acá en
 * TypeScript garantizaría que las dos copias se separen; llamar es una ida y
 * vuelta por IPC que no se nota al tipear.
 */
/**
 * Contadores de las validaciones en vuelo.
 *
 * Se dispara una por tecla, y no hay garantía de que contesten en orden.
 * Escribiendo `pat` y borrando hasta `p`, la respuesta de `pat` puede llegar
 * después de la de `p` y dejar el campo marcado con un error que corresponde a
 * un texto que ya no está: la persona ve un error rojo sobre lo que acaba de
 * escribir bien, y no hay forma de sacárselo de encima más que seguir tipeando.
 */
let userInFlight = 0;
let hostInFlight = 0;

async function validateUser() {
	const mine = ++userInFlight;
	if (!store.eleccion.usuario) {
		userError.value = null;
		return;
	}
	try {
		await invoke('validar_usuario', { nombre: store.eleccion.usuario });
		if (mine !== userInFlight) return;
		userError.value = null;
	} catch (error) {
		if (mine !== userInFlight) return;
		userError.value = error as Reason;
	}
}

async function validateHost() {
	const mine = ++hostInFlight;
	if (!store.eleccion.hostname) {
		hostError.value = null;
		return;
	}
	try {
		await invoke('validar_equipo', { nombre: store.eleccion.hostname });
		if (mine !== hostInFlight) return;
		hostError.value = null;
	} catch (error) {
		if (mine !== hostInFlight) return;
		hostError.value = error as Reason;
	}
}

watch(() => store.eleccion.usuario, validateUser, { immediate: true });
watch(() => store.eleccion.hostname, validateHost, { immediate: true });

/**
 * El nombre de usuario se propone a partir del nombre completo, **hasta que la
 * persona lo toca**.
 *
 * Sin esa condición, escribir el usuario y después corregir un acento del nombre
 * completo pisaba lo que ya se había escrito a mano.
 */
const userTouched = ref(false);
let suggestionInFlight = 0;
watch(
	() => store.eleccion.nombreCompleto,
	async (fullName) => {
		if (userTouched.value) return;
		const mine = ++suggestionInFlight;
		const suggested = await invoke<string>('sugerir_usuario', { nombreCompleto: fullName });
		// Misma carrera que las validaciones: una sugerencia vieja que llega
		// tarde pisaría el campo con la de un nombre que ya se cambió.
		if (mine !== suggestionInFlight || userTouched.value) return;
		store.eleccion.usuario = suggested;
	}
);

watch(
	() => store.secretos.usuario,
	async (password) => {
		strength.value = await invoke('fuerza_contrasena', { contrasena: password });
	},
	{ immediate: true }
);

const passwordsDiffer = computed(
	() =>
		store.secretos.usuarioRepetida.length > 0 &&
		store.secretos.usuario !== store.secretos.usuarioRepetida
);

const rootPasswordsDiffer = computed(
	() =>
		store.eleccion.rootHabilitado &&
		store.secretos.rootRepetida.length > 0 &&
		store.secretos.root !== store.secretos.rootRepetida
);

const nobodyCanAdminister = computed(
	() => !store.eleccion.administrador && !store.eleccion.rootHabilitado
);

const strengthText = computed(() => {
	switch (strength.value) {
		case 'buena':
			return t('cuenta.fuerzaBuena');
		case 'aceptable':
			return t('cuenta.fuerzaAceptable');
		case 'debil':
			return t('cuenta.fuerzaDebil');
		default:
			return t('cuenta.fuerzaVacia');
	}
});

/**
 * El medidor de la fuerza: `ProgressBar` de la librería, en tercios.
 *
 * Era una barra a mano con el ancho en clases (`w-1/3`, `w-2/3`). El tono es
 * el del medidor de la librería: crítico, atención o normal. El nivel sigue
 * escrito al lado, no sólo en el color (WCAG 1.4.1).
 */
const STRENGTH_VALUE = { vacia: 0, debil: 33, aceptable: 67, buena: 100 } as const;
const STRENGTH_TONE = {
	vacia: 'normal',
	debil: 'critical',
	aceptable: 'warning',
	buena: 'normal',
} as const;

const strengthValue = computed(() => STRENGTH_VALUE[strength.value]);
const strengthTone = computed(() => STRENGTH_TONE[strength.value]);
</script>

<template>
  <div>
    <PageHeader class="mb-5" :icon="STEP_ICONS.cuenta" icon-type="symbol" :title="t('cuenta.titulo')" />

    <div class="space-y-4">
      <Panel>
        <div class="space-y-3">
          <FormGroup :label="t('cuenta.nombreCompleto')" html-for="nombre">
            <TextInput
              id="nombre"
              v-model="store.eleccion.nombreCompleto"
              autocomplete="name"
              :placeholder="t('cuenta.nombreCompletoPlaceholder')"
            />
          </FormGroup>

          <!-- La ayuda o el error, no los dos: el error **reemplaza** la ayuda,
               como antes. -->
          <FormGroup
            v-slot="{ id, describedBy, invalid }"
            :label="t('cuenta.usuario')"
            html-for="usuario"
            :help="userError ? '' : t('cuenta.usuarioAyuda')"
            :error="userError ? errorText(userError, 'usuario') : ''"
          >
            <TextInput
              :id="id"
              v-model="store.eleccion.usuario"
              mono
              autocomplete="username"
              :invalid="invalid"
              :described-by="describedBy"
              @update:model-value="userTouched = true"
            />
          </FormGroup>
        </div>
      </Panel>

      <Panel>
        <div class="space-y-3">
          <FormGroup :label="t('cuenta.contrasena')" html-for="clave" :help="t('cuenta.fuerzaAyuda')">
            <TextInput
              id="clave"
              v-model="store.secretos.usuario"
              type="password"
              autocomplete="new-password"
            />
            <div class="flex items-center gap-2">
              <div class="min-w-0 flex-1">
                <ProgressBar :value="strengthValue" :label="t('cuenta.fuerza')" :tone="strengthTone" size="sm" />
              </div>
              <!--
                El nivel también va escrito, no sólo en el color de la barra: una
                barra roja y una verde son el mismo gris para quien no distingue
                esos dos colores (WCAG 1.4.1).
              -->
              <span class="shrink-0 text-tx-muted text-xs">
                {{ t('cuenta.fuerza') }}: {{ strengthText }}
              </span>
            </div>
          </FormGroup>

          <FormGroup
            v-slot="{ id, describedBy, invalid }"
            :label="t('cuenta.contrasenaRepetir')"
            html-for="clave2"
            :error="passwordsDiffer ? t('cuenta.contrasenasDistintas') : ''"
          >
            <TextInput
              :id="id"
              v-model="store.secretos.usuarioRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </FormGroup>
        </div>
      </Panel>

      <Panel>
        <FormGroup
          v-slot="{ id, describedBy, invalid }"
          :label="t('cuenta.equipo')"
          html-for="equipo"
          :help="hostError ? '' : t('cuenta.equipoAyuda')"
          :error="hostError ? errorText(hostError, 'equipo') : ''"
        >
          <TextInput
            :id="id"
            v-model="store.eleccion.hostname"
            mono
            :invalid="invalid"
            :described-by="describedBy"
          />
        </FormGroup>
      </Panel>

      <Panel>
        <SwitchRow
          v-model="store.eleccion.administrador"
          :label="t('cuenta.administrador')"
          :description="t('cuenta.administradorAyuda')"
        />
        <SwitchRow
          v-model="store.eleccion.rootHabilitado"
          :label="t('cuenta.rootHabilitado')"
          :description="t('cuenta.rootAyuda')"
        />

        <div v-if="store.eleccion.rootHabilitado" class="mt-3 space-y-3">
          <FormGroup :label="t('cuenta.rootContrasena')" html-for="root1">
            <TextInput
              id="root1"
              v-model="store.secretos.root"
              type="password"
              autocomplete="new-password"
            />
          </FormGroup>
          <FormGroup
            v-slot="{ id, describedBy, invalid }"
            :label="t('cuenta.rootContrasenaRepetir')"
            html-for="root2"
            :error="rootPasswordsDiffer ? t('cuenta.contrasenasDistintas') : ''"
          >
            <TextInput
              :id="id"
              v-model="store.secretos.rootRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="invalid"
              :described-by="describedBy"
            />
          </FormGroup>
        </div>
      </Panel>

      <!--
        No se bloquea: se avisa. Puede haber una razón para instalar un equipo
        sin nadie que lo administre —un kiosco, una máquina de prueba— y quien
        elige eso a propósito no necesita que se lo impidan. Quien lo eligió sin
        querer, sí necesita enterarse.
      -->
      <AlertMessage
        v-if="nobodyCanAdminister"
        tone="warning"
        icon="auto"
        :title="t('cuenta.sinAdminNiRootTitulo')"
      >
        {{ t('cuenta.sinAdminNiRoot') }}
      </AlertMessage>
    </div>
  </div>
</template>
