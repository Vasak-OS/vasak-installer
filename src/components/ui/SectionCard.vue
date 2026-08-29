<script setup lang="ts">
/**
 * La tarjeta que agrupa un bloque de opciones.
 *
 * El título va en un `<h2>` de verdad y no en un `div` con clases: el instalador
 * es la primera pantalla que alguien ve del sistema, y si se recorre con un
 * lector de pantalla la estructura tiene que existir. En vasak-settings la prop
 * `title` de este mismo componente estaba declarada y **nunca se renderizaba**,
 * así que dos páginas enteras salían sin sus títulos.
 */
interface Props {
	titulo?: string;
	descripcion?: string;
}
defineProps<Props>();
</script>

<template>
  <section class="rounded-corner border border-ui-border bg-ui-surface/40 p-4">
    <header v-if="titulo || descripcion || $slots.encabezado" class="mb-3">
      <slot name="encabezado">
        <h2 v-if="titulo" class="font-semibold text-sm">{{ titulo }}</h2>
        <p v-if="descripcion" class="mt-1 text-tx-muted text-xs">{{ descripcion }}</p>
      </slot>
    </header>
    <slot />
  </section>
</template>
