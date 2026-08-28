<script setup lang="ts">
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { computed, onMounted, watch } from 'vue';
import AlertMessage from '@/components/ui/AlertMessage.vue';
import PageHeader from '@/components/ui/PageHeader.vue';
import SectionCard from '@/components/ui/SectionCard.vue';
import SwitchToggle from '@/components/ui/SwitchToggle.vue';
import TextInput from '@/components/ui/TextInput.vue';
import { type Disco, type SistemaArchivos, useInstalacionStore } from '@/stores/instalacion';
import { formatearBytes } from '@/tools/formato';
import { interpolar } from '@/tools/interpolar';

const { t, locale } = useI18n();
const store = useInstalacionStore();

/** El mínimo que exige el backend. Duplicado acá sólo para el texto del aviso. */
const MINIMO_GIB = 20;

const sistemasDeArchivos: { valor: SistemaArchivos; nombre: string; ayuda: string }[] = [
	{ valor: 'btrfs', nombre: 'disco.btrfsNombre', ayuda: 'disco.btrfsAyuda' },
	{ valor: 'ext4', nombre: 'disco.ext4Nombre', ayuda: 'disco.ext4Ayuda' },
	{ valor: 'xfs', nombre: 'disco.xfsNombre', ayuda: 'disco.xfsAyuda' },
];

function tamano(bytes: number) {
	return formatearBytes(bytes, locale.value);
}

function muyChico(disco: Disco) {
	return disco.tamano_bytes < MINIMO_GIB * 1024 ** 3;
}

const frasesDistintas = computed(
	() =>
		store.eleccion.cifrar &&
		store.secretos.cifradoRepetida.length > 0 &&
		store.secretos.cifrado !== store.secretos.cifradoRepetida
);

// La vista previa se recalcula cuando cambia cualquier cosa que la afecte. Es lo
// que garantiza que el resumen muestre el plan de verdad y no uno viejo: sin
// esto, cambiar de btrfs a ext4 dejaba los subvolúmenes listados en el resumen.
watch(
	() => [store.eleccion.disco, store.eleccion.sistemaArchivos, store.eleccion.cifrar],
	() => store.calcularVistaPrevia(),
	{ immediate: true }
);

onMounted(async () => {
	// Acá es donde por primera vez hace falta root, y donde ya se entiende para
	// qué. Si la autorización se rechaza, el paso sigue funcionando —la lista de
	// discos sale igual, sin los nombres de los sistemas instalados— y el aviso
	// aparece recién en el resumen, que es donde bloquea.
	await store.prepararAyudante();
});
</script>

