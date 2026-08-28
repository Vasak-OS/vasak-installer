import { getIconSource } from '@vasakgroup/plugin-vicons';
import { setupContextMenu } from '@vasakgroup/plugin-vsk-contextual-menu';
import { captureFailures } from '@vasakgroup/plugin-vsk-journal';
import I18n from '@vasakgroup/tauri-plugin-i18n';
import { createPinia } from 'pinia';
import { createApp } from 'vue';
import App from '@/App.vue';
import '@/assets/main.css';

/**
 * Cuánto se espera a las traducciones antes de montar.
 *
 * Se espera para que la primera pantalla no muestre las claves crudas, pero con
 * un plazo: si el backend no contesta, es mejor una interfaz con las claves a la
 * vista que una ventana en blanco para siempre.
 */
const PLAZO_TRADUCCIONES_MS = 3000;

/**
 * Saca de una URL lo que no debería quedar en un registro.
 *
 * Se conserva el esquema y la autoridad usando `href`, y no `origin + pathname`:
 * para esquemas propios como `asset:` o `ipc:` el `origin` es la cadena «null».
 * Y el `catch` no devuelve el valor tal cual —una ruta relativa dejaría la query
 * en el registro—, sólo pasan los marcadores que informa la especificación.
 */
const MARCADORES_CSP = new Set([
	'inline',
	'eval',
	'wasm-eval',
	'data',
	'blob',
	'filesystem',
	'self',
	'unsafe-eval',
	'unsafe-inline',
]);

const sanearUrl = (valor: string | null | undefined): string => {
	if (!valor) {
		return '';
	}
	try {
		const url = new URL(valor);
		if (url.protocol === 'data:') {
			return 'data:(recortado)';
		}
		url.username = '';
		url.password = '';
		url.search = '';
		url.hash = '';
		return url.href;
	} catch {
		if (MARCADORES_CSP.has(valor)) {
			return valor;
		}
		return valor.split(/[?#]/)[0];
	}
};

// Una violación de CSP no se ve: el recurso no carga y la interfaz queda a
// medias sin decir nada. Esto la manda a la consola, saneada.
document.addEventListener('securitypolicyviolation', (evento) => {
	const recurso = evento.blockedURI ? sanearUrl(evento.blockedURI) : '(en línea)';
	const origen = evento.sourceFile ? sanearUrl(evento.sourceFile) : 'documento';
	console.error(
		`[CSP] bloqueado ${recurso} por la directiva ` +
			`«${evento.violatedDirective}» en ${origen}:${evento.lineNumber}`
	);
});

// Lo que rompe la interfaz va al diario del sistema, con el nombre de esta
// aplicación. Antes no iba a ninguna parte: un error de JavaScript deja la
// pantalla a medias y no queda registro de por qué, y la consola del WebView no
// la ve nadie en una máquina instalada.
//
// Va lo más arriba posible, antes de armar la aplicación, para que también
// atrape lo que falle durante el arranque.
captureFailures();

// El clic derecho abre el menú de VasakOS —el mismo de todo el escritorio— y no
// el del motor del navegador, que ofrece «Recargar» e «Inspeccionar elemento».
setupContextMenu({ iconResolver: getIconSource });

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);

// Un error de Vue en producción no va a ninguna parte; al menos que quede en la
// consola con el contexto de dónde ocurrió.
app.config.errorHandler = (error, _instancia, info) => {
	console.error(`[vue] falló en ${info}:`, error);
};

// Se esperan las traducciones antes de montar, con plazo: montando primero, el
// arranque enseña las claves crudas hasta que el archivo de idioma termina de
// cargar.
await Promise.race([
	I18n.getInstance()
		.load()
		.catch((error) => {
			console.error('No se pudieron cargar las traducciones', error);
		}),
	new Promise((resolve) => setTimeout(resolve, PLAZO_TRADUCCIONES_MS)),
]);

app.mount('#app');
