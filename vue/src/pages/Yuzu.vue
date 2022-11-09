<template>
  <div>
    <v-container>
      <v-row>
        <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
        <v-col md="10" lg="8">
          <v-card class="mx-auto">
            <v-container>
              <v-row>
                <v-col>
                  <p class="text-h4 primary--text">
                    Yuzu 基础信息
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <v-row>
                <v-col cols="7">
                  <v-text-field label="Yuzu 路径" readonly v-model="yuzuConfig.yuzu_path"></v-text-field>
                </v-col>
                <v-col cols="5">
                  <v-btn large color="secondary" outlined style="margin-right: 5px" min-width="120px">修改路径</v-btn>
                  <v-btn large color="success" outlined min-width="120px">启动 Yuzu</v-btn>
                </v-col>
              </v-row>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">
                    当前 Yuzu 版本：
                  </span>
                  <v-tooltip top>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="warning" outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="detectYuzuVersion">
                        {{ yuzuConfig.yuzu_version }}
                      </v-btn>
                    </template>
                    <span>点击重新检测 Yuzu 版本</span>
                  </v-tooltip>
                  <span class="text-h6 secondary--text">
                    最新 Yuzu 版本：
                  </span>
                  <span class="text-h6">
                    {{latestYuzuVersion}}
                  </span>
                </v-col>
              </v-row>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">当前固件版本：</span>
                  <v-tooltip top>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="warning" outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="detectFirmwareVersion">
                        {{ yuzuConfig.yuzu_firmware }}
                      </v-btn>
                    </template>
                    <span>点击重新检测固件版本</span>
                  </v-tooltip>
                  <span class="text-h6 secondary--text">
                    最新固件版本：
                  </span>
                  <span class="text-h6">
                    {{ latestFirmwareVersion }}
                  </span>
                </v-col>
              </v-row>
            </v-container>
          </v-card>
        </v-col>
        <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
      </v-row>
      <v-row>
        <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
        <v-col md="10" lg="8">
          <v-card class="mx-auto">
            <v-container>
              <v-row>
                <v-col>
                  <p class="text-h4 primary--text">
                    Yuzu 组件管理
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>

            </v-container>
          </v-card>
        </v-col>
        <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
      </v-row>
    </v-container>
  </div>
</template>

<script>
export default {
  name: "YuzuPage",
  data: () => ({
    yuzuConfig: {
      yuzu_path: '',
      yuzu_version: '',
      yuzu_firmware: '',
      branch: '',
    },
    branch: 'ea',
    allYuzuReleaseVersions: [],
    availableFirmwareInfos: [],
  }),
  created() {
    this.updateYuzuConfig()
    this.updateYuzuReleaseVersions()
    this.updateAvailableFirmwareInfos()
  },
  methods: {
    async updateYuzuConfig() {
      this.yuzuConfig = await window.eel.get_yuzu_config()()
      this.branch = this.yuzuConfig.branch
    },
    updateYuzuReleaseVersions() {
      window.eel.get_all_yuzu_release_versions()((data) => {
        if (data['code'] === 0) {
          let infos = data['data']
          this.allYuzuReleaseVersions = infos
          this.targetYuzuVersion = infos[0]
        } else {
          this.topBarMsg = 'yuzu 版本信息加载异常.'
        }
      })
    },
    updateAvailableFirmwareInfos() {
      window.eel.get_available_firmware_infos()((data) => {
        if (data['code'] === 0) {
          let infos = data['data']
          this.availableFirmwareInfos = infos
          this.targetFirmwareVersion = infos[0]['version']
        } else {
          this.topBarMsg = '固件信息加载异常.'
        }
      })
    },
    async detectFirmwareVersion() {
      window.eel.detect_firmware_version("yuzu")((data) => {
        if (data['code'] === 0) {
          this.updateYuzuConfig()
        }
      })
    },
    async detectYuzuVersion() {
      let previousBranch = this.branch
      let data = await window.eel.detect_yuzu_version()()
      if (data['code'] === 0) {
        await this.updateYuzuConfig()
        if (previousBranch !== this.branch) {
          this.updateYuzuReleaseVersions()
        }
      } else {
        this.topBarMsg = '检测 yuzu 版本时发生异常'
      }
    },
  },
  computed: {
    latestYuzuVersion: function () {
      if (this.allYuzuReleaseVersions.length > 0) {
        return this.allYuzuReleaseVersions[0]
      }
      return "加载中"
    },
    latestFirmwareVersion: function () {
      if (this.availableFirmwareInfos.length > 0) {
        return this.availableFirmwareInfos[0]['version']
      }
      return "加载中"
    },
  }
}
</script>

<style scoped>

</style>