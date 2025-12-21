<template>
  <SimplePage>
    <v-card class="mx-auto" style="margin-bottom: 10px">
      <v-container>
        <v-row>
          <v-col>
            <div style="height: 50px">
              <v-img src="@/assets/ryujinx.webp" height="40" width="40" class="float-left"
                   style="margin-right: 15px"></v-img>
              <p class="text-h4 text-primary float-left">
                Ryujinx 基础信息
              </p>
            </div>
          </v-col>
        </v-row>
        <v-divider style="margin-bottom: 15px"></v-divider>
        <v-row>
          <v-col>
            <v-select variant="outlined" v-model="selectedBranch" :items="availableBranch" hide-details
                      @update:model-value="switchRyujinxBranch" color="error" item-color="error"
                      item-title="text" item-value="value"
                      label="当前使用的 Ryujinx 分支"></v-select>
          </v-col>
        </v-row>
        <v-row>
          <v-col cols="7">
            <v-autocomplete label="Ryujinx 路径" v-model="selectedRyujinxPath" :items="historyPathList"
                            @update:model-value="updateRyujinxPathFunc"
                            style="cursor: default" variant="underlined">
              <template v-slot:item="{props, item}">
                <v-list-item v-bind="props" :title="item.raw">
                  <template v-slot:append>
                    <v-btn color="error" size="small" icon variant="outlined" right
                           v-if="selectedRyujinxPath !== item.raw"
                           @click.stop="deleteHistoryPath(item.raw)">
                      <v-icon size="small" :icon="mdiTrashCanOutline"></v-icon>
                    </v-btn>
                  </template>
                </v-list-item>

              </template>
            </v-autocomplete>
          </v-col>
          <v-col cols="5">
            <v-btn size="large" color="secondary" variant="outlined" style="margin-right: 5px" min-width="120px"
                   :disabled='isRunningInstall' @click="askAndUpdateRyujinxPath">修改路径
            </v-btn>
            <v-btn size="large" color="success" variant="outlined" min-width="120px" :disabled='isRunningInstall'
                   @click="startRyujinx">启动龙神
            </v-btn>
          </v-col>
        </v-row>
        <v-row>
          <v-col>
                  <span class="text-h6 text-secondary">
                    当前 Ryujinx 版本：
                  </span>
            <v-tooltip top>
              <template v-slot:activator="{ props }">
                <v-btn color="warning" variant="outlined" style="margin-right: 15px" v-bind="props"
                       @click="detectRyujinxVersion" :disabled='isRunningInstall'>
                  {{ configStore.config.ryujinx.version ? configStore.config.ryujinx.version : "未知" }}
                </v-btn>
              </template>
              <span>点击重新检测 Ryujinx 版本</span>
            </v-tooltip>
            <span class="text-h6 text-secondary">
                    最新 Ryujinx 版本：
                  </span>
            <span class="text-h6">
                    {{ latestRyujinxVersion }}
                  </span>
            <ChangeLogDialog v-if="selectedBranch === 'canary' || selectedBranch === 'mainline'">
              <template v-slot:activator="{ props }">
                <span v-bind="props" @click="loadChangeLog"
                      style="margin-left: 10px">
                  <v-icon color="warning" :icon="mdiTimelineQuestionOutline"></v-icon>
                </span>
              </template>
              <template v-slot:content>
                <div class="text--primary" v-html="changeLogHtml"></div>
              </template>
            </ChangeLogDialog>
          </v-col>
        </v-row>
        <v-row>
          <v-col>
            <span class="text-h6 text-secondary">当前固件版本：</span>
            <v-tooltip top>
              <template v-slot:activator="{ props }">
                <v-btn color="warning" variant="outlined" v-bind="props"
                       @click="detectFirmwareVersion" :disabled='isRunningInstall'>
                  {{ configStore.config.ryujinx.firmware ? configStore.config.ryujinx.firmware : "未知" }}
                </v-btn>
              </template>
              <span>点击重新检测固件版本, 需安装密钥后使用</span>
            </v-tooltip>
            <span class="text-h7 text-secondary">
              （如果固件能用就没必要更新）
            </span>
          </v-col>
        </v-row>
      </v-container>
    </v-card>
    <v-card class="mx-auto">
      <v-container>
        <v-row>
          <v-col>
            <div style="height: 50px">
              <v-img src="@/assets/ryujinx.webp" height="40" width="40" class="float-left"
                   style="margin-right: 15px"></v-img>
              <p class="text-h4 text-primary float-left">
                Ryujinx 组件管理
              </p>
            </div>
          </v-col>
        </v-row>
        <v-divider style="margin-bottom: 15px"></v-divider>
        <v-row>
          <v-col cols="7">
            <v-text-field hide-details label="需要安装的 Ryujinx 版本" variant="underlined" v-model="targetRyujinxVersion"></v-text-field>
          </v-col>
          <v-col>
            <v-btn color="info" size="large" variant="outlined" min-width="160px"
                   @click="installRyujinx">
              安装 Ryujinx
            </v-btn>
          </v-col>
        </v-row>
        <v-row>
          <v-col cols="7">
            <v-autocomplete hide-details v-model="appStore.targetFirmwareVersion" label="需要安装的固件版本"
                          item-title="name" item-value="version"
                          :items="appStore.availableFirmwareInfos" variant="underlined"></v-autocomplete>
          </v-col>
          <v-col>
            <v-btn color="info" size="large" variant="outlined" min-width="160px" :disabled='isRunningInstall'
                   @click="firmwareInstallationWarningDialog = true" >
              安装固件
            </v-btn>
          </v-col>
        </v-row>
        <v-row>
          <v-col>
                  <span>安装/更新固件后, 请一并安装相应的 keys:
                    <router-link to="/keys" class="info--text">密钥管理</router-link>
                  </span>
          </v-col>
        </v-row>
      </v-container>
    </v-card>
    <v-dialog v-model="firmwareInstallationWarningDialog" max-width="800">
      <v-card>
        <dialog-title>
          安装前必读
        </dialog-title>
        <MarkdownContentBox :content="firmwareWarningMsg"/>

        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
              color="primary"
              variant="text"
              @click="firmwareInstallationWarningDialog = false"
          >
            取消安装
          </v-btn>
          <v-btn
              color="primary"
              variant="text"
              @click="installFirmware"
          >
            安装固件
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </SimplePage>
</template>

