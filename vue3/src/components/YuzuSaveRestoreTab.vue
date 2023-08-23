<template>
  <v-card variant="flat" style="min-height: 400px">
    <YuzuSaveCommonPart/>
    <v-card-text v-if="backupList.length === 0" class="text-center text-h2">无备份存档</v-card-text>
    <div v-else>
      <span class="text-h5 text-primary" style="padding-left: 15px">备份存档</span>
      <v-virtual-scroll :items="backupList" :height="backupItemBoxHeight" item-height="106">
        <template v-slot:default="{ item }">
          <v-list-item three-line>
            <v-list-item-title class="text-info">{{ concatGameName(item) }}</v-list-item-title>
            <v-list-item-subtitle>
              <v-icon size="20">{{ mdiClockTimeNineOutline }}</v-icon>
              备份时间: {{ new Date(item.bak_time).toLocaleString() }}
            </v-list-item-subtitle>
            <v-list-item-subtitle>
              <v-icon size="20">{{ mdiFileDocumentOutline }}</v-icon>
              文件名: {{ item.filename }}
            </v-list-item-subtitle>
            <template v-slot:append>
              <div>
                <v-btn variant="outlined" color="warning" @click="restoreBackup(item.path)">还原备份</v-btn>
                <br/>
                <v-btn variant="outlined" color="error" @click="deletePath(item.path)">删除备份</v-btn>
              </div>
            </template>
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
              variant="text"
              @click="restoreWaringDialog = false"
          >
            取消还原
          </v-btn>
          <v-btn
              color="primary"
              variant="text"
              @click="summitRestoreRequest"
          >
            执行还原
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-card>
</template>

<script setup lang="ts">
import {mdiClockTimeNineOutline, mdiFileDocumentOutline} from '@mdi/js';
import {onMounted, onUnmounted, ref} from "vue";
import {useAppStore} from "@/store/app";
import {useYuzuSaveStore} from "@/store/YuzuSaveStore";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import {CommonResponse, YuzuSaveBackupListItem} from "@/types";
import {useEmitter} from "@/plugins/mitt";
import YuzuSaveCommonPart from "@/components/YuzuSaveCommonPart.vue";

let backupList = ref<YuzuSaveBackupListItem[]>([])
let backupItemBoxHeight = ref(window.innerHeight - 420)
let restoreWaringDialog = ref(false)
let restoreBackupFile = ref('')
const appStore = useAppStore()
const yuzuSaveStore = useYuzuSaveStore()
const cds = useConsoleDialogStore()
const emitter = useEmitter()

onMounted(() => {
  loadAllYuzuBackups()
  window.addEventListener('resize', updateBackupItemBoxHeight);
  emitter.on('yuzuSave:tabChange', (tab) => {
    loadAllYuzuBackups()
  })
})

onUnmounted(() => {
  window.removeEventListener('resize', updateBackupItemBoxHeight)
  emitter.off('yuzuSave:tabChange')
})

function loadAllYuzuBackups() {
  window.eel.list_all_yuzu_backups()((resp: CommonResponse) => {
    backupList.value = resp.data
    updateBackupItemBoxHeight()
    appStore.loadGameData().then(gameData => {
      let nl = []
      for (let item of backupList.value) {
        item.game_name = gameData[item.title_id]
        nl.push(item)
      }
      backupList.value = nl;
    })
  })
}

function updateBackupItemBoxHeight() {
  let maxHeight = window.innerHeight - 420
  backupItemBoxHeight.value = Math.min(backupList.value.length * 106, maxHeight)
}

function restoreBackup(filepath: string) {
  if (yuzuSaveStore.selectedUser === '') {
    cds.cleanAndShowConsoleDialog()
    cds.appendConsoleMessage('请先选择一个模拟器用户')
    return
  }
  restoreBackupFile.value = filepath
  restoreWaringDialog.value = true
}

function summitRestoreRequest() {
  restoreWaringDialog.value = false
  cds.cleanAndShowConsoleDialog()
  window.eel.restore_yuzu_save_from_backup(yuzuSaveStore.selectedUser, restoreBackupFile.value)()
}

function deletePath(path: string) {
  window.eel.delete_path(path)((resp: CommonResponse) => {
    if (resp.code === 0) {
      loadAllYuzuBackups()
    }
  })
}

function concatGameName(item: YuzuSaveBackupListItem) {
  let gameName = item.game_name ? item.game_name : appStore.gameDataInited ?
      '未知游戏 - ' + item.title_id : '游戏信息加载中...'
  return gameName
}
</script>

<style scoped>

</style>
