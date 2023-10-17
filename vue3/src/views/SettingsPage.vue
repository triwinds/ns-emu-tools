<template>
  <SimplePage>
    <v-card>
      <v-card-title class="text-h4 text-primary">
        设置
      </v-card-title>
      <v-divider></v-divider>
      <v-container>
        <v-row>
          <v-col>
            <p class="text-success text-h5" style="margin-bottom: 10px;">网络设置</p>
            <span class="body-2 text-info">Cloudflare 源下载速度慢可以看看 <router-link
                to="/cloudflareST">这个</router-link></span>
          </v-col>
        </v-row>
<!--        <v-select-->
<!--            v-model="setting.network.firmwareSource"-->
<!--            :items="availableNetworkMode"-->
<!--            item-title="name"-->
<!--            item-value="value"-->
<!--            label="固件下载源配置"-->
<!--            hide-details-->
<!--            variant="underlined"-->
<!--            color="primary"-->
<!--        ></v-select>-->
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
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.autoDeleteAfterInstall" label="安装完成后自动删除下载的安装包"></v-switch>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.disableAria2Ipv6" label="aria2 禁用 IPv6 (重启程序后生效)"></v-switch>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.removeOldAria2LogFile" label="启动 aria2 前删除旧的日志"></v-switch>
        <v-switch density="compact" color="primary" :hide-details="true" v-model="setting.download.verifyFirmwareMd5" label="固件下载完成后校验 md5"></v-switch>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import {useConfigStore} from "@/store/ConfigStore";
import {onMounted, ref, watch} from "vue";
import {NameValueItem} from "@/types";
import {
  mdiHelpCircle
} from '@mdi/js'


let configStore = useConfigStore()
configStore.reloadConfig()
let setting = configStore.config.setting
let availableNetworkMode = [
  {name: '根据系统代理自动决定', value: 'auto-detect'},
  {name: '[美国 Cloudflare CDN] - 自建代理服务器', value: 'cdn'},
  {name: '直连', value: 'direct'},
]
let availableGithubDownloadSource = ref<NameValueItem[]>([])
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
  setting = configStore.config.setting
  proxyInput.value = setting.network.proxy
  updateProxyMode()
  watch(setting, async (newValue) => {
    console.log(newValue)
    let resp = await window.eel.update_setting(newValue)()
    if (resp['code'] === 0) {
      configStore.config = resp['data']
    }
  }, {deep: true, immediate: false})
})
async function loadAvailableGithubDownloadSource() {
  let resp = await window.eel.get_github_mirrors()()
  console.log(resp)
  if (resp.code === 0) {
    for (let mirror of resp.data) {
      availableGithubDownloadSource.value.push({name: mirror[2], value: mirror[0]})
    }
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

</script>

<style scoped>
.v-select {
  margin-bottom: 5px;
  margin-top: 10px;
}
</style>
