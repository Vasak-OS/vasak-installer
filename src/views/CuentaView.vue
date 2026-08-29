<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, ref, watch } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SwitchToggle from '@/components/ui/SwitchToggle.vue';
import TextInput from '@/components/ui/TextInput.vue';
import { useInstalacionStore } from '@/stores/instalacion';
import { ICONO_PASO } from '@/tools/iconos';
import { interpolar } from '@/tools/interpolar';

const { t } = useI18n();
const store = useInstalacionStore();

/** El motivo que devolvió el backend, o `null` si el nombre es válido. */
type Motivo = { motivo: string; maximo?: number; cual?: string } | null;

const errorUsuario = ref<Motivo>(null);
const errorEquipo = ref<Motivo>(null);
const fuerza = ref<'vacia' | 'debil' | 'aceptable' | 'buena'>('vacia');

/**
 * Traduce el motivo que devolvió el backend.
 *
 * Los mensajes específicos van primero: «tiene que empezar con una letra
 * minúscula» dice qué hacer, y «no puede empezar así» sólo dice que está mal.
 */
function textoDeError(motivo: Motivo, campo: 'usuario' | 'equipo'): string {
	if (!motivo) return '';
	switch (motivo.motivo) {
		case 'vacio':
			return t('errores.vacio');
		case 'largo':
			return interpolar(t('errores.largo'), motivo.maximo ?? 0);
		case 'caracter':
			return interpolar(t('errores.caracter'), motivo.cual ?? '');
		case 'empieza_mal':
			return campo === 'usuario' ? t('errores.usuarioEmpiezaMal') : t('errores.equipoEmpiezaMal');
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
let usuarioEnVuelo = 0;
let equipoEnVuelo = 0;

async function validarUsuario() {
	const mia = ++usuarioEnVuelo;
	if (!store.eleccion.usuario) {
		errorUsuario.value = null;
		return;
	}
	try {
		await invoke('validar_usuario', { nombre: store.eleccion.usuario });
		if (mia !== usuarioEnVuelo) return;
		errorUsuario.value = null;
	} catch (error) {
		if (mia !== usuarioEnVuelo) return;
		errorUsuario.value = error as Motivo;
	}
}

async function validarEquipo() {
	const mia = ++equipoEnVuelo;
	if (!store.eleccion.hostname) {
		errorEquipo.value = null;
		return;
	}
	try {
		await invoke('validar_equipo', { nombre: store.eleccion.hostname });
		if (mia !== equipoEnVuelo) return;
		errorEquipo.value = null;
	} catch (error) {
		if (mia !== equipoEnVuelo) return;
		errorEquipo.value = error as Motivo;
	}
}

watch(() => store.eleccion.usuario, validarUsuario, { immediate: true });
watch(() => store.eleccion.hostname, validarEquipo, { immediate: true });

/**
 * El nombre de usuario se propone a partir del nombre completo, **hasta que la
 * persona lo toca**.
 *
 * Sin esa condición, escribir el usuario y después corregir un acento del nombre
 * completo pisaba lo que ya se había escrito a mano.
 */
const usuarioTocado = ref(false);
let sugerenciaEnVuelo = 0;
watch(
	() => store.eleccion.nombreCompleto,
	async (nombre) => {
		if (usuarioTocado.value) return;
		const mia = ++sugerenciaEnVuelo;
		const sugerido = await invoke<string>('sugerir_usuario', { nombreCompleto: nombre });
		// Misma carrera que las validaciones: una sugerencia vieja que llega
		// tarde pisaría el campo con la de un nombre que ya se cambió.
		if (mia !== sugerenciaEnVuelo || usuarioTocado.value) return;
		store.eleccion.usuario = sugerido;
	}
);

watch(
	() => store.secretos.usuario,
	async (valor) => {
		fuerza.value = await invoke('fuerza_contrasena', { contrasena: valor });
	},
	{ immediate: true }
);

const contrasenasDistintas = computed(
	() =>
		store.secretos.usuarioRepetida.length > 0 &&
		store.secretos.usuario !== store.secretos.usuarioRepetida
);

const rootDistintas = computed(
	() =>
		store.eleccion.rootHabilitado &&
		store.secretos.rootRepetida.length > 0 &&
		store.secretos.root !== store.secretos.rootRepetida
);

const nadiePuedeAdministrar = computed(
	() => !store.eleccion.administrador && !store.eleccion.rootHabilitado
);

const textoFuerza = computed(() => {
	switch (fuerza.value) {
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

const colorFuerza = computed(() => {
	switch (fuerza.value) {
		case 'buena':
			return 'bg-status-success';
		case 'aceptable':
			return 'bg-status-warning';
		case 'debil':
			return 'bg-status-error';
		default:
			return 'bg-ui-surface';
	}
});

const anchoFuerza = computed(() => {
	switch (fuerza.value) {
		case 'buena':
			return 'w-full';
		case 'aceptable':
			return 'w-2/3';
		case 'debil':
			return 'w-1/3';
		default:
			return 'w-0';
	}
});
</script>

<template>
  <div>
    <PageHeader :icono="ICONO_PASO.cuenta" :titulo="t('cuenta.titulo')" />

    <div class="space-y-4">
      <SectionCard>
        <div class="space-y-3">
          <div>
            <label for="nombre" class="mb-1 block text-sm">{{ t('cuenta.nombreCompleto') }}</label>
            <TextInput
              id="nombre"
              v-model="store.eleccion.nombreCompleto"
              autocomplete="name"
              :placeholder="t('cuenta.nombreCompletoPlaceholder')"
            />
          </div>

          <div>
            <label for="usuario" class="mb-1 block text-sm">{{ t('cuenta.usuario') }}</label>
            <TextInput
              id="usuario"
              v-model="store.eleccion.usuario"
              mono
              autocomplete="username"
              :invalid="errorUsuario !== null"
              described-by="usuario-ayuda"
              @update:model-value="usuarioTocado = true"
            />
            <p
              v-if="errorUsuario"
              id="usuario-ayuda"
              class="mt-1 text-status-error text-xs"
            >
              {{ textoDeError(errorUsuario, 'usuario') }}
            </p>
            <p v-else id="usuario-ayuda" class="mt-1 text-tx-muted text-xs">
              {{ t('cuenta.usuarioAyuda') }}
            </p>
          </div>
        </div>
      </SectionCard>

      <SectionCard>
        <div class="space-y-3">
          <div>
            <label for="clave" class="mb-1 block text-sm">{{ t('cuenta.contrasena') }}</label>
            <TextInput
              id="clave"
              v-model="store.secretos.usuario"
              type="password"
              autocomplete="new-password"
            />
            <div class="mt-2 flex items-center gap-2">
              <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ui-surface">
                <div
                  class="h-full rounded-full transition-all"
                  :class="[colorFuerza, anchoFuerza]"
                />
              </div>
              <!--
                El nivel también va escrito, no sólo en el color de la barra: una
                barra roja y una verde son el mismo gris para quien no distingue
                esos dos colores (WCAG 1.4.1).
              -->
              <span class="shrink-0 text-tx-muted text-xs">
                {{ t('cuenta.fuerza') }}: {{ textoFuerza }}
              </span>
            </div>
            <p class="mt-1 text-tx-muted text-xs">{{ t('cuenta.fuerzaAyuda') }}</p>
          </div>

          <div>
            <label for="clave2" class="mb-1 block text-sm">{{ t('cuenta.contrasenaRepetir') }}</label>
            <TextInput
              id="clave2"
              v-model="store.secretos.usuarioRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="contrasenasDistintas"
              described-by="clave-error"
            />
            <p v-if="contrasenasDistintas" id="clave-error" class="mt-1 text-status-error text-xs">
              {{ t('cuenta.contrasenasDistintas') }}
            </p>
          </div>
        </div>
      </SectionCard>

      <SectionCard>
        <label for="equipo" class="mb-1 block text-sm">{{ t('cuenta.equipo') }}</label>
        <TextInput
          id="equipo"
          v-model="store.eleccion.hostname"
          mono
          :invalid="errorEquipo !== null"
          described-by="equipo-ayuda"
        />
        <p v-if="errorEquipo" id="equipo-ayuda" class="mt-1 text-status-error text-xs">
          {{ textoDeError(errorEquipo, 'equipo') }}
        </p>
        <p v-else id="equipo-ayuda" class="mt-1 text-tx-muted text-xs">
          {{ t('cuenta.equipoAyuda') }}
        </p>
      </SectionCard>

      <SectionCard>
        <SwitchToggle
          v-model="store.eleccion.administrador"
          :label="t('cuenta.administrador')"
          :descripcion="t('cuenta.administradorAyuda')"
        />
        <SwitchToggle
          v-model="store.eleccion.rootHabilitado"
          :label="t('cuenta.rootHabilitado')"
          :descripcion="t('cuenta.rootAyuda')"
        />

        <div v-if="store.eleccion.rootHabilitado" class="mt-3 space-y-3">
          <div>
            <label for="root1" class="mb-1 block text-sm">{{ t('cuenta.rootContrasena') }}</label>
            <TextInput
              id="root1"
              v-model="store.secretos.root"
              type="password"
              autocomplete="new-password"
            />
          </div>
          <div>
            <label for="root2" class="mb-1 block text-sm">
              {{ t('cuenta.rootContrasenaRepetir') }}
            </label>
            <TextInput
              id="root2"
              v-model="store.secretos.rootRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="rootDistintas"
              described-by="root-error"
            />
            <p v-if="rootDistintas" id="root-error" class="mt-1 text-status-error text-xs">
              {{ t('cuenta.contrasenasDistintas') }}
            </p>
          </div>
        </div>
      </SectionCard>

      <!--
        No se bloquea: se avisa. Puede haber una razón para instalar un equipo
        sin nadie que lo administre —un kiosco, una máquina de prueba— y quien
        elige eso a propósito no necesita que se lo impidan. Quien lo eligió sin
        querer, sí necesita enterarse.
      -->
      <AlertMessage
        v-if="nadiePuedeAdministrar"
        tipo="aviso"
        :titulo="t('cuenta.sinAdminNiRootTitulo')"
      >
        {{ t('cuenta.sinAdminNiRoot') }}
      </AlertMessage>
    </div>
  </div>
</template>
