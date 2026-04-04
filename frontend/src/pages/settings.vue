<template>
  <SimplePage>
    <v-snackbar
        v-model="githubMirrorRefreshNotice.visible"
        :color="githubMirrorRefreshNotice.color"
        :timeout="githubMirrorRefreshNotice.timeout"
        location="top"
    >
      {{ githubMirrorRefreshNotice.text }}
    </v-snackbar>
    <v-card>
      <v-card-title class="text-h4 text-primary">
        设置
      </v-card-title>
      <v-divider></v-divider>
      <v-container>
        <v-row>
          <v-col>
            <p class="text-success text-h5" style="margin-bottom: 10px;">网络设置</p>
          </v-col>
        </v-row>
        <v-select
            v-model="setting.network.githubApiMode"
            :items="availableNetworkMode"
            item-title="name"
            item-value="value"
            label="GitHub Api CDN 配置"
            hide-details
            variant="underlined"
            color="primary"
        ></v-select>
        <v-select
            v-model="setting.network.ryujinxGitLabDownloadMirror"
            :items="availableNetworkMode"
            item-title="name"
            item-value="value"
            label="Ryujinx GitLab CDN 配置"
            hide-details
            variant="underlined"
            color="primary"
        ></v-select>
        <v-select
            v-model="setting.network.edenGitDownloadMirror"
            :items="availableNetworkMode"
            item-title="name"
            item-value="value"
            label="Eden 官方源 CDN 配置"
            hide-details
            variant="underlined"
            color="primary"
        ></v-select>
        <v-row align="center">
          <v-col>
            <v-select
                v-model="setting.network.githubDownloadMirror"
                :items="availableGithubDownloadSource"
                item-title="name"
                item-value="value"
                label="GitHub 下载源配置"
                persistent-hint
                hint="如果速度可以接受，希望大家尽量多使用前面的美国节点，避免流量都集中到亚洲公益节点，减少成本压力运营才能更持久~"
                variant="underlined"
                color="primary"
            ></v-select>
          </v-col>
          <v-col cols="auto">
            <v-btn
                color="primary"
                variant="text"
                :prepend-icon="mdiRefresh"
                :loading="isRefreshingGithubMirrors"
                :disabled="isRefreshingGithubMirrors"
                @click="onRefreshGithubMirrors"
            >
              刷新列表
            </v-btn>
          </v-col>
        </v-row>
        <v-divider style="margin-bottom: 10px; margin-top: 10px"></v-divider>
        <v-select
            v-model="proxyMode"
            :items="availableProxyMode"
            item-title="name"
            item-value="value"
            label="代理设置"
            hide-details
            @update:model-value="onProxyModeChange"
            variant="underlined"
            color="primary"
        ></v-select>
        <v-text-field
            v-if="proxyMode === 'http'" v-model="proxyInput" @update:model-value="onProxyChange" label="代理服务器地址"
            persistent-hint hint="仅支持 HTTP 代理, 如 http://127.0.0.1:7890"
            :rules="[rules.validateProxy]"
            variant="underlined"
            color="primary"
        ></v-text-field>
        <v-switch
            v-model="setting.network.useDoh"
            variant="underlined"
            color="primary"
        >
          <template v-slot:label>
            访问 api 时使用 DNS over HTTPS
            <v-tooltip top>
              <template v-slot:activator="{ props }">
                <v-icon color="grey lighten-1" v-bind="props">
                  {{ mdiHelpCircle }}
                </v-icon>
              </template>
              <span>可以解决运营商劫持 DNS 的问题, 但会稍微降低访问速度, 重启程序后生效</span>
            </v-tooltip>
          </template>
        </v-switch>


        <v-divider style="margin-bottom: 10px"></v-divider>
        <v-row>
          <v-col><p class="text-success text-h5">下载设置</p></v-col>
        </v-row>
        <v-select
            v-model="setting.download.backend"
            :items="availableDownloadBackend"
            item-title="name"
            item-value="value"
            label="下载器选择"
            persistent-hint
            hint="重启程序后生效。Auto: 优先使用 aria2，不可用时自动切换到 Rust 下载器；Aria2: 强制使用 aria2（多线程下载，需要下载 aria2）；Rust: 强制使用纯 Rust 下载器（内置，无需额外下载）"
            variant="underlined"
            color="primary"
        ></v-select>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.autoDeleteAfterInstall" label="安装完成后自动删除下载的安装包"></v-switch>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.disableAria2Ipv6" label="aria2 禁用 IPv6 (重启程序后生效)"></v-switch>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.removeOldAria2LogFile" label="启动 aria2 前删除旧的日志"></v-switch>


        <v-divider style="margin-bottom: 10px"></v-divider>
        <v-row>
          <v-col><p class="text-success text-h5">其它设置</p></v-col>
        </v-row>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.other.rename_yuzu_to_cemu"
                  label="安装完成后将 yuzu.exe 重命名为 cemu.exe (For Windows Auto HDR)"></v-switch>
      </v-container>
      <v-divider></v-divider>
      <v-card-actions>
        <v-btn block color="info" variant="outlined" :prepend-icon="mdiFolderOpenOutline" @click="openConfigJsonFolder">
          打开 config.json 所在文件夹
        </v-btn>
      </v-card-actions>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import {useConfigStore} from "@/stores/ConfigStore";
import {onBeforeMount, onMounted, ref, watch} from "vue";
import type {NameValueItem, Setting} from "@/types";
import {defaultConfig} from "@/types/DefaultConfig";
import {
  extractErrorMessage,
  updateSetting,
  getGithubMirrors,
  openConfigFolder,
  refreshGithubMirrors,
  type GithubMirrorListResponse,
} from "@/utils/tauri";
import {
  mdiFolderOpenOutline,
  mdiHelpCircle,
  mdiRefresh
} from '@mdi/js'

