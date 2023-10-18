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
                            @update:model-value="updateRyujinxPath"
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
            <ChangeLogDialog v-if="selectedBranch === 'ava' || selectedBranch === 'mainline'">
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
            <v-btn color="info" size="large" variant="outlined" min-width="160px" :disabled='isRunningInstall'
                   @click="installRyujinx">
              安装 Ryujinx
            </v-btn>
          </v-col>
        </v-row>
        <v-row>
          <v-col cols="7">
            <v-autocomplete hide-details v-model="appStore.targetFirmwareVersion" variant="underlined" label="需要安装的固件版本"
                            :items="appStore.availableFirmwareVersions"></v-autocomplete>
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
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import {useConfigStore} from "@/store/ConfigStore";
import {CommonResponse} from "@/types";
import {useAppStore} from "@/store/app";
import showdown from "showdown";
import {mdiTimelineQuestionOutline, mdiTrashCanOutline} from "@mdi/js";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import SimplePage from "@/components/SimplePage.vue";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import DialogTitle from "@/components/DialogTitle.vue";

let allRyujinxReleaseInfos = ref([])
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
    text: '正式版 (老 UI)',
    value: 'mainline'
  }, {
    text: 'AVA 版 (新 UI)',
    value: 'ava'
  }, {
    text: 'LDN 版 (联机版本)',
    value: 'ldn'
  },
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
  window.eel.update_last_open_emu_page('ryujinx')()
})

function updateRyujinxReleaseInfos() {
  allRyujinxReleaseInfos.value = []
  targetRyujinxVersion.value = ""
  window.eel.get_ryujinx_release_infos()((data: CommonResponse) => {
    if (data['code'] === 0) {
      let infos = data['data']
      allRyujinxReleaseInfos.value = infos
      targetRyujinxVersion.value = infos[0]['tag_name']
    } else {
      cds.appendConsoleMessage('ryujinx 版本信息加载异常.')
    }
  })
}

async function loadHistoryPathList() {
  let data = await window.eel.load_history_path('ryujinx')()
  if (data.code === 0) {
    historyPathList.value = data.data
  }
}

async function updateRyujinxPath() {
  await window.eel.update_ryujinx_path(selectedRyujinxPath.value)()
  let oldBranch = configStore.config.ryujinx.branch
  await configStore.reloadConfig()
  selectedRyujinxPath.value = configStore.config.ryujinx.path
  await loadHistoryPathList()
  if (oldBranch !== configStore.config.ryujinx.branch) {
    updateRyujinxReleaseInfos()
  }
}

function deleteHistoryPath(targetPath: string) {
  window.eel.delete_history_path('ryujinx', targetPath)((resp: CommonResponse) => {
    if (resp.code === 0) {
      loadHistoryPathList()
    }
  })
}

function detectRyujinxVersion() {
  cds.cleanAndShowConsoleDialog()
  window.eel.detect_ryujinx_version()((data: CommonResponse) => {
    if (data['code'] === 0) {
      configStore.reloadConfig()
      updateRyujinxReleaseInfos()
      cds.appendConsoleMessage('Ryujinx 版本检测完成')
    } else {
      cds.appendConsoleMessage('检测 Ryujinx 版本时发生异常')
    }
  })
}

function installRyujinx() {
  cds.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  cds.persistentConsoleDialog = true
  window.eel.install_ryujinx(targetRyujinxVersion.value, selectedBranch.value)((resp: CommonResponse) => {
    isRunningInstall.value = false
    cds.persistentConsoleDialog = false
    cds.appendConsoleMessage(resp['msg'])
    if (resp['code'] === 0) {
      configStore.reloadConfig()
    }
  });
}

function installFirmware() {
  cds.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  firmwareInstallationWarningDialog.value = false
  cds.persistentConsoleDialog = true
  window.eel.install_ryujinx_firmware(appStore.targetFirmwareVersion)((resp: CommonResponse) => {
    isRunningInstall.value = false
    cds.persistentConsoleDialog = false
    cds.appendConsoleMessage(resp['msg'])
    if (resp['code'] === 0) {
      configStore.reloadConfig()
    }
  })
}

async function askAndUpdateRyujinxPath() {
  cds.cleanAndShowConsoleDialog()
  cds.appendConsoleMessage('=============================================')
  cds.appendConsoleMessage('选择的目录将作为存放模拟器的根目录')
  cds.appendConsoleMessage('建议新建目录单独存放')
  cds.appendConsoleMessage('=============================================')
  let data = await window.eel.ask_and_update_ryujinx_path()();
  if (data['code'] === 0) {
    let oldBranch = configStore.config.ryujinx.branch
    await configStore.reloadConfig()
    if (oldBranch !== configStore.config.ryujinx.branch) {
      updateRyujinxReleaseInfos()
    }
    await loadHistoryPathList()
    selectedRyujinxPath.value = configStore.config.ryujinx.path
  }
  cds.appendConsoleMessage(data['msg'])
}

function startRyujinx() {
  window.eel.start_ryujinx()((data: CommonResponse) => {
    if (data['code'] === 0) {
      cds.appendConsoleMessage('Ryujinx 启动成功')
    } else {
      cds.appendConsoleMessage('Ryujinx 启动失败')
    }
  })
}

async function detectFirmwareVersion() {
  cds.cleanAndShowConsoleDialog()
  window.eel.detect_firmware_version("ryujinx")(() => {
    configStore.reloadConfig()
  })
}

async function switchRyujinxBranch() {
  await window.eel.switch_ryujinx_branch(selectedBranch.value)()
  await configStore.reloadConfig()
  await updateRyujinxReleaseInfos()
  // console.log(this.selectedBranch)
}

function loadChangeLog() {
  window.eel.load_ryujinx_change_log()((resp: CommonResponse) => {
    if (resp.code === 0) {
      const converter = new showdown.Converter()
      changeLogHtml.value = converter.makeHtml(resp.data)
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  })
}

</script>

<style scoped>

</style>
