<template>
  <router-view />
</template>

<script lang="ts" setup>
import {onMounted} from "vue";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";

const cds = useConsoleDialogStore()
let pendingWriteSize = false

onMounted(() => {
  if (process.env.NODE_ENV !== 'development') {
    console.log('setupWebsocketConnectivityCheck')
    setupWebsocketConnectivityCheck()
  }
  window.addEventListener('resize', rememberWindowSize);
})

function rememberWindowSize() {
  if (!pendingWriteSize) {
    pendingWriteSize = true
    setTimeout(() => {
      pendingWriteSize = false
      window.eel.update_window_size(window.outerWidth, window.outerHeight)()
    }, 1000)
  }
}

function setupWebsocketConnectivityCheck() {
  setInterval(() => {
    try {
      let ws = window.eel._websocket
      if (ws.readyState === ws.CLOSED || ws.readyState === ws.CLOSING) {
        cds.cleanAndShowConsoleDialog()
        cds.appendConsoleMessage('程序后端连接出错, 请关闭当前页面并重启程序以解决这个问题。')
      }
    } catch (e) {
      console.log(e)
      cds.cleanAndShowConsoleDialog()
      cds.appendConsoleMessage('程序后端连接出错, 请关闭当前页面并重启程序以解决这个问题。')
    }
  }, 5000)
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
