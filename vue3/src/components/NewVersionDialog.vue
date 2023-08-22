<template>
<div class="text-center">
    <v-dialog
      v-model="dialog"
      width="850"
    >
      <v-card>
        <v-card-title class="text-h5 bg-primary text-white">
          {{ configStore.hasNewVersion ? '更新日志' : '版本检测' }}
        </v-card-title>

        <div style="padding: 15px;">
          <p class="text-h6 text--primary" v-show="!configStore.hasNewVersion">当前版本已经是最新版本</p>
          <div v-show="configStore.hasNewVersion" >
<!--            <p class="text-h6 text&#45;&#45;primary">[{{newVersion}}] 更新内容:</p>-->
            <div v-html="releaseDescriptionHtml" class="text--primary"
                 style="max-height: 300px; overflow-y: auto"></div>
          </div>
        </div>

        <v-divider></v-divider>

        <v-card-actions v-show="!configStore.hasNewVersion">
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            variant="text"
            @click="dialog = false"
          >
            OK
          </v-btn>
        </v-card-actions>
        <v-card-actions v-show="configStore.hasNewVersion">
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            variant="text"
            @click="updateNET"
          >
            自动更新
          </v-btn>
          <v-btn
            color="primary"
            variant="text"
            @click="downloadNET"
          >
            下载最新版本
          </v-btn>
          <v-btn
            color="primary"
            variant="text"
            @click="openReleasePage"
          >
            前往发布页
          </v-btn>
          <v-btn
            color="primary"
            variant="text"
            @click="dialog = false"
          >
            取消
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script setup lang="ts">
import {onMounted, ref} from "vue";
import {useConfigStore} from "@/store/ConfigStore";
import {openUrlWithDefaultBrowser} from "@/utils/common";
import showdown from "showdown";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import {CommonResponse} from "@/types";
import {useEmitter} from "@/plugins/mitt";

let dialog = ref(false)
const configStore = useConfigStore()
let newVersion = ref('')
let releaseDescriptionHtml = ref('')
const cds = useConsoleDialogStore()
const emitter = useEmitter()

onMounted(() => {
  emitter.on('showNewVersionDialog', showNewVersionDialog)
})

function showNewVersionDialog(info: any) {
  dialog.value = true
  newVersion.value = info.latestVersion
  if (configStore.hasNewVersion) {
    loadReleaseDescription()
  }
}

function openReleasePage() {
  dialog.value = false
  if (configStore.hasNewVersion) {
    openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
  }
}

function loadReleaseDescription() {
  window.eel.load_change_log()((resp: CommonResponse) => {
    if (resp.code === 0) {
      const converter = new showdown.Converter()
      let rawMd = resp.data.replace('# Change Log\n\n', '')
      releaseDescriptionHtml.value = converter.makeHtml(rawMd)
    } else {
      releaseDescriptionHtml.value = '<p>加载失败</p>'
    }
  })
}

function downloadNET() {
  cds.cleanAndShowConsoleDialog()
  window.eel.download_net_by_tag(newVersion.value)((resp: CommonResponse) => {
    if (resp.code === 0) {
      cds.appendConsoleMessage('NET 下载完成')
    } else {
      cds.appendConsoleMessage(resp.msg)
      cds.appendConsoleMessage('NET 下载失败')
    }
  })
}

async function updateNET() {
  cds.cleanAndShowConsoleDialog()
  window.eel.update_net_by_tag(newVersion.value)
}
</script>

<style scoped>

</style>
