<template>
  <SimplePage>
    <v-card class="mx-auto" style="margin-bottom: 10px">
    <v-container>
      <v-row>
        <v-col>
          <div style="height: 50px">
            <v-img src="@/assets/yuzu.webp" height="40" width="40" class="float-left"
                 style="margin-right: 15px"></v-img>
            <p class="text-h4 text-primary float-left">
              Yuzu 基础信息
            </p>
          </div>
        </v-col>
      </v-row>
      <v-divider style="margin-bottom: 15px"></v-divider>
      <v-row>
        <v-col>
          <span class="text-h6 text-secondary">当前使用的 Yuzu 分支：</span>
          <v-tooltip right>
            <template v-slot:activator="{ props }">
              <v-btn color="error" size="large" variant="outlined" style="margin-right: 15px" v-bind="props"
                     @click="switchYuzuBranch" :disabled='isRunningInstall'>
                {{ displayBranch }} 版
              </v-btn>
            </template>
            <span>切换安装分支</span>
          </v-tooltip>
        </v-col>
      </v-row>
      <v-row>
        <v-col cols="7">
          <v-autocomplete label="Yuzu 路径" v-model="selectedYuzuPath" :items="historyPathList"
                          @update:model-value="updateYuzuPath" variant="underlined"
                          style="cursor: default">
            <template v-slot:item="{props, item}">
              <v-list-item v-bind="props" :title="item.raw">
                <template v-slot:append>
                  <v-btn color="error" size="small" icon variant="outlined" right v-if="selectedYuzuPath !== item.raw"
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
                 :disabled='isRunningInstall' @click="modifyYuzuPath">修改路径
          </v-btn>
          <v-btn size="large" color="success" variant="outlined" min-width="120px" :disabled='isRunningInstall'
                 @click="startYuzu">启动 Yuzu
          </v-btn>
        </v-col>
      </v-row>
      <v-row>
        <v-col>
                  <span class="text-h6 text-secondary">
                    当前 Yuzu 版本：
                  </span>
          <v-tooltip top>
            <template v-slot:activator="{ props }">
              <v-btn color="warning" variant="outlined" style="margin-right: 15px" v-bind="props"
                     @click="detectYuzuVersion" :disabled='isRunningInstall'>
                {{ yuzuConfig.yuzu_version ? yuzuConfig.yuzu_version : '未知' }}
              </v-btn>
            </template>
            <span>点击重新检测 Yuzu 版本</span>
          </v-tooltip>
          <span class="text-h6 text-secondary">
                    最新 Yuzu 版本：
                  </span>
          <span class="text-h6">
                    {{ latestYuzuVersion }}
                  </span>
          <ChangeLogDialog>
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
                {{ yuzuConfig.yuzu_firmware ? yuzuConfig.yuzu_firmware : '未知' }}
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
            <v-img src="@/assets/yuzu.webp" height="40" width="40" class="float-left"
                 style="margin-right: 15px"></v-img>
            <p class="text-h4 text-primary float-left">
              Yuzu 组件管理
            </p>
          </div>
        </v-col>
      </v-row>
      <v-divider style="margin-bottom: 15px"></v-divider>
      <v-row>
        <v-col cols="7">
          <v-text-field hide-details label="需要安装的 Yuzu 版本" v-model="targetYuzuVersion" variant="underlined"></v-text-field>
        </v-col>
        <v-col>
          <v-btn color="info" size="large" variant="outlined" min-width="140px" :disabled='isRunningInstall'
                 @click="installYuzu">
            安装 Yuzu
          </v-btn>
        </v-col>
      </v-row>
      <v-row>
        <v-col cols="7">
          <v-autocomplete hide-details v-model="appStore.targetFirmwareVersion" label="需要安装的固件版本"
                          :items="appStore.availableFirmwareVersions" variant="underlined"></v-autocomplete>
        </v-col>
        <v-col>
          <v-btn color="info" size="large" variant="outlined" min-width="140px" :disabled='isRunningInstall'
                 @click="firmwareInstallationWarning = true">
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
  <v-dialog v-model="firmwareInstallationWarning" max-width="800">
    <v-card>
      <dialog-title>
        安装前必读
      </dialog-title>
      <MarkdownContentBox :content="md"/>


      <v-divider></v-divider>

      <v-card-actions>
        <v-spacer></v-spacer>
        <v-btn
            color="primary"
            variant="text"
            @click="firmwareInstallationWarning = false"
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
import {useConfigStore} from "@/store/ConfigStore";
import {CommonResponse} from "@/types";
import {useAppStore} from "@/store/app";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import {mdiTimelineQuestionOutline, mdiTrashCanOutline} from "@mdi/js";
import showdown from 'showdown'
import SimplePage from "@/components/SimplePage.vue";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import DialogTitle from "@/components/DialogTitle.vue";

let allYuzuReleaseVersions = ref([])
let targetYuzuVersion = ref('')
let isRunningInstall = ref(false)
let historyPathList = ref<string[]>([])
let selectedYuzuPath = ref('')
let changeLogHtml = ref('<p>加载中...</p>')
let firmwareInstallationWarning = ref(false)
const md = ref(`
一般来说，更新固件并不会改善你的游戏体验。只要你的模拟器能够正常识别游戏，并且游戏内的字体显示正常，
那么你就不需要更新固件。其他问题，比如游戏内材质错误、帧率低等问题与固件无关，可以通过更换模拟器版本或者使用 mod 来解决。

需要注意的是，由于yuzu有特殊的存档机制，更新固件或者密钥后存档位置可能会发生改变，因此在更新之前请务必备份你的存档。
`)
let configStore = useConfigStore()
let appStore = useAppStore()
let consoleDialogStore = useConsoleDialogStore()
let yuzuConfig = computed(() => {
  return configStore.config.yuzu
})
let branch = computed(() => {
  return configStore.config.yuzu.branch
})
let displayBranch = computed(() => {
  if (branch.value === 'ea') {
    return 'EA'
  } else if (branch.value === 'mainline') {
    return '主线'
  }
  return '未知'
})
let latestYuzuVersion = computed(() => {
  if (allYuzuReleaseVersions.value.length > 0) {
    return allYuzuReleaseVersions.value[0]
  }
  return "加载中"
})

