<template>
  <SimplePage>
    <v-card style="height: 100%">
      <v-card-title class="text-h4 primary--text">
        Yuzu 金手指管理
      </v-card-title>
      <v-divider></v-divider>
      <v-container>
        <v-row>
          <v-col>
            <v-select
              :items="cheatsFolders"
              v-model="selectedFolder"
              :item-text="concatFolderItemName"
              item-value="cheats_path"
              label="选择游戏 mod 目录"
            ></v-select>
          </v-col>
        </v-row>
        <v-row v-show="selectedFolder !== ''">
          <v-col>
            <v-select
              :items="cheatFiles"
              v-model="selectedCheatFile"
              label="选择金手指文件"
              item-text="name"
              item-value="path"
            ></v-select>
          </v-col>
        </v-row>
        <v-row v-show="selectedCheatFile !== ''">
          <v-col>
            <v-btn block outlined color="success" @click="saveSelectedCheats">保存设定</v-btn>
          </v-col>
          <v-col>
            <v-btn block outlined color="info" @click="openCheatModFolder">打开 Mod 文件夹</v-btn>
          </v-col>
        </v-row>
        <v-row v-show="selectedCheatFile !== ''">
          <v-col>
            <v-virtual-scroll
              :items="cheatItems"
              :item-height="35"
              :height="cheatItemBoxHeight"
            >
              <template v-slot:default="{ item }">
                <v-list-item>
                  <v-list-item-action>
                    <v-checkbox
                      v-model="item.enable"
                      :label="item.title"
                    >
                    </v-checkbox>
                  </v-list-item-action>
                </v-list-item>
              </template>
            </v-virtual-scroll>

          </v-col>
        </v-row>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script>
import SimplePage from "@/components/SimplePage";

export default {
  name: "YuzuCheatsManagement",
  components: {SimplePage},
  data() {
    return {
      selected: false,
      cheatsFolders: [],
      selectedFolder: '',
      cheatFiles: [],
      selectedCheatFile: '',
      cheatItems: [],
      cheatItemBoxHeight: 340,
    }
  },
  async created() {
    this.$store.commit('CLEAR_CONSOLE_MESSAGES', true)
    await this.scanCheatsFolders()
  },
  mounted() {
    this.updateCheatItemBoxHeight()
    window.addEventListener('resize', this.updateCheatItemBoxHeight);
  },
  beforeDestroy() {
    window.removeEventListener('resize', this.updateCheatItemBoxHeight);
  },
  methods: {
    async scanCheatsFolders() {
      let resp = await window.eel.scan_all_cheats_folder()()
      if (resp.code === 0 && resp.data) {
        this.cheatsFolders = resp.data
        return this.cheatsFolders
      }
      return []
    },
    updateCheatItemBoxHeight() {
      this.cheatItemBoxHeight = window.innerHeight - 510
    },
    concatFolderItemName(item) {
      return `[${item.game_id}] ${item.game_name ? item.game_name : '未能识别的游戏'}`
    },
    listAllCheatFilesFromFolder(selectedFolder) {
      window.eel.list_all_cheat_files_from_folder(selectedFolder)((resp) => {
        if (resp.code === 0 && resp.data) {
          this.cheatFiles = resp.data
          this.selectedCheatFile = resp.data[0].path
        } else {
          this.cheatsFolders = []
          this.selectedCheatFile = ''
        }
      })
    },
    loadCheatChunkInfo(selectedCheatFile) {
      window.eel.load_cheat_chunk_info(selectedCheatFile)((resp) => {
        if (resp.code === 0 && resp.data) {
          this.cheatItems = resp.data
        }
      })
      // console.log(selectedCheatFile)
      // let test = []
      // for (let i = 0; i < 100; i++) {
      //   test.push({title: "title " + i, enable: true})
      // }
      // this.cheatItems = test
    },
    saveSelectedCheats() {
      if (!this.cheatItems) {
        return
      }
      let enabledTitles = this.cheatItems.filter(d=>d.enable).map(d=>d.title)
      window.eel.update_current_cheats(enabledTitles, this.selectedCheatFile)((resp) => {
        if (resp.code === 0) {
          this.appendConsoleMessage('保存成功')
          this.showConsoleDialog()
        }
      })
    },
    openCheatModFolder() {
      window.eel.open_cheat_mod_folder(this.selectedFolder)((resp) => {
        if (resp.code === 0) {
          this.appendConsoleMessage("打开文件夹成功")
        }
      })
    }
  },
  watch: {
    selectedFolder: {
      immediate: false,
      handler(newValue) {
        if (newValue && newValue.length > 0) {
          this.listAllCheatFilesFromFolder(newValue)
        }
      }
    },
    selectedCheatFile: {
      immediate: false,
      handler(newValue) {
        if (newValue && newValue.length > 0) {
          this.loadCheatChunkInfo(newValue)
        }
      }
    },
  }
}
</script>

<style scoped>

</style>