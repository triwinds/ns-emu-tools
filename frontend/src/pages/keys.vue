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
          <p>请善用搜索引擎...</p>
          <div id="root">
            <div class="text-h4 text-warning">添加 keys 到模拟器</div>
            <v-divider style="margin-bottom: 15px"></v-divider>
            <p>请将上面获取到的 <b>prod.keys</b> 放进相应模拟器的文件夹
            </p>
            <p><b>注意:</b> 你使用的 keys 版本号应该大于等于当前使用的固件版本号</p>
            <v-row style="padding-top: 25px">
              <v-col>
                <v-btn color="info" variant="outlined" @click="openYuzuKeyFolder" block>打开 Yuzu keys 文件夹</v-btn>
              </v-col>
              <v-col>
                <v-btn color="info" variant="outlined" @click="openSuyuKeyFolder" block>打开 Suzu keys 文件夹</v-btn>
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

function openSuyuKeyFolder() {
  window.eel.open_suyu_keys_folder()(() => {
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
