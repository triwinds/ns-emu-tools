<template>
  <SimplePage>
    <v-card class="mx-auto" style="margin-bottom: 10px">
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
            <v-select outlined v-model="selectedBranch" :items="availableBranch"
                      @change="switchRyujinxBranch" color="error" item-color="error"
                      label="当前使用的 Ryujinx 分支"></v-select>
          </v-col>
        </v-row>
        <v-row>
          <v-col cols="7">
            <v-autocomplete label="Ryujinx 路径" v-model="selectedRyujinxPath" :items="historyPathList"
                            @change="updateRyujinxPath"
                            style="cursor: default"></v-autocomplete>
          </v-col>
          <v-col cols="5">
            <v-btn large color="secondary" outlined style="margin-right: 5px" min-width="120px"
                   :disabled='isRunningInstall' @click="askAndUpdateRyujinxPath">修改路径
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
            <ChangeLogDialog v-if="selectedBranch === 'ava' || selectedBranch === 'mainline'">
              <template v-slot:activator="{on, attrs}">
                      <span v-bind="attrs" v-on="on" @click="loadChangeLog"
                            style="margin-left: 10px">
                        <v-icon color="warning">
                          {{ svgPath.mdiTimelineQuestionOutline }}
                        </v-icon>
                      </span>
              </template>
              <template v-slot:content>
                <div class="text--primary" v-html="changeLogHtml"></div>
              </template>
            </ChangeLogDialog>
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
  </SimplePage>
</template>

<script>
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import * as showdown from "showdown";
import {mdiTimelineQuestionOutline} from '@mdi/js';
import SimplePage from "@/components/SimplePage";


export default {
  name: "RyujinxPage",
  components: {SimplePage, ChangeLogDialog},
  data: () => ({
    allRyujinxReleaseInfos: [],
    availableFirmwareInfos: [],
    historyPathList: [],
    selectedRyujinxPath: '',
    targetRyujinxVersion: "",
    isRunningInstall: false,
    changeLogHtml: '<p>加载中...</p>',
    availableBranch: [
      {
        text: '正式版 (老 UI)',
        value: 'mainline'
      }, {
        text: 'AVA 版 (新 UI)',
        value: 'ava'
      }, {
        text: 'LDN 版 (联机版本)',
        value: 'ldn'
      },
    ],
    selectedBranch: '',
    svgPath: {
      mdiTimelineQuestionOutline
    }
  }),
  mounted() {
    this.updateRyujinxReleaseInfos()
    this.updateRyujinxConfig()
    this.loadHistoryPathList().then(() => {
      this.selectedRyujinxPath = this.ryujinxConfig.path
    })
    this.selectedBranch = this.ryujinxConfig.branch
    window.eel.update_last_open_emu_page('ryujinx')()
  },
  methods: {
    updateRyujinxReleaseInfos() {
      this.allRyujinxReleaseInfos = []
      this.targetRyujinxVersion = ""
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
    async updateRyujinxPath() {
      await window.eel.update_ryujinx_path(this.selectedRyujinxPath)()
      let oldBranch = this.ryujinxConfig.branch
      await this.updateRyujinxConfig()
      this.selectedRyujinxPath = this.ryujinxConfig.path
      await this.loadHistoryPathList()
      if (oldBranch !== this.selectedRyujinxPath.branch) {
        this.updateRyujinxReleaseInfos()
      }
    },
    async loadHistoryPathList() {
      let data = await window.eel.load_history_path('ryujinx')()
      if (data.code === 0) {
        this.historyPathList = data.data
      }
    },
    detectRyujinxVersion() {
      this.cleanAndShowConsoleDialog()
      window.eel.detect_ryujinx_version()((data) => {
        if (data['code'] === 0) {
          this.updateRyujinxConfig()
          this.updateRyujinxReleaseInfos()
          this.appendConsoleMessage('Ryujinx 版本检测完成')
        } else {
          this.appendConsoleMessage('检测 Ryujinx 版本时发生异常')
        }
      })
    },
    installRyujinx() {
      this.cleanAndShowConsoleDialog()
      this.isRunningInstall = true
      window.eel.install_ryujinx(this.targetRyujinxVersion, this.selectedBranch)((resp) => {
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
    async askAndUpdateRyujinxPath() {
      this.cleanAndShowConsoleDialog()
      this.appendConsoleMessage('=============================================')
      this.appendConsoleMessage('安装/更新模拟器时会删除目录下除模拟器用户数据外的其他文件')
      this.appendConsoleMessage('请确保您选择的目录下没有除模拟器外的其他文件')
      this.appendConsoleMessage('建议新建目录单独存放')
      this.appendConsoleMessage('=============================================')
      let data = await window.eel.ask_and_update_ryujinx_path()();
      if (data['code'] === 0) {
        let oldBranch = this.ryujinxConfig.branch
        this.updateRyujinxConfig()
        if (oldBranch !== this.ryujinxConfig.branch) {
          this.updateRyujinxReleaseInfos()
        }
        await this.loadHistoryPathList()
        this.selectedRyujinxPath = this.ryujinxConfig.path
      }
      this.appendConsoleMessage(data['msg'])
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
      await window.eel.switch_ryujinx_branch(this.selectedBranch)()
      await this.updateRyujinxConfig()
      await this.updateRyujinxReleaseInfos()
      // console.log(this.selectedBranch)
    },
    async updateRyujinxConfig() {
      await this.$store.dispatch('loadConfig')
      this.selectedBranch = this.ryujinxConfig.branch
    },
    loadChangeLog() {
      window.eel.load_ryujinx_change_log()((resp) => {
        if (resp.code === 0) {
          const converter = new showdown.Converter()
          this.changeLogHtml = converter.makeHtml(resp.data)
        } else {
          this.changeLogHtml = '<p>加载失败。</p>'
        }
      })
    },
  },
  computed: {
    latestRyujinxVersion: function () {
      if (this.allRyujinxReleaseInfos.length > 0) {
        return this.allRyujinxReleaseInfos[0]['tag_name']
      }
      return "加载中"
    },
  }
}
</script>

<style scoped>

</style>