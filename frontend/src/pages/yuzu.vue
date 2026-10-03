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
          @update:model-value="switchYuzuBranch"
        />
      </header>

      <v-card class="management-card" variant="elevated">
        <div class="emulator-identity">
          <div class="identity-title">
            <v-img src="@/assets/yuzu.webp" width="48" height="48" class="emulator-logo" />
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
            @click="startYuzuHandler"
          >启动模拟器</v-btn>
        </div>

        <v-divider class="card-divider" />

        <div class="path-row">
          <h3 class="row-title">安装文件夹</h3>
          <v-autocomplete
            v-model="selectedYuzuPath"
            :items="historyPathList"
            label="安装目录"
            variant="outlined"
            density="compact"
            color="primary"
            hide-details
            :disabled="isRunningInstall"
            class="path-input"
            @update:model-value="updateYuzuPathHandler"
          >
            <template #item="{ props, item }">
              <v-list-item v-bind="props" :title="item.raw">
                <template #append>
                  <v-btn
                    v-if="selectedYuzuPath !== item.raw"
                    color="error"
                    size="small"
                    icon
                    variant="text"
                    :aria-label="`删除历史路径 ${item.raw}`"
                    @click.stop="deleteHistoryPathHandler(item.raw)"
                  >
                    <v-icon size="small" :icon="mdiTrashCanOutline" />
                  </v-btn>
                </template>
              </v-list-item>
            </template>
          </v-autocomplete>
          <v-btn color="secondary" variant="outlined" :disabled="isRunningInstall" @click="modifyYuzuPath">
            修改路径
          </v-btn>
        </div>

        <div class="version-grid">
          <section class="version-section">
            <h3 class="row-title">当前已安装版本</h3>
            <p class="version-value">
              <span
                class="status-dot"
                :class="yuzuConfig.yuzu_version ? 'bg-success' : 'bg-warning'"
                aria-hidden="true"
              />
              <span class="version-text">{{ yuzuConfig.yuzu_version || '未识别' }}</span>
            </p>
            <div class="version-actions">
              <p class="section-description">
                {{ yuzuConfig.yuzu_version ? '可重新检测当前模拟器版本。' : '未识别当前模拟器版本，可尝试重新检测。' }}
              </p>
              <v-btn color="secondary" variant="outlined" :disabled="isRunningInstall" @click="detectYuzuVersionHandler">
                重新检测
              </v-btn>
            </div>
          </section>

          <section class="version-section latest-version">
            <h3 class="row-title">最新 {{ selectedEmulatorName }} 版本</h3>
            <p class="version-value version-text">{{ latestYuzuVersion }}</p>
            <ChangeLogDialog v-if="isBranchAvailable">
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
              v-model="targetYuzuVersion"
              label="安装版本"
              variant="outlined"
              density="compact"
              color="primary"
              hide-details
              class="version-input"
              :disabled="isRunningInstall || isLoadingYuzuVersions || !isBranchAvailable"
              :loading="isLoadingYuzuVersions"
            />
            <v-btn
              color="info"
              variant="flat"
              :disabled="isRunningInstall || isLoadingYuzuVersions || !isBranchAvailable"
              @click="installYuzuHandler"
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
                  :class="yuzuConfig.yuzu_firmware ? 'bg-success' : 'bg-warning'"
                  aria-hidden="true"
                />
                <span class="version-text">{{ yuzuConfig.yuzu_firmware || '未识别' }}</span>
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
              <v-btn color="info" variant="flat" :disabled="isRunningInstall" @click="firmwareInstallationWarning = true">
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
import {computed, onBeforeMount, onMounted, onUnmounted, ref} from "vue";
import {useConfigStore} from "@/stores/ConfigStore";
import type {CommonResponse} from "@/types";
import {useAppStore} from "@/stores/app";
import {useConsoleDialogStore} from "@/stores/ConsoleDialogStore";
import {mdiInformationOutline, mdiPlay, mdiTrashCanOutline} from "@mdi/js";
import markdown from "@/utils/markdown";
import SimplePage from "@/components/SimplePage.vue";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import DialogTitle from "@/components/DialogTitle.vue";
import { open } from '@tauri-apps/plugin-dialog'
import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  extractErrorMessage,
  getAllYuzuVersions,
  switchYuzuBranch as switchBranchAPI,
  installYuzu as installYuzuAPI,
  installFirmwareToYuzu,
  detectYuzuVersion as detectYuzuVersionAPI,
  getYuzuChangeLogs,
  startYuzu as startYuzuAPI,
  updateYuzuPath as updateYuzuPathAPI,
  deleteHistoryPath as deleteHistoryPathAPI,
  updateLastOpenEmuPage,
  getStorage,
  onDownloadProgress,
  onNotifyMessage,
  formatSize,
  formatSpeed,
  detectFirmwareVersion as detectFirmwareVersionApi,
  type DownloadProgress,
  type NotifyMessage
} from '@/utils/tauri'

