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
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";

const app = createApp(App)

registerPlugins(app)

app.mount('#app')

declare global {
    interface Window {
        $vm: typeof app;
        eel: any;
        $bus: typeof emitter;
    }
}

window.$vm = app
window.$bus = useEmitter()
const cds = useConsoleDialogStore()
window.$bus.on('APPEND_CONSOLE_MESSAGE', (msg) => {
    cds.appendConsoleMessage(msg as string)
})
cds.appendConsoleMessage('启动时间: ' + new Date())
