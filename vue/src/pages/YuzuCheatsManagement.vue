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
              hide-details
              :disabled="!cheatsInited"
            ></v-select>
          </v-col>
        </v-row>
        <div style="padding: 30px" v-if="selectedFolder === ''">
          <div class="body-1 text--primary" v-html="descriptionHtml"></div>
        </div>
        <v-row v-show="selectedFolder !== ''">
          <v-col>
            <v-select
              :items="cheatFiles"
              v-model="selectedCheatFile"
              label="选择金手指文件"
              item-text="name"
              item-value="path"
              hide-details
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
import * as showdown from 'showdown'

let mdDescription = `
这个模块是对 Yuzu 金手指功能的一个补充，目标是实现类似 Ryujinx 对金手指中单个作弊项进行开关的功能.

在使用前请先阅读以下说明：

1. 请先确认金手指文件已经可以在 Yuzu 中被识别，如果 Yuzu 不能识别你的金手指，那么这里也不能
2. 如果不清楚如何在 Yuzu 中添加金手指，可以在 B 站上搜索相关教程
3. 某些金手指中的前面的一些公共的作弊项是必须启用的，请不要关闭这些作弊项（这些项目一般都在文件的最上面
4. 修改后需要重启游戏才会生效
5. 点击保存时会自动备份原来的金手指文件，如果出现问题，可以自行用这些备份文件来还原
`

export default {
  name: "YuzuCheatsManagement",
  components: {SimplePage},
  data() {
    return {
      cheatsInited: false,
      cheatsFolders: [],
      selectedFolder: '',
      cheatFiles: [],
      selectedCheatFile: '',
      cheatItems: [],
      cheatItemBoxHeight: 410,
      descriptionHtml: '',
      gameDataInited: false,
    }
  },
  async created() {
    this.$store.commit('CLEAR_CONSOLE_MESSAGES', true)
    await this.scanCheatsFolders()
  },
  mounted() {
    this.updateCheatItemBoxHeight()
    window.addEventListener('resize', this.updateCheatItemBoxHeight);
    const converter = new showdown.Converter({strikethrough: true})
    this.descriptionHtml = converter.makeHtml(mdDescription)
  },
  beforeDestroy() {
    window.removeEventListener('resize', this.updateCheatItemBoxHeight);
  },
  methods: {
    async scanCheatsFolders() {
      let resp = await window.eel.scan_all_cheats_folder()()
      if (resp.code === 0 && resp.data) {
        this.cheatsFolders = resp.data
        window.eel.get_game_data()((resp) => {
          this.gameDataInited = true
          if (resp.code === 0) {
            let nl = []
            for (let item of this.cheatsFolders) {
              item.game_name = resp.data[item.game_id]
              nl.push(item)
            }
            this.cheatsFolders = nl;
          }
        });
        this.cheatsInited = true
        return this.cheatsFolders
      }
      return []
    },
    updateCheatItemBoxHeight() {
      this.cheatItemBoxHeight = window.innerHeight - 390
    },
    concatFolderItemName(item) {
      let gameName = item.game_name ? item.game_name : this.gameDataInited ? '未知游戏' : '游戏信息加载中...'
      return `[${item.game_id}] ${gameName}`
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