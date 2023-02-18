<template>
  <div class="text-center">
    <v-dialog
      v-model="dialog"
      width="850"
    >
      <v-card>
        <v-card-title class="text-h5 primary white--text">
          {{ $store.state.hasNewVersion ? '更新日志' : '版本检测' }}
        </v-card-title>

        <div style="padding: 15px;">
          <p class="text-h6 text--primary" v-show="!$store.state.hasNewVersion">当前版本已经是最新版本</p>
          <div v-show="$store.state.hasNewVersion" >
<!--            <p class="text-h6 text&#45;&#45;primary">[{{newVersion}}] 更新内容:</p>-->
            <div v-html="releaseDescriptionHtml" class="text--primary"
                 style="max-height: 300px; overflow-y: auto"></div>
          </div>
        </div>

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
            @click="updateNET"
          >
            自动更新
          </v-btn>
          <v-btn
            color="primary"
            text
            @click="downloadNET"
          >
            下载最新版本
          </v-btn>
          <v-btn
            color="primary"
            text
            @click="openReleasePage"
          >
            前往发布页
          </v-btn>
          <v-btn
            color="primary"
            text
            @click="dialog = false"
          >
            取消
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
      window.eel.load_change_log()((resp) => {
        console.log(resp.data)
        if (resp.code === 0) {
          const converter = new showdown.Converter()
          let rawMd = resp.data.replace('# Change Log\n\n', '')
          this.releaseDescriptionHtml = converter.makeHtml(rawMd)
        } else {
          this.releaseDescriptionHtml = '<p>加载失败</p>'
        }
      })
    },
    downloadNET() {
      this.cleanAndShowConsoleDialog()
      window.eel.download_net_by_tag(this.newVersion)((resp) => {
        if (resp.code === 0) {
          this.appendConsoleMessage('NET 下载完成')
        } else {
          this.appendConsoleMessage(resp.msg)
          this.appendConsoleMessage('NET 下载失败')
        }
      })
    },
    async updateNET() {
      this.cleanAndShowConsoleDialog()
      window.eel.update_net_by_tag(this.newVersion)
    }
  }
}
</script>

<style scoped>

</style>