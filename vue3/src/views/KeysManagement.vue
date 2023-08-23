<template>
  <SimplePage>
    <v-card>
      <v-container id="main-container">
        <v-row>
          <v-col>
            <p class="text-h3 text-primary">
              密钥管理
            </p>
          </v-col>
        </v-row>
        <v-divider style="margin-bottom: 15px"></v-divider>

        <div class="text-h6">
          <p>由于版权原因, 本项目不再提供 keys 自动安装功能。</p>
          <p>请用户自行放置 keys 文件至相应文件夹。</p>
          <div class="text-h4 text-accent">获取密钥</div>
          <v-divider style="margin-bottom: 15px"></v-divider>
          <p>你可以跟随
            <a
                @click="openUrlWithDefaultBrowser('https://yuzu-emu.org/help/quickstart/#dumping-prodkeys-and-titlekeys')"
                class="text-info">yuzu 的官方教程</a>
            从 switch 中提取 keys 文件.
          </p>
          <p>也可以在贴吧或者 qq 群等地方找到其他大佬分享的 keys 文件.</p>
          <p>下面是两位大佬分享的 keys:</p>
          <p>1. 由 文无 大佬提供的
            <a @click="openUrlWithDefaultBrowser('https://url11.ctfile.com/d/18719411-48103523-6faacf?49760005')"
               class="text-info">城通网盘</a>(访问密码：7257),
            原帖地址: <a @click="openUrlWithDefaultBrowser('https://tieba.baidu.com/p/7799545671')"
                         class="text-info">百度贴吧</a>
          </p>
          <p>2. 由 PrincessAkira 大佬提供的
            <a
                @click="openUrlWithDefaultBrowser('https://drive.google.com/drive/folders/1KAym-RpGIDuJiSmMLmpCtGVbhLm4VjTZ')"
                class="text-info">GDrive(需代理)</a>
            原地址: <a
                @click="openUrlWithDefaultBrowser('https://github.com/PrincessAkira/road-to-yuzu-without-switch')"
                class="text-info">GitHub</a>
          </p>
          <div id="root">
            <div class="text-h4 text-warning">添加 keys 到模拟器</div>
            <v-divider style="margin-bottom: 15px"></v-divider>
            <p>请将上面获取到的 <b>prod.keys</b> 和 <b>title.keys</b> 放进相应模拟器的文件夹
            </p>
            <p><b>注意:</b> 你使用的 keys 版本号应该大于等于当前使用的固件版本号</p>
            <v-row style="padding-top: 25px">
              <v-col>
                <v-btn color="info" variant="outlined" @click="openYuzuKeyFolder" block>打开 Yuzu keys 文件夹</v-btn>
              </v-col>
              <v-col>
                <v-btn color="info" variant="outlined" @click="openRyujinxKeyFolder" block>打开 Ryujinx keys 文件夹
                </v-btn>
              </v-col>
            </v-row>
          </div>
        </div>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import {openUrlWithDefaultBrowser} from "@/utils/common";

function openYuzuKeyFolder() {
  window.eel.open_yuzu_keys_folder()(() => {
  })
}

function openRyujinxKeyFolder() {
  window.eel.open_ryujinx_keys_folder()(() => {
  })
}
</script>

<style scoped>
.text-h3 {
  padding-bottom: 10px;
}

.text-h4 {
  padding-top: 25px;
  padding-bottom: 10px;
}
</style>
