<template>
  <SimplePage>
    <div class="emulator-page">
      <header class="page-header">
        <div>
          <h1 class="page-title text-primary">模拟器管理</h1>
          <p class="section-description">管理 {{ selectedEmulatorName }} 模拟器的安装、版本和运行</p>
        </div>
        <v-select
          v-model="selectedBranch"
          :items="availableBranch"
          item-title="text"
          item-value="value"
          label="模拟器分支"
          variant="outlined"
          density="compact"
          color="primary"
          hide-details
          class="branch-select"
          :disabled="isRunningInstall"
          @update:model-value="switchRyujinxBranch"
        >
          <template #selection>
            <span>{{ selectedBranch === 'canary' ? 'Ryujinx Canary 版' : 'Ryujinx 正式版' }}</span>
          </template>
        </v-select>
      </header>

      <v-card class="management-card" variant="elevated">
        <div class="emulator-identity">
          <div class="identity-title">
            <v-img src="@/assets/ryujinx.webp" width="48" height="48" class="emulator-logo" />
            <div>
              <h2 class="emulator-name">{{ selectedEmulatorName }}</h2>
              <p class="section-description">Nintendo Switch 模拟器</p>
            </div>
          </div>
          <v-btn
            color="primary"
            variant="flat"
            size="large"
            :prepend-icon="mdiPlay"
            :disabled="isRunningInstall"
            class="launch-button"
            @click="startRyujinx"
          >启动模拟器</v-btn>
        </div>

        <v-divider class="card-divider" />

        <div class="path-row">
          <h3 class="row-title">安装文件夹</h3>
          <v-autocomplete
            v-model="selectedRyujinxPath"
            :items="historyPathList"
            label="安装目录"
            variant="outlined"
            density="compact"
            color="primary"
            hide-details
            :disabled="isRunningInstall"
            class="path-input"
            @update:model-value="updateRyujinxPathFunc"
          >
            <template #item="{ props, item }">
              <v-list-item v-bind="props" :title="item.raw">
                <template #append>
                  <v-btn
                    v-if="selectedRyujinxPath !== item.raw"
                    color="error"
                    size="small"
                    icon
                    variant="text"
                    :aria-label="`删除历史路径 ${item.raw}`"
                    @click.stop="deleteHistoryPath(item.raw)"
                  >
                    <v-icon size="small" :icon="mdiTrashCanOutline" />
                  </v-btn>
                </template>
              </v-list-item>
            </template>
          </v-autocomplete>
          <v-btn color="secondary" variant="outlined" :disabled="isRunningInstall" @click="askAndUpdateRyujinxPath">
            修改路径
          </v-btn>
        </div>

        <div class="version-grid">
          <section class="version-section">
            <h3 class="row-title">当前已安装版本</h3>
            <p class="version-value">
              <span
                class="status-dot"
                :class="configStore.config.ryujinx.version ? 'bg-success' : 'bg-warning'"
                aria-hidden="true"
              />
              <span class="version-text">{{ configStore.config.ryujinx.version || '未识别' }}</span>
            </p>
            <div class="version-actions">
              <p class="section-description">
                {{ configStore.config.ryujinx.version ? '可重新检测当前模拟器版本。' : '未识别当前模拟器版本，可尝试重新检测。' }}
              </p>
              <v-btn color="secondary" variant="outlined" :disabled="isRunningInstall" @click="detectRyujinxVersion">
                重新检测
              </v-btn>
            </div>
          </section>

          <section class="version-section latest-version">
            <h3 class="row-title">最新 {{ selectedEmulatorName }} 版本</h3>
            <p class="version-value version-text">{{ latestRyujinxVersion }}</p>
            <ChangeLogDialog v-if="selectedBranch === 'canary' || selectedBranch === 'mainline'">
              <template #activator="{ props }">
                <v-btn v-bind="props" color="secondary" variant="outlined" @click="loadChangeLog">
                  查看更新日志
                </v-btn>
              </template>
              <template #content>
                <div v-html="changeLogHtml" />
              </template>
            </ChangeLogDialog>
          </section>
        </div>
      </v-card>

      <header class="component-header">
        <h2 class="page-title text-primary">组件管理</h2>
        <p class="section-description">管理模拟器版本与固件</p>
      </header>

      <v-card class="management-card" variant="elevated">
        <div class="install-row">
          <div>
            <h3 class="row-title">模拟器版本安装</h3>
            <p class="section-description">安装指定版本的 {{ selectedEmulatorName }} 模拟器。</p>
          </div>
          <div class="install-controls">
            <v-text-field
              v-model="targetRyujinxVersion"
              label="安装版本"
              variant="outlined"
              density="compact"
              color="primary"
              hide-details
              class="version-input"
              :disabled="isRunningInstall || isLoadingRyujinxVersions"
              :loading="isLoadingRyujinxVersions"
            />
            <v-btn
              color="info"
              variant="flat"
              :disabled="isRunningInstall || isLoadingRyujinxVersions"
              @click="installRyujinx"
            >安装模拟器</v-btn>
          </div>
        </div>

        <v-divider class="card-divider" />

        <div class="install-row">
          <div>
            <h3 class="row-title">固件管理</h3>
            <p class="section-description">安装或更新 Nintendo Switch 固件。</p>
          </div>
          <div class="firmware-controls">
            <div class="firmware-status">
              <p class="section-description">当前版本</p>
              <p class="firmware-version">
                <span
                  class="status-dot"
                  :class="configStore.config.ryujinx.firmware ? 'bg-success' : 'bg-warning'"
                  aria-hidden="true"
                />
                <span class="version-text">{{ configStore.config.ryujinx.firmware || '未识别' }}</span>
              </p>
              <v-tooltip text="重新检测固件版本，需先安装密钥" location="top">
                <template #activator="{ props }">
                  <v-btn v-bind="props" color="secondary" variant="outlined" :disabled="isRunningInstall" @click="detectFirmwareVersion">
                    重新检测
                  </v-btn>
                </template>
              </v-tooltip>
            </div>
            <div class="install-controls">
              <v-autocomplete
                v-model="appStore.targetFirmwareVersion"
                :items="appStore.availableFirmwareInfos"
                item-title="name"
                item-value="version"
                label="安装固件版本"
                variant="outlined"
                density="compact"
                color="primary"
                hide-details
                :disabled="isRunningInstall"
              />
              <v-btn color="info" variant="flat" :disabled="isRunningInstall" @click="firmwareInstallationWarningDialog = true">
                安装固件
              </v-btn>
            </div>
          </div>
        </div>

        <v-divider class="card-divider" />

        <footer class="component-footer">
          <p class="section-description firmware-note">
            <v-icon :icon="mdiInformationOutline" size="20" />
            能正常运行游戏时，无需更新固件。
          </p>
          <p class="section-description">
            安装或更新固件后，请安装对应密钥。
            <router-link to="/keys" class="keys-link text-accent">密钥管理</router-link>
          </p>
        </footer>
      </v-card>
    </div>
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
import {mdiInformationOutline, mdiPlay, mdiTrashCanOutline} from "@mdi/js";
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
  getRyujinxChangeLogs,
  getPlatform
} from "@/utils/tauri";