async function loadHistoryPathList() {
  let data = await window.eel.load_history_path('yuzu')()
  if (data.code === 0) {
    historyPathList.value = data.data
  }
}

onBeforeMount(async () => {
  await loadHistoryPathList()
  await configStore.reloadConfig()
  selectedYuzuPath.value = configStore.config.yuzu.yuzu_path
  updateYuzuReleaseVersions()
  window.eel.update_last_open_emu_page('yuzu')()
})

function updateYuzuReleaseVersions() {
  allYuzuReleaseVersions.value = []
  targetYuzuVersion.value = ""
  window.eel.get_all_yuzu_release_versions()((data: CommonResponse) => {
    if (data.code === 0) {
      let infos = data.data
      allYuzuReleaseVersions.value = infos
      targetYuzuVersion.value = infos[0]
    } else {
      consoleDialogStore.showConsoleDialog()
      consoleDialogStore.appendConsoleMessage('yuzu 版本信息加载异常.')
    }
  })
}

async function switchYuzuBranch() {
  await window.eel.switch_yuzu_branch()()
  await configStore.reloadConfig()
  allYuzuReleaseVersions.value = []
  updateYuzuReleaseVersions()
}

function installFirmware() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  firmwareInstallationWarning.value = false
  consoleDialogStore.persistentConsoleDialog = true
  window.eel.install_yuzu_firmware(appStore.targetFirmwareVersion)((resp: CommonResponse) => {
    isRunningInstall.value = false
    consoleDialogStore.persistentConsoleDialog = false
    if (resp['msg']) {
      consoleDialogStore.appendConsoleMessage(resp.msg)
    }
    configStore.reloadConfig()
  })
}

function installYuzu() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  consoleDialogStore.persistentConsoleDialog = true
  window.eel.install_yuzu(targetYuzuVersion.value, branch.value)((resp: CommonResponse) => {
    isRunningInstall.value = false
    consoleDialogStore.persistentConsoleDialog = false
    if (resp['code'] === 0) {
      configStore.reloadConfig()
      consoleDialogStore.appendConsoleMessage(resp.msg)
    } else {
      consoleDialogStore.appendConsoleMessage(resp.msg)
    }
  });
}

async function detectFirmwareVersion() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  window.eel.detect_firmware_version("yuzu")(() => {
    configStore.reloadConfig()
    consoleDialogStore.appendConsoleMessage('固件版本检测完成')
  })
}
function loadChangeLog() {
  window.eel.get_yuzu_commit_logs()((resp: CommonResponse) => {
    if (resp.code === 0) {
      const converter = new showdown.Converter()
      changeLogHtml.value = converter.makeHtml(resp.data)
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  })
}

async function detectYuzuVersion() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  let previousBranch = branch.value
  let data = await window.eel.detect_yuzu_version()()
  await configStore.reloadConfig()
  if (data['code'] === 0) {
    if (previousBranch !== branch.value) {
      updateYuzuReleaseVersions()
    }
    consoleDialogStore.appendConsoleMessage('Yuzu 版本检测完成')
  } else {
    consoleDialogStore.appendConsoleMessage('检测 yuzu 版本时发生异常')
  }
}

function startYuzu() {
  window.eel.start_yuzu()((data: CommonResponse) => {
    if (data['code'] === 0) {
      consoleDialogStore.appendConsoleMessage('yuzu 启动成功')
    } else {
      consoleDialogStore.appendConsoleMessage('yuzu 启动失败')
    }
  })
}

async function modifyYuzuPath() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  consoleDialogStore.appendConsoleMessage('=============================================')
  consoleDialogStore.appendConsoleMessage('选择的目录将作为存放模拟器的根目录')
  consoleDialogStore.appendConsoleMessage('建议新建目录单独存放')
  consoleDialogStore.appendConsoleMessage('=============================================')
  let data = await window.eel.ask_and_update_yuzu_path()()
  if (data['code'] === 0) {
    let oldBranch = configStore.config.yuzu.branch
    await configStore.reloadConfig()
    if (oldBranch !== configStore.config.yuzu.branch) {
      updateYuzuReleaseVersions()
    }
    await loadHistoryPathList()
    selectedYuzuPath.value = configStore.config.yuzu.yuzu_path
  }
  consoleDialogStore.appendConsoleMessage(data['msg'])
  await loadHistoryPathList()
}
function deleteHistoryPath(targetPath: string) {
  window.eel.delete_history_path('yuzu', targetPath)((resp: CommonResponse) => {
    if (resp.code === 0) {
      loadHistoryPathList()
    }
  })
}

async function updateYuzuPath() {
  await window.eel.update_yuzu_path(selectedYuzuPath.value)()
  let oldBranch = configStore.yuzuConfig.branch
  await configStore.reloadConfig()
  await loadHistoryPathList()
  selectedYuzuPath.value = configStore.yuzuConfig.yuzu_path

  if (oldBranch !== configStore.yuzuConfig.branch) {
    updateYuzuReleaseVersions()
  }
}
</script>

<style scoped>

</style>
