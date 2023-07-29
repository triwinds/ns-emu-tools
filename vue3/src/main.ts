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
import type {useEmitter} from "@/plugins/mitt";

const app = createApp(App)

registerPlugins(app)

app.mount('#app')

declare global {
    interface Window {
        $vm: typeof app;
        eel: any;
        $bus: typeof useEmitter;
    }
}

window.$vm = app
