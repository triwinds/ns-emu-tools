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
            <v-switch
              v-model="setting.network.useOriginalUrlDirectly"
              inset
              label="不使用 Cloudflare 代理"
            ></v-switch>
            <v-switch
              v-model="setting.network.requestGithubApiDirectly"
              inset
              label="直接访问 GitHub api"
            ></v-switch>
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
      inited: false
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