let allYuzuReleaseVersions = ref<string[]>([])
let targetYuzuVersion = ref('项目已被关闭')
let isRunningInstall = ref(false)
let isBranchAvailable = ref(false)
let isLoadingYuzuVersions = ref(false)
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
function showNotice(type: NotifyMessage['type'], content: string, persistent = type === 'error') {
  window.$bus.emit('showNotifyMessage', {
    type,
    content,
    persistent,
  })
}

let yuzuConfig = computed(() => {
  return configStore.config.yuzu
})
let branch = computed(() => {
  return normalizeYuzuBranch(configStore.config.yuzu.branch)
})

function normalizeYuzuBranch(value: string) {
  if (value === 'citron') {
    return 'citron-stable'
  }
  return value
}

let branches = [
  {
    text: 'Eden',
    value: 'eden',
    name: 'Eden',
    available: true
  },
  {
    text: 'Eden Nightly',
    value: 'eden-nightly',
    name: 'Eden Nightly',
    available: true
  },
  {
    text: 'Citron Stable',
    value: 'citron-stable',
    name: 'Citron Stable',
    available: true
  },
  {
    text: 'Citron Nightly',
    value: 'citron-nightly',
    name: 'Citron Nightly',
    available: true
  },
  {
    text: 'Yuzu 主线 (项目已关闭)',
    value: 'mainline',
    name: 'Yuzu',
    available: false
  },
  {
    text: 'Yuzu EA (项目已关闭)',
    value: 'ea',
    name: 'Yuzu EA',
    available: false
  },
]

let branchMap: Record<string, any> = {}
for (let branch of branches) {
  branchMap[branch.value] = branch
}

let selectedEmulatorName = computed(() => {
  return branchMap[selectedBranch.value]?.name ?? 'Yuzu'
})

let availableBranch = ref(branches)
let selectedBranch = ref('')
let latestYuzuVersion = computed(() => {
  if (allYuzuReleaseVersions.value.length > 0) {
    return allYuzuReleaseVersions.value[0]
  }
  if (!isBranchAvailable.value) {
    return '项目已被关闭'
  }
  return "加载中"
})

async function loadHistoryPathList() {
  const storage = await getStorage()
  historyPathList.value = Object.keys(storage.yuzu_history)
}

onBeforeMount(async () => {
  await loadHistoryPathList()
  await configStore.reloadConfig()
  appStore.updateAvailableFirmwareInfos()
  selectedYuzuPath.value = configStore.config.yuzu.yuzu_path
  selectedBranch.value = normalizeYuzuBranch(configStore.config.yuzu.branch)
  handleSelectedBranchUpdate()
  await updateLastOpenEmuPage('yuzu')
})

// 事件监听器
let unlistenDownload: UnlistenFn | null = null

onMounted(async () => {
  // 监听下载进度
  unlistenDownload = await onDownloadProgress((event) => {
    const progress: DownloadProgress = event.payload
    if (progress.percentage > 0) {
      const msg = `下载进度: ${progress.percentage.toFixed(1)}% - ${formatSize(progress.downloaded)} / ${formatSize(progress.total)} @ ${formatSpeed(progress.speed)}`
      consoleDialogStore.appendConsoleMessage(msg)
    }
  })
})

onUnmounted(() => {
  // 清理监听器
  if (unlistenDownload) unlistenDownload()
})

async function updateYuzuReleaseVersions() {
  console.log(selectedBranch.value)
  if (selectedBranch.value in branchMap && !branchMap[selectedBranch.value].available) {
    allYuzuReleaseVersions.value = []
    targetYuzuVersion.value = "项目已被关闭"
    isBranchAvailable.value = false
    return
  }
  isBranchAvailable.value = true
  isLoadingYuzuVersions.value = true
  allYuzuReleaseVersions.value = []
  targetYuzuVersion.value = "加载中..."

  try {
    const response = await getAllYuzuVersions(selectedBranch.value)
    if (response.code === 0) {
      console.log(response.data)
      const infos = response.data || []
      allYuzuReleaseVersions.value = infos
      targetYuzuVersion.value = infos[0] ?? ''
    } else {
      const message = 'yuzu 版本信息加载异常.'
      consoleDialogStore.appendConsoleMessage(message)
      showNotice('error', message)
      targetYuzuVersion.value = "加载失败"
    }
  } catch (error) {
    consoleDialogStore.appendConsoleMessage(`加载版本失败: ${error}`)
    targetYuzuVersion.value = "加载失败"
  } finally {
    isLoadingYuzuVersions.value = false
  }
}

function handleSelectedBranchUpdate() {
  if (selectedBranch.value in branchMap && !branchMap[selectedBranch.value].available) {
    allYuzuReleaseVersions.value = []
    targetYuzuVersion.value = "项目已被关闭"
    isBranchAvailable.value = false
    isLoadingYuzuVersions.value = false
    return
  } else {
    isBranchAvailable.value = true
    updateYuzuReleaseVersions()
  }
}

async function switchYuzuBranch() {
  await switchBranchAPI(selectedBranch.value)
  await configStore.reloadConfig()
  allYuzuReleaseVersions.value = []
  await handleSelectedBranchUpdate()
}

