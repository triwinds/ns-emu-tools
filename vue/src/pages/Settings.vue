<template>
  <v-container>
    <v-row>
      <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
      <v-col md="10" lg="8">
        <v-card>
          <v-card-title class="text-h4 primary--text">
            设置
          </v-card-title>
          <v-divider></v-divider>
          <v-container>
            <v-row>
              <v-col><p class="success--text text-h5">网络设置</p></v-col>
            </v-row>
            <v-select
              v-model="setting.network.cdnMode"
              :items="availableNetworkMode"
              item-text="name"
              item-value="value"
              label="下载源 CDN 配置"
            ></v-select>
            <v-select
              v-model="setting.network.githubApiMode"
              :items="availableNetworkMode"
              item-text="name"
              item-value="value"
              label="GitHub Api CDN 配置"
            ></v-select>
          </v-container>
        </v-card>
      </v-col>
      <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
    </v-row>
  </v-container>
</template>

<script>
import store from "@/store";

export default {
  name: "SettingsPage",
  data() {
    return {
      setting: store.state.config.setting,
      inited: false,
      availableNetworkMode: [
        {name: '依据系统代理自动决定', value: 'auto-detect'},
        {name: '使用 CDN', value: 'cdn'},
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