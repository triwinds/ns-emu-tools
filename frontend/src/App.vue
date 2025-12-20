<template>
  <router-view />
</template>

<script lang="ts" setup>
import { onMounted, onUnmounted } from "vue";
import { useConsoleDialogStore } from "@/stores/ConsoleDialogStore";
import { getCurrentWindow } from '@tauri-apps/api/window';
import { updateSetting } from "@/utils/tauri";

const cds = useConsoleDialogStore()
let pendingWriteSize = false
const appWindow = getCurrentWindow()

onMounted(() => {
  window.addEventListener('resize', rememberWindowSize);
})

onUnmounted(() => {
  window.removeEventListener('resize', rememberWindowSize);
})

async function rememberWindowSize() {
  if (!pendingWriteSize) {
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
