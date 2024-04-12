<template>
  <SimplePage>
    <v-card class="mx-auto" style="margin-bottom: 10px">
    <v-container>
      <v-row>
        <v-col>
          <div style="height: 50px">
            <v-img src="@/assets/suyu.png" height="40" width="40" class="float-left"
                 style="margin-right: 15px"></v-img>
            <p class="text-h4 text-primary float-left">
              Suyu 基础信息
            </p>
          </div>
        </v-col>
      </v-row>
      <v-divider style="margin-bottom: 15px"></v-divider>
<!--      <v-row>-->
<!--        <v-col>-->
<!--          <span class="text-h6 text-secondary">当前使用的 suyu 分支：</span>-->
<!--          <v-btn color="error" size="large" variant="outlined" style="margin-right: 15px" :disabled="true">-->
<!--            {{ displayBranch }} 版-->
<!--          </v-btn>-->
<!--        </v-col>-->
<!--      </v-row>-->
      <v-row>
        <v-col cols="7">
          <v-autocomplete label="suyu 路径" v-model="selectedSuyuPath" :items="historyPathList"
                          @update:model-value="updateSuyuPath" variant="underlined"
                          style="cursor: default">
            <template v-slot:item="{props, item}">
              <v-list-item v-bind="props" :title="item.raw">
                <template v-slot:append>
                  <v-btn color="error" size="small" icon variant="outlined" right v-if="selectedSuyuPath !== item.raw"
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
                 :disabled='isRunningInstall' @click="modifySuyuPath">修改路径
          </v-btn>
          <v-btn size="large" color="success" variant="outlined" min-width="120px" :disabled='isRunningInstall'
                 @click="startSuyu">启动 suyu
          </v-btn>
        </v-col>
      </v-row>
      <v-row>
        <v-col>
          <span class="text-h6 text-secondary">
            当前 suyu 版本：
          </span>
          <span class="text-h6 text-warning">{{ suyuConfig.version ? suyuConfig.version : '未知' }}</span>
<!--          <v-tooltip top>-->
<!--            <template v-slot:activator="{ props }">-->
<!--              <v-btn color="warning" variant="outlined" style="margin-right: 15px" v-bind="props"-->
<!--                     @click="detectSuyuVersion" :disabled='true'>-->
<!--                {{ suyuConfig.version ? suyuConfig.version : '未知' }}-->
<!--              </v-btn>-->
<!--            </template>-->
<!--            <span>点击重新检测 suyu 版本</span>-->
<!--          </v-tooltip>-->
          <span class="text-h6 text-secondary">
                    最新 suyu 版本：
                  </span>
          <span class="text-h6">
                    {{ latestSuyuVersion }}
                  </span>
        </v-col>
      </v-row>
      <v-row>
        <v-col>
          <span class="text-h6 text-secondary">当前固件版本：</span>
          <v-tooltip top>
            <template v-slot:activator="{ props }">
              <v-btn color="warning" variant="outlined" v-bind="props"
                     @click="detectFirmwareVersion" :disabled='isRunningInstall'>
                {{ suyuConfig.firmware ? suyuConfig.firmware : '未知' }}
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
            <v-img src="@/assets/suyu.png" height="40" width="40" class="float-left"
                 style="margin-right: 15px"></v-img>
            <p class="text-h4 text-primary float-left">
              suyu 组件管理
            </p>
          </div>
        </v-col>
      </v-row>
      <v-divider style="margin-bottom: 15px"></v-divider>
      <v-row>
        <v-col cols="7">
          <v-text-field hide-details label="需要安装的 suyu 版本" v-model="targetsuyuVersion" variant="underlined"></v-text-field>
        </v-col>
        <v-col>
          <v-btn color="info" size="large" variant="outlined" min-width="140px" :disabled='isRunningInstall'
                 @click="installSuyu">
            安装 suyu
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

let allsuyuReleaseVersions = ref([])
let targetsuyuVersion = ref('')
let isRunningInstall = ref(false)
let historyPathList = ref<string[]>([])
let selectedSuyuPath = ref('')
let changeLogHtml = ref('<p>加载中...</p>')
let firmwareInstallationWarning = ref(false)
const md = ref(`
一般来说，更新固件并不会改善你的游戏体验。只要你的模拟器能够正常识别游戏，并且游戏内的字体显示正常，
那么你就不需要更新固件。其他问题，比如游戏内材质错误、帧率低等问题与固件无关，可以通过更换模拟器版本或者使用 mod 来解决。

需要注意的是，由于suyu有特殊的存档机制，更新固件或者密钥后存档位置可能会发生改变，因此在更新之前请务必备份你的存档。
`)
let configStore = useConfigStore()
let appStore = useAppStore()
let consoleDialogStore = useConsoleDialogStore()
let suyuConfig = computed(() => {
  return configStore.config.suyu
})
let branch = computed(() => {
  return configStore.config.suyu.branch
})
let latestSuyuVersion = computed(() => {
  if (allsuyuReleaseVersions.value.length > 0) {
    return allsuyuReleaseVersions.value[0]
  }
  return "加载中"
})

