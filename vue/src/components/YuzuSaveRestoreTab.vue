<template>
  <v-card flat>
    <YuzuSaveCommonPart/>
    <v-card-text v-if="backupList.length === 0" class="text-center text-h2">无备份存档</v-card-text>
    <div v-else>
      <span class="text-h5 primary--text" style="padding-left: 15px">备份存档</span>
      <v-virtual-scroll :items="backupList" :height="backupItemBoxHeight" item-height="106">
        <template v-slot:default="{ item }">
          <v-list-item three-line>
            <v-list-item-content>
              <v-list-item-title class="info--text">{{ concatGameName(item) }}</v-list-item-title>
              <v-list-item-subtitle>
                <v-icon size="20">{{ svgPath.mdiClockTimeNineOutline }}</v-icon>
                备份时间: {{ new Date(item.bak_time).toLocaleString() }}
              </v-list-item-subtitle>
              <v-list-item-subtitle>
                <v-icon size="20">{{ svgPath.mdiFileDocumentOutline }}</v-icon>
                文件名: {{ item.filename }}
              </v-list-item-subtitle>
            </v-list-item-content>
            <v-list-item-action>
              <v-btn outlined color="warning" @click="restoreBackup(item.path)">还原备份</v-btn>
              <v-btn outlined color="error" @click="deletePath(item.path)">删除备份</v-btn>
            </v-list-item-action>
          </v-list-item>
          <v-divider/>
        </template>
      </v-virtual-scroll>
    </div>
    <v-dialog v-model="restoreWaringDialog" max-width="600">
      <v-card>
        <v-card-title class="primary text--white text-h5">提示</v-card-title>
        <v-divider/>
        <v-card-text class="text--primary text-body-1" style="padding-top: 15px">
          还原时会先清空相应游戏的存档文件夹，然后再将备份的文件放入。<br/>是否继续还原存档？
        </v-card-text>
        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
              color="primary"
              text
              @click="restoreWaringDialog = false"
          >
            取消还原
          </v-btn>
          <v-btn
              color="primary"
              text
              @click="summitRestoreRequest"
          >
            执行还原
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-card>
</template>

<script>
import {mdiFileDocumentOutline, mdiClockTimeNineOutline} from '@mdi/js';
import YuzuSaveCommonPart from "@/components/YuzuSaveCommonPart";

export default {
  name: "YuzuSaveRestoreTab",
  components: {YuzuSaveCommonPart},
  data() {
    return {
      backupList: [],
      selectedUser: '',
      backupItemBoxHeight: window.innerHeight - 420,
      restoreWaringDialog: false,
      restoreBackupFile: '',
      svgPath: {
        mdiFileDocumentOutline,
        mdiClockTimeNineOutline
      }
    }
  },
  mounted() {
    this.loadAllYuzuBackups()
    this.$bus.$on('yuzuSave:selectedUser', newUser => {
      this.selectedUser = newUser
    })
    this.$bus.$on('yuzuSave:tabChange', tab => {
      if (tab === 1) {
        this.loadAllYuzuBackups()
      }
    })
    window.addEventListener('resize', this.updateBackupItemBoxHeight);
  },
  beforeDestroy() {
    this.$bus.$off('yuzuSave:selectedUser')
    this.$bus.$off('yuzuSave:tabChange')
    window.removeEventListener('resize', this.updateBackupItemBoxHeight);
  },
  methods: {
    concatGameName(item) {
      let gameName = item.game_name ? item.game_name : this.gameDataInited ?
          '未知游戏 - ' + item.title_id : '游戏信息加载中...'
      return gameName
    },
    loadAllYuzuBackups() {
      window.eel.list_all_yuzu_backups()(resp => {
        this.backupList = resp.data
        this.updateBackupItemBoxHeight()
        this.loadGameData().then(gameData => {
          let nl = []
          for (let item of this.backupList) {
            item.game_name = gameData[item.title_id]
            nl.push(item)
          }
          this.backupList = nl;
        })
      })
    },
    updateBackupItemBoxHeight() {
      let maxHeight = window.innerHeight - 420
      this.backupItemBoxHeight = Math.min(this.backupList.length * 106, maxHeight)
    },
    restoreBackup(filepath) {
      if (this.selectedUser === '') {
        this.cleanAndShowConsoleDialog()
        this.appendConsoleMessage('请先选择一个模拟器用户')
        return
      }
      this.restoreBackupFile = filepath
      this.restoreWaringDialog = true
    },
    summitRestoreRequest() {
      this.restoreWaringDialog = false
      this.cleanAndShowConsoleDialog()
      window.eel.restore_yuzu_save_from_backup(this.selectedUser, this.restoreBackupFile)()
    },
    deletePath(path) {
      window.eel.delete_path(path)(resp => {
        if (resp.code === 0) {
          this.loadAllYuzuBackups()
        }
      })
    },
  }
}
</script>

<style scoped>

</style>