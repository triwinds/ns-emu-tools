<template>
  <div class="text-center">
    <v-dialog
        v-model="$store.state.consoleDialogFlag"
        max-width="900"
        :persistent="$store.state.persistentConsoleDialog"
    >

      <v-card>
        <v-card-title class="text-h5 primary white--text">
          控制台日志
        </v-card-title>

        <div style="padding-left: 10px; padding-right: 10px; padding-top: 10px;" class="flex-grow-0">
          <textarea id="consoleBox" :value="logText" readonly rows="15"></textarea>
        </div>

        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
              color="primary"
              text
              @click="pauseDownload"
              v-if="$store.state.persistentConsoleDialog"
          >
            暂停下载任务
          </v-btn>
          <v-btn
              color="primary"
              text
              @click="stopDownload"
              v-if="$store.state.persistentConsoleDialog"
          >
            中断并删除下载任务
          </v-btn>
          <v-btn
              color="primary"
              text
              @click="closeDialog"
              :disabled="$store.state.persistentConsoleDialog"
          >
            关闭
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script>
export default {
  name: 'ConsoleDialog',
  data() {
    return {}
  },
  created() {
    // this.showConsoleDialog()
    // setInterval(() => {
    //   this.appendConsoleMessage("test" + new Date().getTime())
    // }, 300)
  },
  methods: {
    closeDialog() {
      this.$store.commit('SET_CONSOLE_DIALOG_FLAG', false)
    },
    stopDownload() {
      window.eel.stop_download()((resp) => {
        console.log(resp)
      })
    },
    pauseDownload() {
      window.eel.pause_download()((resp) => {
        console.log(resp)
      })
    },
  },
  computed: {
    logText() {
      let text = ''
      for (let line of this.$store.state.consoleMessages) {
        text += line + '\n'
      }
      return text
    }
  },
  updated() {
    this.$nextTick(() => {
      let consoleBox = document.getElementById("consoleBox")
      if (consoleBox) {
        consoleBox.scrollTop = consoleBox.scrollHeight
      }
    })
  }
}
</script>

<style scoped>
#consoleBox {
  background-color: #000;
  width: 100%;
  color: white;
  overflow-x: scroll;
  overflow-y: scroll;
  resize: none;
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