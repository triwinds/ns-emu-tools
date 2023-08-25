<template>
  <SimplePage>
    <v-card>
      <v-card-title class="text-h4 text-primary">Ns Emu Tools</v-card-title>
      <v-divider></v-divider>
      <v-spacer></v-spacer>
      <v-card-text class="text-h6">
        一个用于安装/更新 NS 模拟器的工具
      </v-card-text>
      <v-card-text class="text-h6">
        当前版本：v{{ configStore.currentVersion }}
        <v-btn color="primary" variant="outlined" @click="configStore.checkUpdate(true)">
          检测新版本
        </v-btn>
        <ChangeLogDialog>
          <template v-slot:activator="{props}">
            <v-btn color="info" v-bind="props" @click="loadChangeLog"
                   variant="outlined" style="margin-left: 10px">
              更新日志
            </v-btn>
          </template>
          <template v-slot:content>
            <div v-html="changeLogHtml"></div>
          </template>
        </ChangeLogDialog>
        <v-btn color="success" variant="outlined" style="margin-left: 10px"
               @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/blob/main/LICENSE')">
          License
        </v-btn>
      </v-card-text>
      <v-card-text class="text-h6 text--primary">
        <div class="info-block">
          <p class="text-h5 text-accent" style="padding-bottom: 5px">项目地址</p>
          <div class="line-group">
          <div class="line-item-icon"><v-icon size="24">{{ mdiGithub }}</v-icon></div>
          <div class="line-item">GitHub：<a class="text-error"
                    @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools')">

          triwinds/ns-emu-tools</a></div>
          </div>
          <span>如果您觉得这个软件好用, 可以在 GitHub 上点个 star</span><br>
          <span>这是对我最大的鼓励。</span>
        </div>
        <div class="info-block">
          <p class="text-h5 text-success">讨论组</p>
          <div class="line-group">
            <div class="line-item" style="">
              <v-img src="@/assets/telegram.webp" height="20" width="20"
                     v-show="theme.global.name.value === 'dark'"></v-img>
              <v-img src="@/assets/telegram_black.webp" height="20" width="20"
                     v-show="theme.global.name.value !== 'dark'"></v-img>
            </div>
            <div class="line-item">Telegram：
              <a class="text-secondary"
                 @click="openUrlWithDefaultBrowser('https://t.me/+mxI34BRClLUwZDcx')">Telegram 讨论组</a></div>
          </div>
        </div>
        <div class="info-block">
          <p class="text-h5 text-warning">Credits</p>
          <div class="line-group" v-for="(item, index) in credits" :key="index">
            <div class="line-item-icon">
              <v-icon size="24">{{ mdiGithub }}</v-icon>
            </div>
            <div class="line-item">
              <a class="text-secondary"
                 @click="openUrlWithDefaultBrowser(item.link)">{{ item.name }}</a>
              - {{ item.description }}
            </div>
          </div>
        </div>

      </v-card-text>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import {mdiGithub} from "@mdi/js";
import SimplePage from "@/components/SimplePage.vue";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import {openUrlWithDefaultBrowser} from "@/utils/common";
import {useTheme} from "vuetify";
import {ref} from "vue";
import {CommonResponse} from "@/types";
import showdown from "showdown";
import {useConfigStore} from "@/store/ConfigStore";

const theme = useTheme()
const configStore = useConfigStore()
let credits = [
  {name: 'Yuzu', link: 'https://github.com/yuzu-emu/yuzu', description: 'Yuzu 模拟器'},
  {name: 'Ryujinx', link: 'https://github.com/Ryujinx/Ryujinx', description: 'Ryujinx 模拟器'},
  {name: 'hactool', link: 'https://github.com/SciresM/hactool', description: 'NS 固件解析'},
  {name: 'aria2', link: 'https://github.com/aria2/aria2', description: 'aria2 下载器'},
  {name: 'Github 镜像源', link: 'https://github.com/XIU2/UserScript/blob/master/GithubEnhanced-High-Speed-Download.user.js', description: '来自 X.I.U 大佬的 Github 增强脚本'},
  {name: 'pineappleEA', link: 'https://github.com/pineappleEA/pineapple-src', description: 'Yuzu EA 版本来源'},
  {name: 'darthsternie.net', link: 'https://darthsternie.net/switch-firmwares/', description: 'NS 固件来源'},
]
let changeLogHtml = ref('<p>加载中...</p>')
function loadChangeLog() {
  window.eel.load_change_log()((resp: CommonResponse) => {
    if (resp.code === 0) {
      const converter = new showdown.Converter()
      changeLogHtml.value = converter.makeHtml(resp.data)
    } else {
      changeLogHtml.value = '<p>加载失败。</p>'
    }
  })
}
</script>

<style scoped>
.info-block {
  margin-bottom: 20px;
}

.line-group {
  width: 100%;
  /*overflow-x: auto;*/
  overflow: hidden;
  margin-top: 5px;
}

.line-item {
  float: left;
  margin-right: 10px;
  margin-top: 2px;
  height: 24px;
}

.line-item-icon {
  float: left;
  margin-right: 10px;
}
span {
  line-height: 30px;
}
</style>
