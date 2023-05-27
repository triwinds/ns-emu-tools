<template>
  <SimplePage>
    <v-card>
      <v-card-title class="primary--text text-h4">Yuzu 存档管理</v-card-title>
      <v-divider></v-divider>
      <v-tabs v-model="tab" vertical>
        <v-tab key="backup">
          <v-icon>{{ svgPath.mdiContentSaveAll }}</v-icon>
          <span style="padding-left: 15px">备份</span>
        </v-tab>
        <v-tab key="restore">
          <v-icon>{{ svgPath.mdiBackupRestore }}</v-icon>
          <span style="padding-left: 15px">还原</span>
        </v-tab>

        <v-tabs-items v-model="tab">
          <v-tab-item key="backup">
            <v-card flat>
              <YuzuSaveCommonPart/>
              <MarkdownContentBox v-if="selectedUser === ''" :content="guide"/>
              <v-container v-if="selectedUser !== ''">
                <v-row>
                  <v-col>
                    <v-autocomplete v-model="selectedGameFolder" hide-details
                                    :items="gameList" label="选择需要进行备份的游戏"
                                    :item-text="concatGameName" item-value="folder"/>
                  </v-col>
                </v-row>
                <v-row>
                  <v-col>
                    <v-btn color="success" block outlined @click="doBackup"
                           :disabled="selectedGameFolder === ''">创建备份</v-btn>
                  </v-col>
                </v-row>
              </v-container>
            </v-card>
          </v-tab-item>

          <v-tab-item key="restore">
            <YuzuSaveRestoreTab/>
          </v-tab-item>

        </v-tabs-items>
      </v-tabs>

    </v-card>
  </SimplePage>
</template>

<script>
import SimplePage from "@/components/SimplePage";
import {mdiContentSaveAll, mdiBackupRestore} from '@mdi/js';
import MarkdownContentBox from "@/components/MarkdownContentBox";
import YuzuSaveCommonPart from "@/components/YuzuSaveCommonPart";
import YuzuSaveRestoreTab from "@/components/YuzuSaveRestoreTab";

const guide = `
## 使用说明

Yuzu 模拟器在保存存档时会根据用户 id 选择不同的文件夹，因此需要先确认你正在使用的用户 id.

模拟器的用户 ID 可以在菜单 模拟->设置->系统->配置 中查看
[参考截图](https://cdn.jsdelivr.net/gh/triwinds/ns-emu-tools@main/doc/assets/yuzu_user_id.jpg).

点击备份会将选择的存档文件夹打包成一个 7z 压缩包放到指定的目录, 你也可以选择手动解压还原。
`

export default {
  name: "YuzuSaveManagement",
  components: {YuzuSaveRestoreTab, YuzuSaveCommonPart, MarkdownContentBox, SimplePage},
  data() {
    return {
      tab: '',
      guide,
      gameList: [],
      selectedGameFolder: '',
      selectedUser: '',
      svgPath: {
        mdiContentSaveAll,
        mdiBackupRestore
      }
    }
  },
  mounted() {
    this.$bus.$on('yuzuSave:selectedUser', newUser => {
      this.selectedUser = newUser
      this.reloadGameList()
    })
  },
  beforeDestroy() {
    this.$bus.$off('yuzuSave:selectedUser')
  },
  methods: {
    reloadGameList() {
      window.eel.list_all_games_by_user_folder(this.selectedUser)(resp => {
        if (resp.code === 0) {
          this.gameList = resp.data
          if (this.gameDataInited) {
            this.enrichGameList()
          } else {
            this.loadGameData().then(() => {
              this.enrichGameList()
            })
          }
        }
      })
    },
    enrichGameList() {
      let nl = []
      for (let item of this.gameList) {
        item.game_name = this.$store.state.gameData[item.title_id]
        nl.push(item)
      }
      this.gameList = nl;
    },
    concatGameName(item) {
      let gameName = item.game_name ? item.game_name : this.gameDataInited ? '未知游戏' : '游戏信息加载中...'
      return `[${item.title_id}] ${gameName}`
    },
    doBackup() {
      this.cleanAndShowConsoleDialog()
      window.eel.backup_yuzu_save_folder(this.selectedGameFolder)()
    },
  },
  watch: {
    tab(val) {
      this.$bus.$emit('yuzuSave:tabChange', val)
    }
  }
}
</script>

<style scoped>

</style>