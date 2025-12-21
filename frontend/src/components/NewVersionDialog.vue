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
import {useConfigStore} from "@/stores/ConfigStore";
import {openUrlWithDefaultBrowser} from "@/utils/common";
import md from "@/utils/markdown";
import {useConsoleDialogStore} from "@/stores/ConsoleDialogStore";
import type {CommonResponse} from "@/types";
import {useEmitter} from "@/plugins/mitt";
import {loadChangeLog, downloadAppUpdate, installAppUpdate, updateSelfByTag} from "@/utils/tauri";

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

async function loadReleaseDescription() {
  try {
    const changelog = await loadChangeLog()
    let rawMd = (changelog || '').replace('# Change Log\n\n', '')
    releaseDescriptionHtml.value = md.parse(rawMd)
  } catch (error) {
    console.error('加载变更日志失败:', error)
    releaseDescriptionHtml.value = '<p>加载失败</p>'
  }
}

async function downloadNET() {
  try {
    console.log('[NewVersionDialog] 开始下载更新')
    console.log('[NewVersionDialog] configStore.updateInfo:', configStore.updateInfo)
    const downloadUrl = configStore.updateInfo?.downloadUrl
    console.log('[NewVersionDialog] downloadUrl:', downloadUrl)

    if (!downloadUrl) {
      console.warn('[NewVersionDialog] downloadUrl 为空，将由后端重新检查更新')
    }

    console.log('[NewVersionDialog] 调用 downloadAppUpdate, includePrerelease=false, downloadUrl=', downloadUrl)
    const updateFilePath = await downloadAppUpdate(false, downloadUrl)
    console.log('[NewVersionDialog] 更新文件已下载:', updateFilePath)
    // 下载完成后可以提示用户安装
  } catch (error) {
    console.error('[NewVersionDialog] 下载更新失败:', error)
  }
}

async function updateNET() {
  try {
    console.log('[NewVersionDialog] 开始自动更新')
    console.log('[NewVersionDialog] configStore.updateInfo:', configStore.updateInfo)
    const latestVersion = configStore.updateInfo?.latestVersion
    console.log('[NewVersionDialog] latestVersion:', latestVersion)

    if (!latestVersion) {
      console.error('[NewVersionDialog] 无法获取版本号')
      return
    }

    console.log('[NewVersionDialog] 调用 updateSelfByTag, version=', latestVersion)
    await updateSelfByTag(latestVersion)
    console.log('[NewVersionDialog] 更新命令已发送，程序将自动退出并更新')
    // 程序会自动退出并更新
  } catch (error) {
    console.error('[NewVersionDialog] 更新失败:', error)
  }
}
</script>

<style scoped>

</style>
