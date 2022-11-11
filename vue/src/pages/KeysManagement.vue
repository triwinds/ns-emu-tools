<template>
  <v-container>
    <v-row>
      <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
      <v-col md="10" lg="8">
        <v-card>
          <v-container id="main-container" style="margin-top: 15px;">
            <v-row>
              <v-col>
                <p class="text-h3 primary--text">
                  密钥管理
                </p>
              </v-col>
            </v-row>
            <v-divider style="margin-bottom: 15px"></v-divider>

            <div class="text-h6">
              <p class="text--primary">由于版权原因, 本项目不再提供 keys 自动安装功能。</p>
              <p class="text--primary">请用户自行放置 keys 文件至相应文件夹。</p>
              <div class="text-h4 accent--text">获取密钥</div>
              <v-divider style="margin-bottom: 15px"></v-divider>
              <p class="text--primary">你可以跟随
                <a
                  @click="openUrlWithDefaultBrowser('https://yuzu-emu.org/help/quickstart/#dumping-prodkeys-and-titlekeys')"
                  class="info--text">yuzu 的官方教程</a>
                从 switch 中提取 keys 文件.
              </p>
              <p class="text--primary">也可以在贴吧或者 qq 群等地方找到其他大佬分享的 keys 文件.</p>
              <p class="text--primary">下面是两位大佬分享的 keys:</p>
              <p class="text--primary">1. 由 文无 大佬提供的
                <a @click="openUrlWithDefaultBrowser('https://url11.ctfile.com/d/18719411-48103523-6faacf?49760005')"
                   class="info--text">城通网盘</a>(访问密码：7257),
                原帖地址: <a @click="openUrlWithDefaultBrowser('https://tieba.baidu.com/p/7799545671')"
                             class="info--text">百度贴吧</a>
              </p>
              <p class="text--primary">2. 由 PrincessAkira 大佬提供的
                <a
                  @click="openUrlWithDefaultBrowser('https://drive.google.com/drive/folders/1KAym-RpGIDuJiSmMLmpCtGVbhLm4VjTZ')"
                  class="info--text">GDrive(需代理)</a>
                原地址: <a
                  @click="openUrlWithDefaultBrowser('https://github.com/PrincessAkira/road-to-yuzu-without-switch')"
                  class="info--text">GitHub</a>
              </p>
              <div id="root">
                <div class="text-h4 warning--text">添加 keys 到模拟器</div>
                <v-divider style="margin-bottom: 15px"></v-divider>
                <p class="text--primary">请将上面获取到的 <b>prod.keys</b> 和 <b>title.keys</b> 放进相应模拟器的文件夹
                </p>
                <p class="text--primary"><b>注意:</b> 你使用的 keys 版本号应该大于等于当前使用的固件版本号</p>
                <v-row>
                  <v-col>
                    <v-btn color="info" outlined @click="openYuzuKeyFolder" block>打开 Yuzu keys 文件夹</v-btn>
                  </v-col>
                  <v-col>
                    <v-btn color="info" outlined @click="openRyujinxKeyFolder" block>打开 Ryujinx keys 文件夹</v-btn>
                  </v-col>
                </v-row>
              </div>
            </div>
          </v-container>
        </v-card>
      </v-col>
      <v-col v-show="$vuetify.breakpoint.mdAndUp"></v-col>
    </v-row>
  </v-container>
</template>

<script>
export default {
  name: "KeysManagement",
  methods: {
    openYuzuKeyFolder() {
      window.eel.open_yuzu_keys_folder()(() => {
      })
    },
    openRyujinxKeyFolder() {
      window.eel.open_ryujinx_keys_folder()(() => {
      })
    }
  }
}
</script>

<style scoped>

</style>