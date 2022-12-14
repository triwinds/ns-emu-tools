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
                  <v-img src="@/assets/yuzu.webp" max-height="40" max-width="40" class="float-left"
                         style="margin-right: 15px"></v-img>
                  <p class="text-h4 primary--text float-left">
                    Yuzu 基础信息
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">当前使用的 Yuzu 分支：</span>
                  <v-tooltip right>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="error" large outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="switchYuzuBranch" :disabled='isRunningInstall'>
                        {{ displayBranch }} 版
                      </v-btn>
                    </template>
                    <span>切换安装分支</span>
                  </v-tooltip>
                </v-col>
              </v-row>
              <v-row>
                <v-col cols="7">
                  <v-text-field label="Yuzu 路径" readonly v-model="yuzuConfig.yuzu_path"
                                style="cursor: default"></v-text-field>
                </v-col>
                <v-col cols="5">
                  <v-btn large color="secondary" outlined style="margin-right: 5px" min-width="120px"
                         :disabled='isRunningInstall' @click="modifyYuzuPath">修改路径
                  </v-btn>
                  <v-btn large color="success" outlined min-width="120px" :disabled='isRunningInstall'
                         @click="startYuzu">启动 Yuzu
                  </v-btn>
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
                             @click="detectYuzuVersion" :disabled='isRunningInstall'>
                        {{ yuzuConfig.yuzu_version ? yuzuConfig.yuzu_version : '未知' }}
                      </v-btn>
                    </template>
                    <span>点击重新检测 Yuzu 版本</span>
                  </v-tooltip>
                  <span class="text-h6 secondary--text">
                    最新 Yuzu 版本：
                  </span>
                  <span class="text-h6">
                    {{ latestYuzuVersion }}
                  </span>
                </v-col>
              </v-row>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">当前固件版本：</span>
                  <v-tooltip top>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="warning" outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="detectFirmwareVersion" :disabled='isRunningInstall'>
                        {{ yuzuConfig.yuzu_firmware ? yuzuConfig.yuzu_firmware : '未知' }}
                      </v-btn>
                    </template>
                    <span>点击重新检测固件版本, 需安装密钥后使用</span>
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
                  <v-img src="@/assets/yuzu.webp" max-height="40" max-width="40" class="float-left"
                         style="margin-right: 15px"></v-img>
                  <p class="text-h4 primary--text float-left">
                    Yuzu 组件管理
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <v-row>
                <v-col cols="7">
                  <v-text-field label="需要安装的 Yuzu 版本" v-model="targetYuzuVersion"></v-text-field>
                </v-col>
                <v-col>
                  <v-btn class="info--text" large outlined min-width="120px" :disabled='isRunningInstall'
                         @click="installYuzu">
                    安装 Yuzu
                  </v-btn>
                </v-col>
              </v-row>
              <v-row>
                <v-col cols="7">
                  <v-autocomplete v-model="targetFirmwareVersion" label="需要安装的固件版本"
                                  :items="availableFirmwareVersions"></v-autocomplete>
                </v-col>
                <v-col>
                  <v-btn class="info--text" large outlined min-width="120px" :disabled='isRunningInstall'
                         @click="installFirmware">
                    安装固件
                  </v-btn>
                </v-col>
              </v-row>
              <v-row>
                <v-col>
                  <span>安装/更新固件后, 请一并安装相应的 keys:
                    <router-link to="/keys" class="info--text">密钥管理</router-link>
                  </span>
                </v-col>
              </v-row>
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
    allYuzuReleaseVersions: [],
    targetYuzuVersion: "",
    isRunningInstall: false,
  }),
  created() {
    this.updateYuzuReleaseVersions()
    window.eel.update_last_open_emu_page('yuzu')()
  },
  methods: {
    async updateYuzuConfig() {
      await this.$store.dispatch('loadConfig')
    },
    updateYuzuReleaseVersions() {
      window.eel.get_all_yuzu_release_versions()((data) => {
        if (data['code'] === 0) {
          let infos = data['data']
          this.allYuzuReleaseVersions = infos
          this.targetYuzuVersion = infos[0]
        } else {
          this.showConsoleDialog()
          this.appendConsoleMessage('yuzu 版本信息加载异常.')
        }
      })
    },
    async detectFirmwareVersion() {
      this.cleanAndShowConsoleDialog()
      window.eel.detect_firmware_version("yuzu")((data) => {
        if (data['code'] === 0) {
          this.updateYuzuConfig()
        }
        this.appendConsoleMessage('固件版本检测完成')
      })
    },
    async detectYuzuVersion() {
      this.cleanAndShowConsoleDialog()
      let previousBranch = this.branch
      let data = await window.eel.detect_yuzu_version()()
      if (data['code'] === 0) {
        await this.updateYuzuConfig()
        if (previousBranch !== this.branch) {
          this.updateYuzuReleaseVersions()
        }
        this.appendConsoleMessage('Yuzu 版本检测完成')
      } else {
        this.appendConsoleMessage('检测 yuzu 版本时发生异常')
      }
    },
    installYuzu() {
      this.cleanAndShowConsoleDialog()
      this.isRunningInstall = true
      window.eel.install_yuzu(this.targetYuzuVersion, this.branch)((resp) => {
        this.isRunningInstall = false
        if (resp['code'] === 0) {
          this.updateYuzuConfig()
          this.appendConsoleMessage(resp['msg'])
        } else {
          this.appendConsoleMessage(resp['msg'])
        }
      });
    },
    installFirmware() {
      this.cleanAndShowConsoleDialog()
      this.isRunningInstall = true
      window.eel.install_yuzu_firmware(this.targetFirmwareVersion)((resp) => {
        this.isRunningInstall = false
        if (resp['msg']) {
          this.appendConsoleMessage(resp['msg'])
        }
        this.updateYuzuConfig()
      })
    },
    modifyYuzuPath() {
      this.cleanAndShowConsoleDialog()
      this.appendConsoleMessage('=============================================')
      this.appendConsoleMessage('安装/更新模拟器时会删除目录下除模拟器用户数据外的其他文件')
      this.appendConsoleMessage('请确保您选择的目录下没有除模拟器外的其他文件')
      this.appendConsoleMessage('建议新建目录单独存放')
      this.appendConsoleMessage('=============================================')
      window.eel.ask_and_update_yuzu_path()((data) => {
        if (data['code'] === 0) {
          this.updateYuzuConfig()
        }
        this.appendConsoleMessage(data['msg'])
      })
    },
    startYuzu() {
      window.eel.start_yuzu()((data) => {
        if (data['code'] === 0) {
          this.appendConsoleMessage('yuzu 启动成功')
        } else {
          this.appendConsoleMessage('yuzu 启动失败')
        }
      })
    },
    async switchYuzuBranch() {
      await window.eel.switch_yuzu_branch()()
      await this.updateYuzuConfig()
      this.allYuzuReleaseVersions = []
      this.updateYuzuReleaseVersions()
    },
  },
  computed: {
    latestYuzuVersion: function () {
      if (this.allYuzuReleaseVersions.length > 0) {
        return this.allYuzuReleaseVersions[0]
      }
      return "加载中"
    },
    displayBranch: function () {
      if (this.branch === 'ea') {
        return 'EA'
      } else if (this.branch === 'mainline') {
        return '主线'
      }
      return '未知'
    },
    branch() {
      return this.$store.state.config.yuzu.branch
    },
  }
}
</script>

<style scoped>

</style>