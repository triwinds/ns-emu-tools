<template>
  <div class="text-center">
    <v-dialog
      v-model="dialog"
      width="700"
    >
      <v-card>
        <v-card-title class="text-h5 primary white--text">
          版本检测
        </v-card-title>

        <v-card-text style="margin-top: 30px">
          <p class="text-h6 text--primary" v-show="!$store.state.hasNewVersion">当前版本已经是最新版本</p>
          <div v-show="$store.state.hasNewVersion" >
            <p class="text-h6 text--primary">[{{newVersion}}] 更新内容:</p>
            <div v-html="releaseDescriptionHtml" class="info--text" style="font-size: 16px"></div>
          </div>
        </v-card-text>

        <v-divider></v-divider>

        <v-card-actions v-show="!$store.state.hasNewVersion">
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            text
            @click="dialog = false"
          >
            OK
          </v-btn>
        </v-card-actions>
        <v-card-actions v-show="$store.state.hasNewVersion">
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            text
            @click="dialog = false"
          >
            取消
          </v-btn>
          <v-btn
            color="primary"
            text
            @click="openReleasePage"
          >
            前往发布页
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script>
import * as showdown from 'showdown';

export default {
  name: "NewVersionDialog",
  data() {
    return {
      dialog: false,
      newVersion: '',
      releaseDescriptionHtml: '<p>加载中</p>',
    }
  },
  mounted() {
    this.$bus.$on('showNewVersionDialog', this.showNewVersionDialog);
  },
  methods: {
    showNewVersionDialog(info) {
      this.dialog = true
      this.newVersion = info.latestVersion
      console.log(this.$store.state.hasNewVersion)
      if (this.$store.state.hasNewVersion) {
        this.loadReleaseDescription()
      }
    },
    openReleasePage() {
      this.dialog = false
      if (this.$store.state.hasNewVersion) {
        this.openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
      }
    },
    loadReleaseDescription() {
      window.eel.get_net_release_info_by_tag(this.newVersion)((resp) => {
        if (resp.code === 0) {
          const converter = new showdown.Converter()
          this.releaseDescriptionHtml = converter.makeHtml(resp.data.body)
        } else {
          this.releaseDescriptionHtml = '<p>加载失败</p>'
        }
      })
    },
  }
}
</script>

<style scoped>

</style>