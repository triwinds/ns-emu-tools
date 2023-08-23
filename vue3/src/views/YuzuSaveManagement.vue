<template>
<SimplePage>
    <v-card>
      <v-card-title class="text-primary text-h4" style="margin-bottom: 10px; margin-top: 10px;">Yuzu 存档管理</v-card-title>
      <v-divider></v-divider>
      <div class="d-flex flex-row">
        <v-tabs v-model="tab" direction="vertical" color="primary">
        <v-tab value="backup">
          <v-icon>{{ mdiContentSaveAll }}</v-icon>
          <span style="padding-left: 15px">备份</span>
        </v-tab>
        <v-tab value="restore">
          <v-icon>{{ mdiBackupRestore }}</v-icon>
          <span style="padding-left: 15px">还原</span>
        </v-tab>
          </v-tabs>

        <v-window v-model="tab" style="width: 100%; height: 100%;">
          <v-window-item key="backup" value="backup">
            <v-card variant="flat">
              <YuzuSaveCommonPart/>
              <MarkdownContentBox v-if="!yuzuSaveStore.selectedUser || yuzuSaveStore.selectedUser === ''" :content="guide"/>
              <v-container v-else>
                <v-row>
                  <v-col>
                    <v-autocomplete v-model="selectedGameFolder" hide-details variant="underlined"
                                    :items="gameList" label="选择需要进行备份的游戏"
                                    :item-title="concatGameName" item-value="folder"/>
                  </v-col>
                </v-row>
                <v-row>
                  <v-col>
                    <v-btn color="success" variant="outlined" block @click="doBackup"
                           :disabled="selectedGameFolder === ''">创建备份</v-btn>
                  </v-col>
                </v-row>
              </v-container>
            </v-card>
          </v-window-item>

          <v-window-item key="restore" value="restore">
            <YuzuSaveRestoreTab/>
          </v-window-item>
        </v-window>
      </div>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import {ref, watch} from "vue";
import {mdiContentSaveAll, mdiBackupRestore} from '@mdi/js';
import {useYuzuSaveStore} from "@/store/YuzuSaveStore";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import YuzuSaveCommonPart from "@/components/YuzuSaveCommonPart.vue";
import {CommonResponse, SaveGameInfo} from "@/types";
import {useAppStore} from "@/store/app";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";
import YuzuSaveRestoreTab from "@/components/YuzuSaveRestoreTab.vue";
import {useEmitter} from "@/plugins/mitt";

let tab = ref('')
const guide = `
## 使用说明

Yuzu 模拟器在保存存档时会根据用户 id 选择不同的文件夹，因此需要先确认你正在使用的用户 id.

模拟器的用户 ID 可以在菜单 模拟->设置->系统->配置 中查看
[参考截图](https://cdn.jsdelivr.net/gh/triwinds/ns-emu-tools@main/doc/assets/yuzu_user_id.jpg).

点击备份会将选择的存档文件夹打包成一个 7z 压缩包放到指定的目录, 你也可以选择手动解压还原。
`
let selectedGameFolder = ref('')
let gameList = ref<SaveGameInfo[]>([])
const yuzuSaveStore = useYuzuSaveStore()
const appStore = useAppStore()
const cds = useConsoleDialogStore()
const emitter = useEmitter()
let lastUser: string = ''

yuzuSaveStore.$subscribe((mutation, state) => {
  if (state.selectedUser && lastUser != state.selectedUser) {
    reloadGameList()
    lastUser = state.selectedUser
  }
})

watch(tab, () => {
  emitter.emit('yuzuSave:tabChange', tab)
})

function reloadGameList() {
  window.eel.list_all_games_by_user_folder(yuzuSaveStore.selectedUser)((resp: CommonResponse) => {
    if (resp.code === 0) {
      gameList.value = resp.data
      if (appStore.gameDataInited) {
        enrichGameList()
      } else {
        appStore.loadGameData().then(() => {
          enrichGameList()
        })
      }
      if (gameList.value.length > 0) {
        selectedGameFolder.value = gameList.value[0].folder
      }
    }
  })
}

function enrichGameList() {
  let nl = []
  for (let item of gameList.value) {
    item.game_name = appStore.gameData[item.title_id]
    nl.push(item)
  }
  gameList.value = nl;
}

function concatGameName(item: SaveGameInfo) {
  let gameName = item.game_name ? item.game_name : appStore.gameDataInited ? '未知游戏' : '游戏信息加载中...'
  return `[${item.title_id}] ${gameName}`
}

function doBackup() {
  cds.cleanAndShowConsoleDialog()
  window.eel.backup_yuzu_save_folder(selectedGameFolder.value)()
}

</script>

<style scoped>

</style>
