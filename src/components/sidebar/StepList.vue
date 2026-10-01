<script setup lang="ts">
/**
 * Los pasos de la instalación, como lista.
 *
 * La dibujan dos lugares: la barra lateral (`StepsSidebar`) y, en una ventana
 * angosta, la ficha de pasos que reemplaza al contenido (`App.vue`), donde no
 * entran dos columnas. Una sola lista para los dos, para que no se separen.
 *
 * # Cada paso es un `SideButton`
 *
 * Antes lo dibujaba `PasoBoton`, propio, porque el botón de la librería no
 * tenía descripción ni lugar para un icono con estado. La 2.1 le sumó las dos
 * cosas (la descripción y la ranura `icon`), y el recuadro con el emblema de
 * terminado es `IconTile` con `status` (vue-libvasak#74). Lo único que sigue
 * siendo del instalador es lo que dice cada paso:
 *
 * - **el estado**: hecho (recuadro y emblema de éxito), actual (el velo de lo
 *   elegido) o pendiente (apagado, porque no se puede ir);
 * - **el número**, en la insignia: dice cuántos faltan;
 * - **`aria-current="step"`** y no `page`: esto es un asistente, no una
 *   navegación. Va como atributo y le gana al de la librería, que Vue fusiona
 *   el atributo de afuera encima del de la raíz;
 * - **el nombre con el número** cuando la barra está plegada, en `title` y en
 *   `aria-label`: en 84 píxeles no entra el texto, y sin eso la barra es una
 *   columna de dibujos y diez botones sin nombre.
 *
 * # El paso actual no se apaga
 *
 * No se puede volver al paso en el que se está, así que su botón va
 * deshabilitado como los demás; pero `SideButton` deshabilitado baja la
 * opacidad, y el paso actual apagado es justo el que no tiene que verse así.
 * `aria-[current=step]:opacity-100` lo devuelve: el selector de atributo pesa
 * más que la clase suelta de la librería, sin depender del orden de la hoja.
 * */
import { useI18n } from '@vasakgroup/tauri-plugin-i18n';
import { IconTile, type IconTileTone, SideButton } from '@vasakgroup/vue-libvasak';
import { computed } from 'vue';
import { PASOS, type Paso } from '@/stores/instalacion';
import { STEP_ICONS } from '@/tools/icons';

interface Props {
	current: Paso;
	/** Falso una vez que se empezó a escribir en el disco. */
	navigable: boolean;
	/** Plegada: sólo el icono, y el nombre con el número en `title` y `aria-label`. */
	collapsed?: boolean;
}
const props = withDefaults(defineProps<Props>(), { collapsed: false });
const emit = defineEmits<{ go: [step: Paso] }>();

const { t } = useI18n();

type StepState = 'done' | 'current' | 'pending';

const currentIndex = computed(() => PASOS.indexOf(props.current));

function stateOf(index: number): StepState {
	if (index < currentIndex.value) return 'done';
	if (index === currentIndex.value) return 'current';
	return 'pending';
}

const TILE_TONE: Record<StepState, IconTileTone> = {
	done: 'success',
	current: 'selected',
	pending: 'neutral',
};

/** Se puede volver a un paso ya hecho, y sólo mientras no se empezó a escribir. */
function canGoTo(index: number): boolean {
	return props.navigable && index < currentIndex.value;
}

/**
 * Lo que el paso le pone encima al botón de la librería: `aria-current="step"`,
 * y plegada, el nombre con el número en `title` y `aria-label`. Va por
 * `v-bind` porque `SideButton` no los declara —le llegan como atributos, y Vue
 * los fusiona encima de los suyos— y `strictTemplates` mide contra la lista de
 * propiedades.
 */
function ownAttributes(step: Paso, index: number): Record<string, string | undefined> {
	const name = props.collapsed ? `${index + 1}. ${t(`pasos.${step}.titulo`)}` : undefined;
	return {
		'aria-current': stateOf(index) === 'current' ? 'step' : undefined,
		title: name,
		'aria-label': name,
	};
}

function go(step: Paso, index: number) {
	if (canGoTo(index)) emit('go', step);
}
</script>

<template>
  <ol :aria-label="t('barraLateral.steps')" class="flex flex-col gap-1">
    <!--
      Una lista ordenada con su nombre: para un lector de pantalla esto es
      «lista de 10 elementos, elemento 5», que es exactamente la información
      que la barra le da a quien la ve. (El comentario va adentro y no antes
      del `<ol>`: antes de la raíz, la plantilla queda partida en dos y el
      componente pierde su raíz.)
    -->
    <li v-for="(step, index) in PASOS" :key="step" :data-step="step" :data-state="stateOf(index)">
      <SideButton
        :label="t(`pasos.${step}.titulo`)"
        :description="t(`pasos.${step}.descripcion`)"
        :badge="index + 1"
        :active="stateOf(index) === 'current'"
        :collapsed="collapsed"
        :disabled="!canGoTo(index)"
        v-bind="ownAttributes(step, index)"
        class="aria-[current=step]:opacity-100"
        @click="go(step, index)">
        <template #icon>
          <!--
            El icono del paso siempre, y el estado como emblema encima.
            Reemplazarlo por un tilde dejaba los pasos completados iguales
            entre sí: la barra perdía la única pista visual de qué era cada
            uno, justo cuando sirve para volver. El emblema es un glifo y no
            sólo un color (WCAG 1.4.1).
          -->
          <IconTile
            :name="STEP_ICONS[step]"
            type="symbol"
            size="sm"
            :tone="TILE_TONE[stateOf(index)]"
            :status="stateOf(index) === 'done' ? 'success' : null" />
        </template>
      </SideButton>
    </li>
  </ol>
</template>
