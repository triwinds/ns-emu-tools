<template>
  <div class="text-center">
    <v-dialog
        v-model="consoleDialogStore.dialogFlag"
        max-width="900"
        :persistent="consoleDialogStore.persistentConsoleDialog"
    >

      <v-card>
        <dialog-title>
          控制台日志
        </dialog-title>

        <div style="padding-left: 10px; padding-right: 10px; padding-top: 10px;" class="flex-grow-0">
          <textarea id="consoleBox" :value="logText" readonly rows="12"></textarea>
        </div>

        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
              color="primary"
              variant="text"
              @click="pauseDownload"
              v-if="consoleDialogStore.persistentConsoleDialog"
          >
            暂停下载任务
          </v-btn>
          <v-btn
              color="primary"
              variant="text"
              @click="stopDownload"
              v-if="consoleDialogStore.persistentConsoleDialog"
          >
            中断并删除下载任务
          </v-btn>
          <v-btn
              color="primary"
              variant="text"
              @click="closeDialog"
              :disabled="consoleDialogStore.persistentConsoleDialog"
          >
            关闭
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script lang="ts" setup>
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import {CommonResponse} from "@/types";
import {computed, nextTick, onUpdated} from "vue";
import DialogTitle from "@/components/DialogTitle.vue";

const consoleDialogStore = useConsoleDialogStore()

function closeDialog() {
  consoleDialogStore.dialogFlag = false
}
function stopDownload() {
  window.eel.stop_download()((resp: CommonResponse) => {
    console.log(resp)
  })
}
function pauseDownload() {
  window.eel.pause_download()((resp: CommonResponse) => {
    console.log(resp)
  })
}

let logText = computed(() => {
  let text = ''
  for (let line of consoleDialogStore.consoleMessages) {
    text += line + '\n'
  }
  return text
})

onUpdated(() => {
  nextTick(() => {
    let consoleBox = document.getElementById("consoleBox")
    if (consoleBox) {
      consoleBox.scrollTop = consoleBox.scrollHeight
    }
  })
})
</script>

<style scoped>
#consoleBox {
  background-color: #000;
  width: 100%;
  color: white;
  overflow-x: scroll;
  overflow-y: scroll;
  resize: none;
  padding: 10px;
  font-family: 'JetBrains Mono Variable',sans-serif !important;
}

#consoleBox::-webkit-resizer, #consoleBox::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

#consoleBox::-webkit-scrollbar {
  width: 5px !important;
  height: 5px !important;
}

#consoleBox::-webkit-scrollbar-corner, #consoleBox ::-webkit-scrollbar-track {
  background: transparent !important;
}

#consoleBox::-webkit-resizer, #consoleBox ::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

#consoleBox::-webkit-scrollbar-corner, #consoleBox ::-webkit-scrollbar-track {
  background: transparent !important;
}
</style>
