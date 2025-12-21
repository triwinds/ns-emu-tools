<template>
  <router-view />
  <InstallationDialog />
</template>


<script lang="ts" setup>
import { onMounted, onUnmounted } from "vue";
import { useConsoleDialogStore } from "@/stores/ConsoleDialogStore";
import { useInstallationStore } from "@/stores/InstallationStore"; // Import store
import InstallationDialog from "@/components/InstallationDialog.vue"; // Import component
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event'; // Import listen

const cds = useConsoleDialogStore()
const installationStore = useInstallationStore() // Init store
let pendingWriteSize = false
let appWindow: any = null
let unlistenInstallation: any = null; // Store unlisten function

try {
  appWindow = getCurrentWindow()
} catch (e) {
  console.log('Running in browser mode, Tauri APIs unavailable')
}

onMounted(async () => {
  window.addEventListener('resize', rememberWindowSize);

  // Listen for installation events
  try {
      unlistenInstallation = await listen('installation-event', (event: any) => {
          const payload = event.payload;
          console.log('Installation Event:', payload);

          switch (payload.type) {
              case 'started':
                  installationStore.reset();
                  installationStore.setSteps(payload.steps);
                  installationStore.openDialog();
                  break;
              case 'stepUpdate':
                  // New unified event - simply update the step
                  installationStore.updateStep(payload.step);
                  break;
              case 'finished':
                  // Optional: handle overall finished state if needed
                  // For now, steps indicating success is enough
                  break;
          }
      });
  } catch (e) {
      console.error('Failed to setup installation event listener', e);
  }
})

onUnmounted(() => {
  window.removeEventListener('resize', rememberWindowSize);
  if (unlistenInstallation) {
      unlistenInstallation();
  }
})

async function rememberWindowSize() {
  if (!pendingWriteSize && appWindow) {
    pendingWriteSize = true
    setTimeout(async () => {
      pendingWriteSize = false
      try {
        const size = await appWindow.outerSize()
        // Note: Tauri automatically saves window state, but we can also manually update settings if needed
        console.log('Window resized:', size.width, size.height)
      } catch (error) {
        console.error('Failed to get window size:', error)
      }
    }, 1000)
  }
}
</script>

<style>
html ::-webkit-scrollbar {
  width: 0 ;
  height: 0 ;
}
div::-webkit-resizer, div::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar {
  width: 5px !important;
  height: 5px !important;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}

div::-webkit-resizer, div ::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}

a {
  cursor: pointer;
}
</style>
