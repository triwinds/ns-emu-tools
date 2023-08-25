<template>
<SimplePage>
    <v-card style="height: 100%">
      <v-card-title class="text-h4 text-primary">
        Yuzu 金手指管理
      </v-card-title>
      <v-divider></v-divider>
      <v-container>
        <v-row>
          <v-col>
            <v-select
                :items="cheatsFolders"
                v-model="selectedFolder"
                :item-title="concatFolderItemName"
                item-value="cheats_path"
                label="选择游戏 mod 目录"
                hide-details
                variant="underlined"
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
                item-title="name"
                item-value="path"
                variant="underlined"
                hide-details
            ></v-select>
          </v-col>
        </v-row>
        <v-row v-show="selectedCheatFile !== ''">
          <v-col>
            <v-btn block variant="outlined" color="success" @click="saveSelectedCheats">保存设定</v-btn>
          </v-col>
          <v-col>
            <v-btn block variant="outlined" color="info" @click="openCheatModFolder">打开 Mod 文件夹</v-btn>
          </v-col>
        </v-row>
        <v-row v-show="selectedCheatFile !== ''">
          <v-col>
            <v-virtual-scroll
                :items="cheatItems"
                :height="cheatItemBoxHeight"
            >
              <template v-slot:default="{ item }">
                <v-list-item>
                  <v-list-item-action>
                    <div style="height: 30px">
                      <v-checkbox
                        v-model="item.enable"
                        :label="item.title"
                        hide-details
                        color="primary"
                        density="compact"/>
                    </div>
                  </v-list-item-action>
                </v-list-item>
              </template>
            </v-virtual-scroll>

          </v-col>
        </v-row>
        <v-row v-if="cheatItems && cheatItems.length > 0">
          <v-col>
            <v-btn block variant="outlined" color="info" @click="updateAllItemState(true)">选择全部</v-btn>
          </v-col>
          <v-col>
            <v-btn block variant="outlined" color="error" @click="updateAllItemState(false)">反选全部</v-btn>
          </v-col>
        </v-row>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import {onMounted, onUnmounted, ref, watch} from "vue";
import {loadGameData} from "@/utils/common";
import {CheatFileInfo, CheatGameInfo, CheatItem, CommonResponse} from "@/types";
import {useAppStore} from "@/store/app";
import showdown from "showdown";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";

let cheatsInited = ref(false)
let cheatsFolders = ref<CheatGameInfo[]>([])
let selectedFolder = ref('')
let cheatFiles = ref<CheatFileInfo[]>([])
let selectedCheatFile = ref('')
let cheatItems = ref<CheatItem[]>([])
let cheatItemBoxHeight = ref(window.innerHeight - 450)
let descriptionHtml = ref('')
const appStore = useAppStore()
const cds = useConsoleDialogStore()

let mdDescription = `
这个模块是对 Yuzu 金手指功能的一个补充，目标是实现类似 Ryujinx 对金手指中单个作弊项进行开关的功能.

在使用前请先阅读以下说明：

1. 请先确认金手指文件已经可以在 Yuzu 中被识别，如果 Yuzu 不能识别你的金手指，那么这里也不能
2. 如果不清楚如何在 Yuzu 中添加金手指，可以在 B 站上搜索相关教程
3. 某些金手指中的前面的一些公共的作弊项是必须启用的，请不要关闭这些作弊项（这些项目一般都在文件的最上面
4. 修改后需要重启游戏才会生效
5. 点击保存时会自动备份原来的金手指文件，如果出现问题，可以自行用这些备份文件来还原
`

onMounted(async () => {
  cds.cleanMessages()
  await scanCheatsFolders()
  updateCheatItemBoxHeight()
  window.addEventListener('resize', updateCheatItemBoxHeight);
  const converter = new showdown.Converter({strikethrough: true})
  descriptionHtml.value = converter.makeHtml(mdDescription)
})

onUnmounted(() => {
  window.removeEventListener('resize', updateCheatItemBoxHeight)
})

async function scanCheatsFolders() {
  let resp = await window.eel.scan_all_cheats_folder()()
  if (resp.code === 0 && resp.data) {
    cheatsFolders.value = resp.data
    loadGameData().then(gameData => {
      let nl = []
      for (let item of cheatsFolders.value) {
        item.game_name = gameData[item.game_id]
        nl.push(item)
      }
      cheatsFolders.value = nl;
    })
    cheatsInited.value = true
    return cheatsFolders.value
  }
  return []
}

function updateCheatItemBoxHeight() {
  cheatItemBoxHeight.value = window.innerHeight - 460
}

function concatFolderItemName(item: CheatGameInfo) {
  if (!item) {
    return ''
  }
  let gameName = item.game_name ? item.game_name : appStore.gameDataInited ? '未知游戏' : '游戏信息加载中...'
  return `[${item.game_id}] ${gameName}`
}

function listAllCheatFilesFromFolder(selectedFolder: string) {
  window.eel.list_all_cheat_files_from_folder(selectedFolder)((resp: CommonResponse) => {
    if (resp.code === 0 && resp.data) {
      cheatFiles.value = resp.data
      selectedCheatFile.value = resp.data[0].path
    } else {
      cheatsFolders.value = []
      selectedCheatFile.value = ''
    }
  })
}

function loadCheatChunkInfo(selectedCheatFile: string) {
  cheatItems.value = []
  window.eel.load_cheat_chunk_info(selectedCheatFile)((resp: CommonResponse) => {
    if (resp.code === 0 && resp.data) {
      cheatItems.value = resp.data
    }
  })
  // console.log(selectedCheatFile)
  // let test = []
  // for (let i = 0; i < 100; i++) {
  //   test.push({title: "title " + i, enable: true})
  // }
  // cheatItems = test
}

function saveSelectedCheats() {
  if (!cheatItems.value) {
    return
  }
  let enabledTitles = cheatItems.value.filter(d => d.enable).map(d => d.title)
  window.eel.update_current_cheats(enabledTitles, selectedCheatFile.value)((resp: CommonResponse) => {
    if (resp.code === 0) {
      cds.appendConsoleMessage('保存成功')
      cds.showConsoleDialog()
    }
  })
}

function openCheatModFolder() {
  window.eel.open_cheat_mod_folder(selectedFolder.value)((resp: CommonResponse) => {
    if (resp.code === 0) {
      cds.appendConsoleMessage("打开文件夹成功")
    }
  })
}

function updateAllItemState(state: boolean) {
  for (let item of cheatItems.value) {
    item.enable = state;
  }
}

watch(selectedFolder, (newValue) => {
  if (newValue && newValue.length > 0) {
    listAllCheatFilesFromFolder(newValue)
  }
}, {immediate: false})
watch(selectedCheatFile, (newValue) => {
  if (newValue && newValue.length > 0) {
    loadCheatChunkInfo(newValue)
  }
}, {immediate: false})

</script>

<style scoped>
div.v-selection-control {
  height: 30px !important;
}

</style>
