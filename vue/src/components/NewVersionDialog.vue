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
          <p class="text-h6 text--primary">检测到新版本 [{{newVersion}}], 是否查看更新?</p>
        </v-card-text>

        <v-divider></v-divider>

        <v-card-actions>
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
      newVersion: ''
    }
  },
  mounted() {
    this.$bus.$on('showNewVersionDialog', this.showNewVersionDialog);
  },
  methods: {
    showNewVersionDialog(newVersion) {
      this.dialog = true
      this.newVersion = newVersion
    },
    openReleasePage() {
      this.openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
    },
  }
}
</script>

<style scoped>

</style>