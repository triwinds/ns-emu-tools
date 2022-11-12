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
                  <v-img src="@/assets/ryujinx.webp" max-height="40" max-width="40" class="float-left"
                         style="margin-right: 15px"></v-img>
                  <p class="text-h4 primary--text float-left">
                    Ryujinx 基础信息
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">当前使用的 Ryujinx 分支：</span>
                  <v-tooltip right>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="error" large outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="switchRyujinxBranch" :disabled='isRunningInstall'>
                        {{ displayBranch }} 版
                      </v-btn>
                    </template>
                    <span>切换安装分支</span>
                  </v-tooltip>
                </v-col>
              </v-row>
              <v-row>
                <v-col cols="7">
                  <v-text-field label="Ryujinx 路径" readonly v-model="ryujinxConfig.path"
                                style="cursor: default"></v-text-field>
                </v-col>
                <v-col cols="5">
                  <v-btn large color="secondary" outlined style="margin-right: 5px" min-width="120px"
                         :disabled='isRunningInstall' @click="modifyRyujinxPath">修改路径
                  </v-btn>
                  <v-btn large color="success" outlined min-width="120px" :disabled='isRunningInstall'
                         @click="startRyujinx">启动龙神
                  </v-btn>
                </v-col>
              </v-row>
              <v-row>
                <v-col>
                  <span class="text-h6 secondary--text">
                    当前 Ryujinx 版本：
                  </span>
                  <v-tooltip top>
                    <template v-slot:activator="{ on, attrs }">
                      <v-btn color="warning" outlined style="margin-right: 15px" v-bind="attrs" v-on="on"
                             @click="detectRyujinxVersion" :disabled='isRunningInstall'>
                        {{ ryujinxConfig.version ? ryujinxConfig.version : "未知" }}
                      </v-btn>
                    </template>
                    <span>点击重新检测 Ryujinx 版本</span>
                  </v-tooltip>
                  <span class="text-h6 secondary--text">
                    最新 Ryujinx 版本：
                  </span>
                  <span class="text-h6">
                    {{ latestRyujinxVersion }}
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
                        {{ ryujinxConfig.firmware ? ryujinxConfig.firmware : "未知" }}
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
                  <v-img src="@/assets/ryujinx.webp" max-height="40" max-width="40" class="float-left"
                         style="margin-right: 15px"></v-img>
                  <p class="text-h4 primary--text float-left">
                    Ryujinx 组件管理
                  </p>
                </v-col>
              </v-row>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <v-row>
                <v-col cols="7">
                  <v-text-field label="需要安装的 Ryujinx 版本" v-model="targetRyujinxVersion"></v-text-field>
                </v-col>
                <v-col>
                  <v-btn class="info--text" large outlined min-width="160px" :disabled='isRunningInstall'
                         @click="installRyujinx">
                    安装 Ryujinx
                  </v-btn>
                </v-col>
              </v-row>
              <v-row>
                <v-col cols="7">
                  <v-autocomplete v-model="targetFirmwareVersion" label="需要安装的固件版本"
                                  :items="availableFirmwareVersions"></v-autocomplete>
                </v-col>
                <v-col>
                  <v-btn class="info--text" large outlined min-width="160px" :disabled='isRunningInstall'
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
  name: "RyujinxPage",
  data: () => ({
    allRyujinxReleaseInfos: [],
    availableFirmwareInfos: [],
    targetRyujinxVersion: "",
    isRunningInstall: false,
  }),
  mounted() {
    this.updateRyujinxReleaseInfos()
  },
  methods: {
    updateRyujinxReleaseInfos() {
      window.eel.get_ryujinx_release_infos()((data) => {
        if (data['code'] === 0) {
          let infos = data['data']
          this.allRyujinxReleaseInfos = infos
          this.targetRyujinxVersion = infos[0]['tag_name']
        } else {
          this.appendConsoleMessage('ryujinx 版本信息加载异常.')
        }
      })
    },
    detectRyujinxVersion() {
      this.cleanAndShowConsoleDialog()
      window.eel.detect_ryujinx_version()((data) => {
        if (data['code'] === 0) {
          this.updateRyujinxConfig()
        } else {
          this.appendConsoleMessage('检测 Ryujinx 版本时发生异常')
        }
      })
    },
    installRyujinx() {
      this.cleanAndShowConsoleDialog()
      this.isRunningInstall = true
      window.eel.install_ryujinx(this.targetRyujinxVersion, this.branch)((resp) => {
        this.isRunningInstall = false
        this.appendConsoleMessage(resp['msg'])
        if (resp['code'] === 0) {
          this.updateRyujinxConfig()
        }
      });
    },
    installFirmware() {
      this.cleanAndShowConsoleDialog()
      this.isRunningInstall = true
      window.eel.install_ryujinx_firmware(this.targetFirmwareVersion)((resp) => {
        this.isRunningInstall = false
        this.appendConsoleMessage(resp['msg'])
        if (resp['code'] === 0) {
          this.updateRyujinxConfig()
        }
      })
    },
    modifyRyujinxPath() {
      window.eel.ask_and_update_ryujinx_path()((data) => {
        if (data['code'] === 0) {
          this.updateRyujinxConfig()
        }
        this.appendConsoleMessage(data['msg'])
      })
    },
    startRyujinx() {
      window.eel.start_ryujinx()((data) => {
        if (data['code'] === 0) {
          this.appendConsoleMessage('Ryujinx 启动成功')
        } else {
          this.appendConsoleMessage('Ryujinx 启动失败')
        }
      })
    },
    async detectFirmwareVersion() {
      this.cleanAndShowConsoleDialog()
      window.eel.detect_firmware_version("ryujinx")((data) => {
        if (data['code'] === 0) {
          this.updateRyujinxConfig()
        }
      })
    },
    async switchRyujinxBranch() {
      await window.eel.switch_ryujinx_branch()()
      await this.updateRyujinxConfig()
    },
    async updateRyujinxConfig() {
      await this.$store.dispatch('loadConfig')
    }
  },
  computed: {
    latestRyujinxVersion: function () {
      if (this.allRyujinxReleaseInfos.length > 0) {
        return this.allRyujinxReleaseInfos[0]['tag_name']
      }
      return "加载中"
    },
    displayBranch: function () {
      if (this.branch === 'ava') {
        return 'ava'
      } else if (this.branch === 'mainline') {
        return '正式'
      }
      return '未知'
    },
    branch: function (){
      return this.ryujinxConfig.branch
    },
  }
}
</script>

<style scoped>

</style>