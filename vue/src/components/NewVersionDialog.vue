<template>
  <div class="text-center">
    <v-dialog
      v-model="dialog"
      width="500"
    >
      <v-card>
        <v-card-title class="text-h5 primary">
          发现新版本
        </v-card-title>

        <v-card-text style="margin-top: 30px">
          <p class="text-h6 text--primary" v-show="$store.state.hasNewVersion">检测到新版本 [{{newVersion}}], 是否查看更新?</p>
          <p class="text-h6 text--primary" v-show="!$store.state.hasNewVersion">当前版本已经是最新版本</p>
        </v-card-text>

        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            text
            @click="dialog = false"
            v-show="$store.state.hasNewVersion"
          >
            取消
          </v-btn>
          <v-btn
            color="primary"
            text
            @click="openReleasePage"
          >
            好的
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script>
export default {
  name: "NewVersionDialog",
  data() {
    return {
      dialog: false,
      newVersion: '',
    }
  },
  mounted() {
    this.$bus.$on('showNewVersionDialog', this.showNewVersionDialog);
  },
  methods: {
    showNewVersionDialog(info) {
      this.dialog = true
      this.newVersion = info.latestVersion
    },
    openReleasePage() {
      if (this.$store.state.hasNewVersion) {
        this.openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
      }
    },
  }
}
</script>

<style scoped>

</style>