<script setup lang="ts">
import {computed, onBeforeMount, ref} from "vue";
import {useConsoleDialogStore} from "@/stores/ConsoleDialogStore";
import {useConfigStore} from "@/stores/ConfigStore";
import type {CommonResponse} from "@/types";
import {useAppStore} from "@/stores/app";
import markdown from "@/utils/markdown";
import {mdiTimelineQuestionOutline, mdiTrashCanOutline} from "@mdi/js";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import SimplePage from "@/components/SimplePage.vue";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import DialogTitle from "@/components/DialogTitle.vue";
import {
  updateLastOpenEmuPage,
  getAllRyujinxVersions,
  loadHistoryPath,
  updateRyujinxPath,
  deleteHistoryPath as deleteHistoryPathApi,
  detectRyujinxVersion as detectRyujinxVersionApi,
  installRyujinx as installRyujinxApi,
  installFirmwareToRyujinx,
  askAndUpdateRyujinxPath as askAndUpdateRyujinxPathApi,
  startRyujinx as startRyujinxApi,
  detectFirmwareVersion as detectFirmwareVersionApi,
  getRyujinxChangeLogs
} from "@/utils/tauri";

let allRyujinxReleaseInfos = ref<{tag_name: string}[]>([])
let historyPathList = ref<string[]>([])
let selectedRyujinxPath = ref('')
let targetRyujinxVersion = ref('')
let isRunningInstall = ref(false)
let changeLogHtml = ref('<p>加载中...</p>')
let firmwareWarningMsg = ref(`一般来说，更新固件并不会改善你的游戏体验。只要你的模拟器能够正常识别游戏，并且游戏内的字体显示正常，
那么你就不需要更新固件。其他问题，比如游戏内材质错误、帧率低等问题与固件无关，可以通过更换模拟器版本或者使用 mod 来解决。
`)
let firmwareInstallationWarningDialog = ref(false)
let availableBranch = ref([
  {
    text: 'Ryubing/Ryujinx 正式版',
    value: 'mainline'
  }, {
    text: 'Ryubing/Ryujinx Canary 版',
    value: 'canary'
  }
])
let selectedBranch = ref('')
const cds = useConsoleDialogStore()
const configStore = useConfigStore()
const appStore = useAppStore()
let latestRyujinxVersion = computed(() => {
  if (allRyujinxReleaseInfos.value.length > 0) {
    return allRyujinxReleaseInfos.value[0]['tag_name']
  }
  return "加载中"
})

onBeforeMount(async () => {
  await configStore.reloadConfig()
  await loadHistoryPathList()
  appStore.updateAvailableFirmwareInfos()
  selectedRyujinxPath.value = configStore.config.ryujinx.path
  selectedBranch.value = configStore.config.ryujinx.branch
  updateRyujinxReleaseInfos()
  updateLastOpenEmuPage('ryujinx')
})

async function updateRyujinxReleaseInfos() {
  allRyujinxReleaseInfos.value = []
  targetRyujinxVersion.value = ""
  try {
    const data = await getAllRyujinxVersions(selectedBranch.value)
    if (data.code === 0) {
      const infos = data.data || []
      allRyujinxReleaseInfos.value = infos.map(v => ({ tag_name: v }))
      targetRyujinxVersion.value = infos[0] ?? ''
    } else {
      cds.appendConsoleMessage('ryujinx 版本信息加载异常.')
    }
  } catch (error) {
    cds.appendConsoleMessage('ryujinx 版本信息加载异常: ' + error)
    console.error('获取 Ryujinx 版本信息失败:', error)
  }
}

async function loadHistoryPathList() {
  try {
    const paths = await loadHistoryPath('ryujinx')
    historyPathList.value = paths
  } catch (error) {
    console.error('加载历史路径失败:', error)
  }
}