let configStore = useConfigStore()
configStore.reloadConfig()
let setting: Setting = defaultConfig.setting

onBeforeMount(async () => {
  await configStore.reloadConfig()
  setting = configStore.config.setting
})
let availableNetworkMode = [
  {name: '根据系统代理自动决定', value: 'auto-detect'},
  {name: '[美国 Cloudflare CDN] - 自建代理服务器', value: 'cdn'},
  {name: '直连', value: 'direct'},
]
let availableDownloadBackend = [
  {name: 'Auto - 自动选择（推荐）', value: 'auto'},
  {name: 'Aria2 - 多线程下载', value: 'aria2'},
  {name: 'Rust - 内置下载器', value: 'rust'},
]
let availableGithubDownloadSource = ref<NameValueItem[]>([])
let isRefreshingGithubMirrors = ref(false)
let githubMirrorRefreshNotice = ref({
  visible: false,
  text: '',
  color: 'success',
  timeout: 3000,
})
let availableProxyMode = [
    {name: '自动检测系统代理', value: 'system'},
    {name: '手动配置 HTTP 代理', value: 'http'},
    {name: '不使用代理', value: 'none'},
]
let proxyMode = ref('')
let proxyInput = ref('')
let rules = {
  validateProxy(value: string) {
    if (!value || value.trim() === '' || value === 'system' || isValidHttpUrl(value)) {
      return true;
    }
    return '仅支持 HTTP 代理, 如 http://127.0.0.1:7890'
  }
}

function isValidHttpUrl(string: string) {
  let url;

  try {
    url = new URL(string);
  } catch (_) {
    return false;
  }

  return url.protocol === "http:" || url.protocol === "https:";
}


onMounted(async () => {
  await loadAvailableGithubDownloadSource()
  proxyInput.value = setting.network.proxy
  updateProxyMode()
  watch(setting, async (newValue) => {
    console.log(newValue)
    try {
      await updateSetting(newValue)
      await configStore.reloadConfig()
    } catch (e) {
      console.error('Failed to update setting:', e)
    }
  }, {deep: true, immediate: false})
})

function applyGithubMirrorOptions(mirrors: Array<[string, string, string]>) {
  const options = mirrors.map((mirror) => ({name: formatGithubMirrorOptionName(mirror[2]), value: mirror[0]}))

  const currentMirror = setting.network.githubDownloadMirror
  if (currentMirror && !options.some((option) => option.value === currentMirror)) {
    options.push({name: `当前配置: ${currentMirror}`, value: currentMirror})
  }

  availableGithubDownloadSource.value = options
}

function formatGithubMirrorOptionName(name: string) {
  const trimmedName = name
    .replace(/\s*提示[:：][\s\S]*$/u, '')
    .replace(/\s+/gu, ' ')
    .trim()

  return trimmedName || name.trim()
}

function showGithubMirrorRefreshNotice(
  text: string,
  color: 'success' | 'error' | 'warning',
  timeout: number = 3000,
) {
  githubMirrorRefreshNotice.value = {
    visible: true,
    text,
    color,
    timeout,
  }
}

function handleGithubMirrorFallbackNotice(response: GithubMirrorListResponse) {
  const notice = response.fallback_notice
  if (!notice) {
    return false
  }

  setting.network.githubDownloadMirror = notice.effective_mirror
  window.$bus.emit('showNotifyMessage', {
    type: 'warning',
    content: notice.message,
    persistent: true,
  })
  return true
}

async function loadAvailableGithubDownloadSource() {
  try {
    const response = await getGithubMirrors()
    applyGithubMirrorOptions(response.mirrors)
    handleGithubMirrorFallbackNotice(response)
  } catch (e) {
    console.error('Failed to load github mirrors:', e)
  }
}

async function onRefreshGithubMirrors() {
  isRefreshingGithubMirrors.value = true
  try {
    const response = await refreshGithubMirrors()
    applyGithubMirrorOptions(response.mirrors)
    if (!handleGithubMirrorFallbackNotice(response)) {
      showGithubMirrorRefreshNotice(`GitHub 镜像列表已刷新，共 ${response.mirrors.length} 个选项`, 'success')
    }
  } catch (e) {
    console.error('Failed to refresh github mirrors:', e)
    showGithubMirrorRefreshNotice(`刷新 GitHub 镜像列表失败: ${extractErrorMessage(e)}`, 'error')
    await loadAvailableGithubDownloadSource()
  } finally {
    isRefreshingGithubMirrors.value = false
  }
}

function updateProxyMode() {
  if (proxyInput.value === 'system') {
    proxyMode.value = 'system'
  } else if (proxyInput.value.startsWith('http')) {
    proxyMode.value = 'http'
  } else {
    proxyMode.value = 'none'
  }
}

function onProxyChange() {
  console.log(`onProxyChange, current input: ${proxyInput.value}`)
  setting.network.proxy = proxyInput.value.trim()
  updateProxyMode()
}
function onProxyModeChange() {
  if (proxyMode.value === 'http' && proxyInput.value === 'system') {
    proxyInput.value = ''
  } else if (proxyMode.value === 'system') {
    proxyInput.value = 'system'
  } else if (proxyMode.value === 'none') {
    proxyInput.value = ''
  }
  console.log(`onProxyModeChange, current input: ${proxyInput.value}`)
  setting.network.proxy = proxyInput.value
}

async function openConfigJsonFolder() {
  await openConfigFolder()
}

</script>

<style scoped>
.v-select {
  margin-bottom: 5px;
  margin-top: 10px;
}
</style>