<template>
  <div>
    <PageHeader :titulo="t('disco.titulo')" :descripcion="t('disco.intro')" />

    <div class="space-y-4">
      <div v-if="store.discos.length === 0">
        <AlertMessage tipo="error" :titulo="t('disco.sinDiscos')">
          {{ t('disco.sinDiscosDetalle') }}
        </AlertMessage>
      </div>

      <ul v-else class="space-y-2">
        <li v-for="disco in store.discos" :key="disco.ruta">
          <!--
            El disco en uso y el demasiado chico se muestran igual, deshabilitados
            y con el motivo escrito. Ocultarlos haría que alguien busque un disco
            que sabe que existe y no lo encuentre, sin ninguna explicación.
          -->
          <button
            type="button"
            :disabled="disco.en_uso || muyChico(disco)"
            class="w-full rounded-corner border p-3 text-left transition-colors"
            :class="[
              store.eleccion.disco === disco.ruta
                ? 'border-secondary bg-primary/10'
                : 'border-ui-border-strong hover:bg-ui-surface/50',
              disco.en_uso || muyChico(disco) ? 'cursor-not-allowed opacity-60' : '',
            ]"
            @click="store.eleccion.disco = disco.ruta"
          >
            <div class="flex items-baseline justify-between gap-3">
              <span class="min-w-0 flex-1 truncate font-medium text-sm">{{ disco.modelo }}</span>
              <span class="shrink-0 font-mono text-sm">{{ tamano(disco.tamano_bytes) }}</span>
            </div>
            <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-tx-muted text-xs">
              <span class="font-mono">{{ disco.ruta }}</span>
              <span v-if="disco.nvme">NVMe</span>
              <span v-else-if="disco.rotacional">HDD</span>
              <span v-else>SSD</span>
              <span>
                {{
                  disco.particiones.length === 0
                    ? t('disco.vacio')
                    : interpolar(t('disco.conParticiones'), disco.particiones.length)
                }}
              </span>
            </div>

            <p v-if="disco.en_uso" class="mt-2 text-status-warning text-xs">
              {{ t('disco.enUso') }} — {{ t('disco.enUsoDetalle') }}
            </p>
            <p v-else-if="muyChico(disco)" class="mt-2 text-status-warning text-xs">
              {{ interpolar(t('disco.muyChico'), MINIMO_GIB) }}
            </p>

            <!--
              Lo que hay adentro, con el nombre del sistema operativo cuando se
              pudo averiguar. «Windows 11» hace que alguien se detenga a mirar;
              «ntfs» no.
            -->
            <ul
              v-else-if="disco.particiones.length > 0"
              class="mt-2 space-y-0.5 text-tx-muted text-xs"
            >
              <li v-for="particion in disco.particiones" :key="particion.ruta" class="truncate">
                <span class="font-mono">{{ particion.ruta }}</span>
                <span class="mx-1">·</span>
                <span>{{ tamano(particion.tamano_bytes) }}</span>
                <template v-if="particion.sistema_operativo">
                  <span class="mx-1">·</span>
                  <span class="font-medium">{{ particion.sistema_operativo }}</span>
                </template>
                <template v-else-if="particion.sistema_archivos">
                  <span class="mx-1">·</span>
                  <span>{{ particion.sistema_archivos }}</span>
                </template>
                <template v-else>
                  <span class="mx-1">·</span>
                  <span>{{ t('disco.sinFormato') }}</span>
                </template>
              </li>
            </ul>
          </button>
        </li>
      </ul>

      <SectionCard :titulo="t('disco.sistemaArchivos')">
        <div class="space-y-1">
          <SwitchToggle
            v-for="fs in sistemasDeArchivos"
            :key="fs.valor"
            :model-value="store.eleccion.sistemaArchivos === fs.valor"
            :label="t(fs.nombre)"
            :descripcion="t(fs.ayuda)"
            @update:model-value="store.eleccion.sistemaArchivos = fs.valor"
          />
        </div>
      </SectionCard>

      <SectionCard>
        <SwitchToggle
          v-model="store.eleccion.zram"
          :label="t('disco.zram')"
          :descripcion="t('disco.zramAyuda')"
        />
        <SwitchToggle
          v-model="store.eleccion.cifrar"
          :label="t('disco.cifrar')"
          :descripcion="t('disco.cifrarAyuda')"
        />

        <div v-if="store.eleccion.cifrar" class="mt-3 space-y-3">
          <AlertMessage tipo="aviso" :titulo="t('disco.cifrarAvisoTitulo')">
            {{ t('disco.cifrarAviso') }}
          </AlertMessage>

          <div>
            <label for="frase" class="mb-1 block text-sm">{{ t('disco.frase') }}</label>
            <TextInput
              id="frase"
              v-model="store.secretos.cifrado"
              type="password"
              autocomplete="new-password"
              :placeholder="t('disco.frasePlaceholder')"
            />
          </div>
          <div>
            <label for="frase2" class="mb-1 block text-sm">{{ t('disco.fraseRepetir') }}</label>
            <TextInput
              id="frase2"
              v-model="store.secretos.cifradoRepetida"
              type="password"
              autocomplete="new-password"
              :invalid="frasesDistintas"
              described-by="frase-error"
            />
            <p v-if="frasesDistintas" id="frase-error" class="mt-1 text-status-error text-xs">
              {{ t('disco.frasesDistintas') }}
            </p>
          </div>
        </div>
      </SectionCard>

      <SectionCard v-if="store.vistaPrevia" :titulo="t('disco.detalleParticionado')">
        <ul class="space-y-2 text-sm">
          <li
            v-for="(particion, indice) in store.vistaPrevia.particiones"
            :key="indice"
            class="rounded-corner border border-ui-border p-2"
          >
            <div class="flex items-baseline justify-between gap-2">
              <span class="font-medium">
                {{
                  particion.rol === 'esp'
                    ? t('disco.rolEsp')
                    : particion.rol === 'bios_grub'
                      ? t('disco.rolBiosGrub')
                      : t('disco.rolRaiz')
                }}
              </span>
              <span class="font-mono text-xs">{{ tamano(particion.tamano_bytes) }}</span>
            </div>
            <div class="mt-1 flex flex-wrap gap-x-3 text-tx-muted text-xs">
              <span v-if="particion.sistema_archivos" class="font-mono">
                {{ particion.sistema_archivos }}
              </span>
              <span v-if="particion.punto_montaje" class="font-mono">
                {{ particion.punto_montaje }}
              </span>
              <span v-if="particion.cifrada" class="text-status-warning">
                {{ t('disco.cifradaEtiqueta') }}
              </span>
            </div>
            <p v-if="particion.opciones_montaje.length" class="mt-1 text-tx-muted text-xs">
              {{ t('disco.opcionesMontaje') }}:
              <span class="font-mono">{{ particion.opciones_montaje.join(',') }}</span>
            </p>
            <p v-if="particion.subvolumenes.length" class="mt-1 text-tx-muted text-xs">
              {{ t('disco.subvolumenes') }}:
              <span class="font-mono">{{ particion.subvolumenes.join('   ') }}</span>
            </p>
          </li>
        </ul>
      </SectionCard>
    </div>
  </div>
</template>