let allRyujinxReleaseInfos = ref<{tag_name: string}[]>([])
let historyPathList = ref<string[]>([])
let selectedRyujinxPath = ref('')
let targetRyujinxVersion = ref('')
let isRunningInstall = ref(false)
let isLoadingRyujinxVersions = ref(false)
let platform = ref('')
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
const selectedEmulatorName = computed(() => selectedBranch.value === 'canary' ? 'Ryujinx Canary' : 'Ryujinx')
const cds = useConsoleDialogStore()
const configStore = useConfigStore()
const appStore = useAppStore()

function showNotice(type: 'info' | 'success' | 'warning' | 'error', content: string, persistent = type === 'error') {
  window.$bus.emit('showNotifyMessage', {
    type,
    content,
    persistent,
  })
}

let latestRyujinxVersion = computed(() => {
  if (allRyujinxReleaseInfos.value.length > 0) {
    return allRyujinxReleaseInfos.value[0]['tag_name']
  }
  return "加载中"
})

let isMacOS = computed(() => platform.value === 'macos')

onBeforeMount(async () => {
  platform.value = await getPlatform()
  await configStore.reloadConfig()
  await loadHistoryPathList()
  appStore.updateAvailableFirmwareInfos()
  selectedRyujinxPath.value = configStore.config.ryujinx.path
  selectedBranch.value = configStore.config.ryujinx.branch
  updateRyujinxReleaseInfos()
  updateLastOpenEmuPage('ryujinx')
})

async function updateRyujinxReleaseInfos() {
  isLoadingRyujinxVersions.value = true
  allRyujinxReleaseInfos.value = []
  targetRyujinxVersion.value = "加载中..."
  try {
    const data = await getAllRyujinxVersions(selectedBranch.value)
    if (data.code === 0) {
      const infos = data.data || []
      allRyujinxReleaseInfos.value = infos.map(v => ({ tag_name: v }))
      targetRyujinxVersion.value = infos[0] ?? ''
    } else {
      cds.appendConsoleMessage('ryujinx 版本信息加载异常.')
      targetRyujinxVersion.value = "加载失败"
    }
  } catch (error) {
    cds.appendConsoleMessage('ryujinx 版本信息加载异常: ' + error)
    console.error('获取 Ryujinx 版本信息失败:', error)
    targetRyujinxVersion.value = "加载失败"
  } finally {
    isLoadingRyujinxVersions.value = false
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
  cds.cleanMessages()
  cds.appendConsoleMessage('=============================================')
  cds.appendConsoleMessage('选择的目录将作为存放模拟器的根目录')
  cds.appendConsoleMessage('建议新建目录单独存放')
  cds.appendConsoleMessage('=============================================')
  showNotice('info', '选择的目录将作为存放模拟器的根目录，建议新建目录单独存放', false)
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
  try {
    await detectFirmwareVersionApi('ryujinx')
    await configStore.reloadConfig()
  } catch (error) {
    console.error('检测固件版本失败:', error)
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

<style scoped src="@/styles/emulator-management.css"></style>
