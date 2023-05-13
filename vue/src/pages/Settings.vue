<template>
  <SimplePage>
    <v-card>
      <v-card-title class="text-h4 primary--text">
        设置
      </v-card-title>
      <v-divider></v-divider>
      <v-container>
        <v-row>
          <v-col>
            <p class="success--text text-h5">网络设置</p>
            <span class="body-2 info--text">Cloudflare 源下载速度慢可以看看 <router-link
                to="/cloudflareST">这个</router-link></span>
          </v-col>
        </v-row>
        <v-select
            v-model="setting.network.firmwareSource"
            :items="availableNetworkMode"
            item-text="name"
            item-value="value"
            label="固件下载源配置"
            hide-details
        ></v-select>
        <v-select
            v-model="setting.network.githubApiMode"
            :items="availableNetworkMode"
            item-text="name"
            item-value="value"
            label="GitHub Api CDN 配置"
            hide-details
        ></v-select>
        <v-select
            v-model="setting.network.githubDownloadMirror"
            :items="availableGithubDownloadSource"
            item-text="name"
            item-value="value"
            label="GitHub 下载源配置"
            persistent-hint
            hint="如果速度可以接受，希望大家尽量多使用前面的美国节点，避免流量都集中到亚洲公益节点，减少成本压力运营才能更持久~"
        ></v-select>
        <v-divider style="margin-bottom: 10px; margin-top: 10px"></v-divider>
        <v-select
            v-model="proxyMode"
            :items="availableProxyMode"
            item-text="name"
            item-value="value"
            label="代理设置"
            hide-details
            @change="onProxyModeChange"
        ></v-select>
        <v-text-field v-if="proxyMode === 'http'" v-model="proxyInput" @change="onProxyChange" label="代理服务器地址"
                      persistent-hint hint="仅支持 HTTP 代理, 如 http://127.0.0.1:7890"
                      :rules="[rules.validateProxy]"
        ></v-text-field>
        <v-switch v-model="setting.network.useDoh">
          <template v-slot:label>
            访问 api 时使用 DNS over HTTPS
            <v-tooltip top>
              <template v-slot:activator="{ on, attrs }">
                <v-btn icon v-bind="attrs" v-on="on">
                  <v-icon color="grey lighten-1">
                    {{ svgPath.helpCircle }}
                  </v-icon>
                </v-btn>
              </template>
              <span>可以解决运营商劫持 DNS 的问题, 但会稍微降低访问速度, 重启程序后生效</span>
            </v-tooltip>
          </template>
        </v-switch>


        <v-divider style="margin-bottom: 10px"></v-divider>
        <v-row>
          <v-col><p class="success--text text-h5">下载设置</p></v-col>
        </v-row>
        <v-switch hide-details v-model="setting.download.autoDeleteAfterInstall" label="安装完成后自动删除下载的安装包"></v-switch>
        <v-switch hide-details v-model="setting.download.disableAria2Ipv6" label="aria2 禁用 IPv6 (重启程序后生效)"></v-switch>
        <v-switch hide-details v-model="setting.download.removeOldAria2LogFile" label="启动 aria2 前删除旧的日志"></v-switch>
        <v-switch hide-details v-model="setting.download.verifyFirmwareMd5" label="固件下载完成后校验 md5"></v-switch>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script>
import store from "@/store";
import SimplePage from "@/components/SimplePage";
import {
  mdiHelpCircle
} from '@mdi/js'

function isValidHttpUrl(string) {
  let url;

  try {
    url = new URL(string);
  } catch (_) {
    return false;
  }

  return url.protocol === "http:" || url.protocol === "https:";
}

export default {
  name: "SettingsPage",
  components: {SimplePage},
  data() {
    return {
      setting: store.state.config.setting,
      inited: false,
      availableNetworkMode: [
        {name: '根据系统代理自动决定', value: 'auto-detect'},
        {name: '[美国 Cloudflare CDN] - 自建代理服务器', value: 'cdn'},
        {name: '直连', value: 'direct'},
      ],
      availableGithubDownloadSource: [],
      availableProxyMode: [
          {name: '自动检测系统代理', value: 'system'},
          {name: '手动配置 HTTP 代理', value: 'http'},
          {name: '不使用代理', value: 'none'},
      ],
      proxyMode: '',
      proxyInput: '',
      svgPath: {
        helpCircle: mdiHelpCircle,
      },
      rules: {
        validateProxy(value) {
          if (!value || value.trim() === '' || value === 'system' || isValidHttpUrl(value)) {
            return true;
          }
          return '仅支持 HTTP 代理, 如 http://127.0.0.1:7890'
        }
      }
    }
  },
  async mounted() {
    let config = await this.$store.dispatch('loadConfig');
    await this.loadAvailableGithubDownloadSource()
    this.setting = config.setting
    this.proxyInput = config.setting.network.proxy
    this.updateProxyMode()
  },
  methods: {
    async loadAvailableGithubDownloadSource() {
      let resp = await window.eel.get_github_mirrors()()
      console.log(resp)
      if (resp.code === 0) {
        for (let mirror of resp.data) {
          this.availableGithubDownloadSource.push({name: mirror[2], value: mirror[0]})
        }
      }
    },
    updateProxyMode() {
      if (this.proxyInput === 'system') {
        this.proxyMode = 'system'
      } else if (this.proxyInput.startsWith('http')) {
        this.proxyMode = 'http'
      } else {
        this.proxyMode = 'none'
      }
    },
    onProxyChange() {
      console.log(`onProxyChange, current input: ${this.proxyInput}`)
      this.setting.network.proxy = this.proxyInput.trim()
      this.updateProxyMode()
    },
    onProxyModeChange() {
      if (this.proxyMode === 'http' && this.proxyInput === 'system') {
        this.proxyInput = ''
      } else if (this.proxyMode === 'system') {
        this.proxyInput = 'system'
      } else if (this.proxyMode === 'none') {
        this.proxyInput = ''
      }
      console.log(`onProxyModeChange, current input: ${this.proxyInput}`)
      this.setting.network.proxy = this.proxyInput
    }
  },
  watch: {
    setting: {
      deep: true,
      immediate: false,
      async handler(newValue) {
        if (!this.inited) {
          this.inited = true
          return
        }
        console.log(this.inited)
        console.log(newValue)
        let resp = await window.eel.update_setting(newValue)()
        if (resp['code'] === 0) {
          this.$store.commit('UPDATE_CONFIG', resp['data'])
        }
      }
    }
  }
}
</script>

<style scoped>

</style>