<template>
  <SimplePage>
    <v-card>
          <v-card-title class="text-h4 primary--text">Ns Emu Tools</v-card-title>
          <v-divider></v-divider>
          <v-spacer></v-spacer>
          <v-card-text class="text-h6 text--primary">
            一个用于安装/更新 NS 模拟器的工具
          </v-card-text>
          <v-card-text class="text-h6 text--primary">
            当前版本：v{{ $store.state.currentVersion }}
            <v-btn color="primary" @click="checkUpdate(true)">
              检测新版本
            </v-btn>
            <ChangeLogDialog>
              <template v-slot:activator="{on, attrs}">
                <v-btn color="info" v-bind="attrs" v-on="on" @click="loadChangeLog"
                       outlined style="margin-left: 10px">
                  更新日志
                </v-btn>
              </template>
              <template v-slot:content>
                <div class="text--primary" v-html="changeLogHtml"></div>
              </template>
            </ChangeLogDialog>
          </v-card-text>
          <v-card-text class="text-h6 text--primary">
            <div class="info-block">
              <p class="text-h5 accent--text">项目地址</p>
              <v-icon>{{ svgPath.github }}</v-icon>
              GitHub：<a class="error--text"
                        @click="openUrlWithDefaultBrowser('https://github.com/MengNianxiaoyao/ns-emu-tools')">
              MengNianxiaoyao/ns-emu-tools</a><br>
              <span class="text--primary">如果您觉得这个软件好用, 可以在 GitHub 上点个 star</span><br>
              <span class="text--primary">这是对我最大的鼓励。</span>
            </div>
            <div class="info-block">
              <p class="text-h5 success--text">讨论组</p>
              <div class="line-group">
                <div class="line-item" style="padding-top: 7px">
                  <v-img src="@/assets/telegram.webp" max-height="20" max-width="20"
                         v-show="$vuetify.theme.dark"></v-img>
                  <v-img src="@/assets/telegram_black.webp" max-height="20" max-width="20"
                         v-show="!$vuetify.theme.dark"></v-img>
                </div>
                <div class="line-item">Telegram：
                  <a class="secondary--text"
                     @click="openUrlWithDefaultBrowser('https://t.me/+mxI34BRClLUwZDcx')">Telegram 讨论组</a></div>
              </div>
            </div>
            <div class="info-block">
              <p class="text-h5 warning--text">Credits</p>
              <div class="line-group" v-for="(item, index) in credits" :key="index">
                <div class="line-item">
                  <v-icon>{{ svgPath.github }}</v-icon>
                </div>
                <div class="line-item">
                  <a class="secondary--text"
                     @click="openUrlWithDefaultBrowser(item.link)">{{ item.name }}</a>
                   - {{item.description}}
                </div>
              </div>
            </div>

          </v-card-text>
        </v-card>
  </SimplePage>
</template>

<script>
import {mdiGithub} from "@mdi/js";
import ChangeLogDialog from "@/components/ChangeLogDialog.vue";
import * as showdown from 'showdown';
import SimplePage from "@/components/SimplePage";

export default {
  name: "AboutPage",
  components: {SimplePage, ChangeLogDialog},
  data() {
    return {
      svgPath: {
        github: mdiGithub
      },
      credits: [
        {name: 'Yuzu', link: 'https://github.com/yuzu-emu/yuzu', description: 'Yuzu 模拟器'},
        {name: 'Ryujinx', link: 'https://github.com/Ryujinx/Ryujinx', description: 'Ryujinx 模拟器'},
        {name: 'hactool', link: 'https://github.com/SciresM/hactool', description: 'NS 固件解析'},
        {name: 'aria2', link: 'https://github.com/aria2/aria2', description: 'aria2 下载器'},
      ],
      changeLogHtml: '<p>加载中...</p>',
    }
  },
  methods: {
    loadChangeLog() {
      window.eel.load_change_log()((resp) => {
        if (resp.code === 0) {
          const converter = new showdown.Converter()
          this.changeLogHtml = converter.makeHtml(resp.data)
        } else {
          this.changeLogHtml = '<p>加载失败。</p>'
        }
      })
    },
  }
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
}

.line-item {
  float: left;
  margin-right: 10px;
}
</style>