async function updateRyujinxPathFunc() {
  try {
    await updateRyujinxPath(selectedRyujinxPath.value)
    const oldBranch = configStore.config.ryujinx.branch
    await configStore.reloadConfig()
    selectedRyujinxPath.value = configStore.config.ryujinx.path
    selectedBranch.value = configStore.config.ryujinx.branch
    await loadHistoryPathList()
    if (oldBranch !== configStore.config.ryujinx.branch) {
      updateRyujinxReleaseInfos()
    }
  } catch (error) {
    console.error('更新 Ryujinx 路径失败:', error)
    cds.appendConsoleMessage('更新路径失败: ' + error)
  }
}

async function deleteHistoryPath(targetPath: string) {
  try {
    await deleteHistoryPathApi('ryujinx', targetPath)
    await loadHistoryPathList()
  } catch (error) {
    console.error('删除历史路径失败:', error)
  }
}

async function detectRyujinxVersion() {
  cds.cleanAndShowConsoleDialog()
  try {
    const data = await detectRyujinxVersionApi()
    if (data.code === 0) {
      await configStore.reloadConfig()
      selectedBranch.value = configStore.config.ryujinx.branch
      updateRyujinxReleaseInfos()
      cds.appendConsoleMessage('Ryujinx 版本检测完成')
    } else {
      cds.appendConsoleMessage('检测 Ryujinx 版本时发生异常')
    }
  } catch (error) {
    console.error('检测 Ryujinx 版本失败:', error)
    cds.appendConsoleMessage('检测 Ryujinx 版本时发生异常: ' + error)
  }
}

async function installRyujinx() {
  isRunningInstall.value = true
  try {
    const resp = await installRyujinxApi(targetRyujinxVersion.value, selectedBranch.value)
    isRunningInstall.value = false
    cds.appendConsoleMessage(resp.msg || '安装完成')
    if (resp.code === 0) {
      configStore.reloadConfig()
    }
  } catch (error) {
    isRunningInstall.value = false
    // 错误消息已经通过 notify_message 事件发送，不需要在这里重复显示
    console.error('安装 Ryujinx 失败:', error)
  }
}

async function installFirmware() {
  isRunningInstall.value = true
  firmwareInstallationWarningDialog.value = false

  try {
    const resp = await installFirmwareToRyujinx(appStore.targetFirmwareVersion)
    if (resp.code === 0) {
      configStore.reloadConfig()
    }
  } catch (error) {
    console.error('安装固件失败:', error)
  } finally {
    isRunningInstall.value = false
  }
}

async function askAndUpdateRyujinxPath() {
  cds.cleanAndShowConsoleDialog()
  cds.appendConsoleMessage('=============================================')
  cds.appendConsoleMessage('选择的目录将作为存放模拟器的根目录')
  cds.appendConsoleMessage('建议新建目录单独存放')
  cds.appendConsoleMessage('=============================================')
  try {
    const data = await askAndUpdateRyujinxPathApi()
    if (data.code === 0) {
      const oldBranch = configStore.config.ryujinx.branch
      await configStore.reloadConfig()
      if (oldBranch !== configStore.config.ryujinx.branch) {
        selectedBranch.value = configStore.config.ryujinx.branch
        updateRyujinxReleaseInfos()
      }
      await loadHistoryPathList()
      selectedRyujinxPath.value = configStore.config.ryujinx.path
      cds.appendConsoleMessage(data.msg || '路径更新成功')
    }
  } catch (error) {
    console.error('更新 Ryujinx 路径失败:', error)
    cds.appendConsoleMessage('操作取消或失败: ' + error)
  }
}

async function startRyujinx() {
  try {
    const data = await startRyujinxApi()
    if (data.code === 0) {
      cds.appendConsoleMessage('Ryujinx 启动成功')
    } else {
      cds.appendConsoleMessage('Ryujinx 启动失败: ' + (data.msg || ''))
    }
  } catch (error) {
    console.error('启动 Ryujinx 失败:', error)
    cds.appendConsoleMessage('Ryujinx 启动失败: ' + error)
  }
}

async function detectFirmwareVersion() {
  cds.cleanAndShowConsoleDialog()
  try {
    await detectFirmwareVersionApi('ryujinx')
    await configStore.reloadConfig()
    cds.appendConsoleMessage('固件版本检测完成')
  } catch (error) {
    console.error('检测固件版本失败:', error)
    cds.appendConsoleMessage('检测固件版本失败: ' + error)
  }
}

async function switchRyujinxBranch() {
  try {
    // Note: switch_ryujinx_branch API not yet implemented, using update_ryujinx_path instead
    await configStore.reloadConfig()
    await updateRyujinxReleaseInfos()
  } catch (error) {
    console.error('切换分支失败:', error)
    cds.appendConsoleMessage('切换分支失败: ' + error)
  }
}

async function loadChangeLog() {
  try {
    const resp = await getRyujinxChangeLogs(selectedBranch.value)
    if (resp.code === 0) {
      changeLogHtml.value = markdown.parse(resp.data || '')
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  } catch (error) {
    console.error('加载变更日志失败:', error)
    changeLogHtml.value = '<p>加载失败。</p>'
  }
}

</script>

<style scoped>

</style>
