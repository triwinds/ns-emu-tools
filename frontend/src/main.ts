/**
 * main.ts
 *
 * Bootstraps Vuetify and other plugins then mounts the App`
 */

// Components
import App from './App.vue'

// Composables
import { createApp } from 'vue'

// Plugins
import { registerPlugins } from '@/plugins'
import {emitter, useEmitter} from "@/plugins/mitt";
import {useConsoleDialogStore} from "@/stores/ConsoleDialogStore";

const app = createApp(App)

registerPlugins(app)

app.mount('#app')

declare global {
    interface Window {
        $vm: typeof app;
        $bus: typeof emitter;
    }
}

window.$vm = app
window.$bus = useEmitter()
const cds = useConsoleDialogStore()
window.$bus.on('APPEND_CONSOLE_MESSAGE', (msg) => {
    cds.appendConsoleMessage(msg as string)
})

// Allow any module (e.g. tauri.ts) to request showing the console dialog
window.$bus.on('SHOW_CONSOLE_DIALOG', () => {
    cds.showConsoleDialog()
})

// Global JS error handlers: best-effort surface unexpected errors
window.addEventListener('unhandledrejection', (event) => {
    const reason: any = (event as any).reason
    const message = reason?.message ? String(reason.message) : String(reason)
    cds.appendConsoleMessage(`[UNHANDLED_REJECTION] ${message}`)
    cds.showConsoleDialog()
    console.error('Unhandled promise rejection:', reason)
})

window.addEventListener('error', (event) => {
    const anyEvent: any = event as any
    const message = anyEvent?.message ? String(anyEvent.message) : 'Unknown error'
    cds.appendConsoleMessage(`[WINDOW_ERROR] ${message}`)
    cds.showConsoleDialog()
    console.error('Window error:', event)
})

cds.appendConsoleMessage('启动时间: ' + new Date())
