<script setup lang="ts">
/**
 * Lo que dice la tarjeta de un disco, adentro de la opción del grupo.
 *
 * La tarjeta —el recuadro con el icono, el punto de radio, el estado elegido o
 * deshabilitado— es `OptionGroup variant="card"` de la librería. Esto es sólo
 * el texto: modelo y tamaño, ruta y tipo, cuántas particiones tiene y cuáles,
 * con el nombre del sistema operativo cuando se pudo averiguar, y el motivo
 * cuando el disco no se puede usar.
 *
 * Todo se acomoda por el ancho que tiene (`flex-wrap`, `break-words`) y nada se
 * corta: en una ventana angosta el tamaño baja a la línea de abajo, en vez de
 * desaparecer del costado como antes.
 */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import type { Disco } from '@/stores/instalacion';
import { formatearBytes } from '@/tools/formato';
import { interpolar } from '@/tools/interpolar';

defineProps<{
	disk: Disco;
	/** Por debajo del mínimo que pide el backend. */
	tooSmall: boolean;
	/** El mínimo, en GiB, para el texto del motivo. */
	minimumGib: number;
}>();

const { t, locale } = useI18n();

function size(bytes: number) {
	return formatearBytes(bytes, locale.value);
}
</script>

<template>
  <span class="flex min-w-0 flex-col gap-2" :data-disk="disk.ruta">
    <span class="flex min-w-0 flex-wrap items-baseline justify-between gap-x-3 gap-y-0.5">
      <span class="min-w-0 break-words font-semibold text-label-m">{{ disk.modelo }}</span>
      <span class="shrink-0 font-mono text-sm">{{ size(disk.tamano_bytes) }}</span>
    </span>
    <span class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-tx-muted text-xs">
      <span class="break-all font-mono">{{ disk.ruta }}</span>
      <span v-if="disk.nvme">NVMe</span>
      <span v-else-if="disk.rotacional">HDD</span>
      <span v-else>SSD</span>
      <span>
        {{
          disk.particiones.length === 0
            ? t('disco.vacio')
            : interpolar(t('disco.conParticiones'), disk.particiones.length)
        }}
      </span>
    </span>

    <span v-if="disk.en_uso" class="break-words text-status-warning text-xs">
      {{ t('disco.enUso') }} — {{ t('disco.enUsoDetalle') }}
    </span>
    <span v-else-if="tooSmall" class="break-words text-status-warning text-xs">
      {{ interpolar(t('disco.muyChico'), minimumGib) }}
    </span>

    <!--
      Lo que hay adentro, con el nombre del sistema operativo cuando se
      pudo averiguar. «Windows 11» hace que alguien se detenga a mirar;
      «ntfs» no. Sin `truncate`: en una ventana angosta la línea baja,
      en vez de cortar justo el nombre del sistema que se va a borrar.
    -->
    <span
      v-else-if="disk.particiones.length > 0"
      class="flex min-w-0 flex-col gap-0.5 text-tx-muted text-xs"
    >
      <!-- Cada dato es un bloque que baja entero a la línea de abajo y sólo se
           parte si no entra solo: así la ruta no queda cortada en «nvme0n1p» y
           un «1» suelto, que es la que se lee para saber qué se borra. -->
      <span
        v-for="partition in disk.particiones"
        :key="partition.ruta"
        class="flex min-w-0 flex-wrap items-baseline gap-x-1"
      >
        <span class="max-w-full break-words font-mono">{{ partition.ruta }}</span>
        <span aria-hidden="true">·</span>
        <span class="whitespace-nowrap">{{ size(partition.tamano_bytes) }}</span>
        <span aria-hidden="true">·</span>
        <span v-if="partition.sistema_operativo" class="max-w-full break-words font-medium">
          {{ partition.sistema_operativo }}
        </span>
        <span v-else-if="partition.sistema_archivos" class="max-w-full break-words">
          {{ partition.sistema_archivos }}
        </span>
        <span v-else class="max-w-full break-words">{{ t('disco.sinFormato') }}</span>
      </span>
    </span>
  </span>
</template>