async function loadHistoryPathList() {
  let data = await window.eel.load_history_path('suyu')()
  if (data.code === 0) {
    historyPathList.value = data.data
  }
}

onBeforeMount(async () => {
  await loadHistoryPathList()
  await configStore.reloadConfig()
  appStore.updateAvailableFirmwareInfos()
  selectedSuyuPath.value = configStore.config.suyu.path
  updateSuyuReleaseVersions()
  window.eel.update_last_open_emu_page('suyu')()
})

function updateSuyuReleaseVersions() {
  allsuyuReleaseVersions.value = []
  targetsuyuVersion.value = ""
  window.eel.get_all_suyu_release_versions()((data: CommonResponse) => {
    if (data.code === 0) {
      let infos = data.data
      allsuyuReleaseVersions.value = infos
      targetsuyuVersion.value = infos[0]
    } else {
      consoleDialogStore.showConsoleDialog()
      consoleDialogStore.appendConsoleMessage('suyu 版本信息加载异常.')
    }
  })
}

function installFirmware() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  firmwareInstallationWarning.value = false
  consoleDialogStore.persistentConsoleDialog = true
  window.eel.install_suyu_firmware(appStore.targetFirmwareVersion)((resp: CommonResponse) => {
    isRunningInstall.value = false
    consoleDialogStore.persistentConsoleDialog = false
    if (resp['msg']) {
      consoleDialogStore.appendConsoleMessage(resp.msg)
    }
    configStore.reloadConfig()
  })
}

function installSuyu() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  isRunningInstall.value = true
  consoleDialogStore.persistentConsoleDialog = true
  window.eel.install_suyu(targetsuyuVersion.value, branch.value)((resp: CommonResponse) => {
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
  window.eel.detect_firmware_version("suyu")(() => {
    configStore.reloadConfig()
    consoleDialogStore.appendConsoleMessage('固件版本检测完成')
  })
}
function loadChangeLog() {
  window.eel.get_suyu_commit_logs()((resp: CommonResponse) => {
    if (resp.code === 0) {
      const converter = new showdown.Converter()
      changeLogHtml.value = converter.makeHtml(resp.data)
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  })
}

async function detectSuyuVersion() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  let previousBranch = branch.value
  let data = await window.eel.detect_suyu_version()()
  await configStore.reloadConfig()
  if (data['code'] === 0) {
    if (previousBranch !== branch.value) {
      updateSuyuReleaseVersions()
    }
    consoleDialogStore.appendConsoleMessage('suyu 版本检测完成')
  } else {
    consoleDialogStore.appendConsoleMessage('检测 suyu 版本时发生异常')
  }
}

function startSuyu() {
  window.eel.start_suyu()((data: CommonResponse) => {
    if (data['code'] === 0) {
      consoleDialogStore.appendConsoleMessage('suyu 启动成功')
    } else {
      consoleDialogStore.appendConsoleMessage('suyu 启动失败')
    }
  })
}

async function modifySuyuPath() {
  consoleDialogStore.cleanAndShowConsoleDialog()
  consoleDialogStore.appendConsoleMessage('=============================================')
  consoleDialogStore.appendConsoleMessage('选择的目录将作为存放 suyu 模拟器的根目录')
  consoleDialogStore.appendConsoleMessage('***** 安装时会清除该目录下的文件 *****')
  consoleDialogStore.appendConsoleMessage('***** 安装时会清除该目录下的文件 *****')
  consoleDialogStore.appendConsoleMessage('***** 安装时会清除该目录下的文件 *****')
  consoleDialogStore.appendConsoleMessage('建议新建目录单独存放')
  consoleDialogStore.appendConsoleMessage('建议新建目录单独存放')
  consoleDialogStore.appendConsoleMessage('建议新建目录单独存放')
  consoleDialogStore.appendConsoleMessage('=============================================')
  let data = await window.eel.ask_and_update_suyu_path()()
  if (data['code'] === 0) {
    let oldBranch = configStore.config.suyu.branch
    await configStore.reloadConfig()
    if (oldBranch !== configStore.config.suyu.branch) {
      updateSuyuReleaseVersions()
    }
    await loadHistoryPathList()
    selectedSuyuPath.value = configStore.config.suyu.path
  }
  consoleDialogStore.appendConsoleMessage(data['msg'])
  await loadHistoryPathList()
}
function deleteHistoryPath(targetPath: string) {
  window.eel.delete_history_path('suyu', targetPath)((resp: CommonResponse) => {
    if (resp.code === 0) {
      loadHistoryPathList()
    }
  })
}

async function updateSuyuPath() {
  await window.eel.update_suyu_path(selectedSuyuPath.value)()
  let oldBranch = configStore.config.suyu.branch
  await configStore.reloadConfig()
  await loadHistoryPathList()
  selectedSuyuPath.value = configStore.config.suyu.path

  if (oldBranch !== configStore.config.suyu.branch) {
    updateSuyuReleaseVersions()
  }
}
</script>

<style scoped>

</style>
