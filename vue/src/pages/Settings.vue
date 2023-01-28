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
                <span class="body-2 info--text">Cloudflare 源下载速度慢可以看看 <router-link to="/cloudflareST">这个</router-link></span>
              </v-col>
            </v-row>
            <v-select
              v-model="setting.network.firmwareSource"
              :items="availableNetworkMode"
              item-text="name"
              item-value="value"
              label="固件下载源配置"
            ></v-select>
            <v-select
              v-model="setting.network.githubApiMode"
              :items="availableNetworkMode"
              item-text="name"
              item-value="value"
              label="GitHub Api CDN 配置"
            ></v-select>
            <v-select
              v-model="setting.network.githubDownloadSource"
              :items="availableGithubDownloadSource"
              item-text="name"
              item-value="value"
              label="GitHub 下载源配置"
            ></v-select>
            <v-divider style="margin-bottom: 10px"></v-divider>
            <v-row>
              <v-col><p class="success--text text-h5">下载设置</p></v-col>
            </v-row>
            <v-switch v-model="setting.download.autoDeleteAfterInstall" label="安装完成后自动删除下载的安装包"></v-switch>
            <v-switch v-model="setting.download.disableAria2Ipv6" label="aria2 禁用 IPV6 (重启程序后生效)"></v-switch>
            <v-switch v-model="setting.download.removeOldAria2LogFile" label="启动 aria2 前删除旧的日志"></v-switch>
          </v-container>
        </v-card>
  </SimplePage>
</template>

<script>
import store from "@/store";
import SimplePage from "@/components/SimplePage";

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
      availableGithubDownloadSource: [
        {name: '[美国 Cloudflare CDN] - 自建代理服务器', value: 'self'},
        {name: '[美国 Cloudflare CDN] - 该公益加速源由 [知了小站] 提供', value: 'zhiliao'},
        {name: '[韩国 首尔] - 该公益加速源由 [ghproxy] 提供', value: 'ghproxy'},
        {name: '直连', value: 'direct'},
      ]
    }
  },
  async mounted() {
    let config = await this.$store.dispatch('loadConfig');
    this.setting = config.setting
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