async function installFirmware() {
  isRunningInstall.value = true
  firmwareInstallationWarning.value = false

  try {
    await installFirmwareToYuzu(appStore.targetFirmwareVersion)
    await configStore.reloadConfig()
  } catch (error) {
    console.error('安装固件失败:', error)
  } finally {
    isRunningInstall.value = false
  }
}

async function installYuzuHandler() {
  isRunningInstall.value = true
  consoleDialogStore.persistentConsoleDialog = true

  try {
    await installYuzuAPI(targetYuzuVersion.value, branch.value)
    await configStore.reloadConfig()
    selectedBranch.value = normalizeYuzuBranch(configStore.config.yuzu.branch)
    // 成功消息已经通过后端的 send_notify 发送，不需要在这里重复显示
  } catch (error) {
    // 错误消息已经通过 notify_message 事件发送，不需要在这里重复显示
    console.error('安装失败:', error)
  } finally {
    isRunningInstall.value = false
    consoleDialogStore.persistentConsoleDialog = false
  }
}

async function detectFirmwareVersion() {
  try {
    await detectFirmwareVersionApi('yuzu')
    await configStore.reloadConfig()
  } catch (error) {
    console.error('检测固件版本失败:', error)
  }
}

async function loadChangeLog() {
  try {
    const response = await getYuzuChangeLogs(selectedBranch.value)
    if (response.code === 0) {
      changeLogHtml.value = markdown.parse(response.data || '')
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  } catch (error) {
    changeLogHtml.value = '<p>加载失败。</p>'
  }
}

async function detectYuzuVersionHandler() {
  let previousBranch = branch.value

  try {
    const response = await detectYuzuVersionAPI()
    await configStore.reloadConfig()
    selectedBranch.value = normalizeYuzuBranch(configStore.config.yuzu.branch)

    if (response.code === 0) {
      console.log(previousBranch, branch.value)
      if (previousBranch !== branch.value) {
        await handleSelectedBranchUpdate()
      }
      consoleDialogStore.appendConsoleMessage('Yuzu 版本检测完成')
    } else {
      consoleDialogStore.appendConsoleMessage('检测 yuzu 版本时发生异常')
    }
  } catch (error) {
    consoleDialogStore.appendConsoleMessage(`检测版本失败: ${error}`)
  }
}

async function startYuzuHandler() {
  try {
    await startYuzuAPI()
    consoleDialogStore.appendConsoleMessage('yuzu 启动成功')
  } catch (error) {
    consoleDialogStore.appendConsoleMessage(`yuzu 启动失败: ${error}`)
  }
}

async function modifyYuzuPath() {
  consoleDialogStore.cleanMessages()
  consoleDialogStore.appendConsoleMessage('=============================================')
  consoleDialogStore.appendConsoleMessage('选择的目录将作为存放模拟器的根目录')
  consoleDialogStore.appendConsoleMessage('建议新建目录单独存放')
  consoleDialogStore.appendConsoleMessage('=============================================')
  showNotice('info', '选择的目录将作为存放模拟器的根目录，建议新建目录单独存放', false)

  const selected = await open({
    directory: true,
    multiple: false,
    title: '选择 Yuzu 安装目录'
  })

  if (selected && typeof selected === 'string') {
    try {
      await updateYuzuPathAPI(selected)

      let oldBranch = configStore.config.yuzu.branch
      await configStore.reloadConfig()
      const newBranch = normalizeYuzuBranch(configStore.config.yuzu.branch)

      if (normalizeYuzuBranch(oldBranch) !== newBranch) {
        selectedBranch.value = newBranch
        await handleSelectedBranchUpdate()
      } else {
        selectedBranch.value = newBranch
      }

      await loadHistoryPathList()
      selectedYuzuPath.value = configStore.config.yuzu.yuzu_path
      consoleDialogStore.appendConsoleMessage('路径更新成功')
    } catch (error) {
      consoleDialogStore.appendConsoleMessage(`更新路径失败: ${error}`)
    }
  }

  await loadHistoryPathList()
}

async function deleteHistoryPathHandler(targetPath: string) {
  try {
    await deleteHistoryPathAPI('yuzu', targetPath)
    await loadHistoryPathList()
  } catch (error) {
    consoleDialogStore.appendConsoleMessage(`删除历史路径失败: ${error}`)
  }
}

async function updateYuzuPathHandler() {
  try {
    await updateYuzuPathAPI(selectedYuzuPath.value)

    let oldBranch = configStore.yuzuConfig.branch
    await configStore.reloadConfig()
    await loadHistoryPathList()
    selectedYuzuPath.value = configStore.yuzuConfig.yuzu_path
    const newBranch = normalizeYuzuBranch(configStore.yuzuConfig.branch)

    if (normalizeYuzuBranch(oldBranch) !== newBranch) {
      selectedBranch.value = newBranch
      await handleSelectedBranchUpdate()
    } else {
      selectedBranch.value = newBranch
    }
  } catch (error) {
    consoleDialogStore.appendConsoleMessage(`更新路径失败: ${error}`)
  }
}
</script>

<style scoped src="@/styles/emulator-management.css"></style>
