<template>
  <v-app id="inspire" :style="{background: $vuetify.theme.themes[theme].background}">
    <v-navigation-drawer
      v-model="drawer"
      app
    >
      <v-sheet
        class="pa-4"
        @click="openReleasePage" :style="{cursor: hasNewVersion ? 'pointer' : 'default' }"
      >
        <v-avatar
          class="mb-4"
          color="#00000000"
          size="100"
          rounded
        >
          <img src="./assets/icon.png" alt="">
        </v-avatar>

        <div>版本：v{{ $store.state.currentVersion }}
          <v-icon color="info" v-show="hasNewVersion">{{ svgPath.newBox }}</v-icon>
        </div>
        <div v-show="hasNewVersion" class="info--text">
          点击查看新版本
        </div>
      </v-sheet>

      <v-divider></v-divider>
      <!--  -->
      <v-list>
        <v-list-item link to="/yuzu">
          <v-list-item-icon>
            <v-img src="./assets/yuzu.webp" max-height="24" max-width="24"></v-img>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Yuzu 模拟器</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/ryujinx">
          <v-list-item-icon>
            <v-img src="./assets/ryujinx.webp" max-height="24" max-width="24"></v-img>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Ryujinx 模拟器</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/keys">
          <v-list-item-icon>
            <v-icon color="amber darken-2">{{ svgPath.key }}</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>密钥管理</v-list-item-title>
          </v-list-item-content>
        </v-list-item>


        <v-list-group
          :value="true"
        >
          <template v-slot:activator>
            <v-list-item-icon>
              <v-icon color="blue lighten-2">{{ svgPath.testTube }}</v-icon>
            </v-list-item-icon>
            <v-list-item-title class="text--primary">实验性功能</v-list-item-title>
          </template>
          <v-list-item link to="/yuzuCheatsManagement" style="padding-left: 30px">
            <v-list-item-icon>
              <v-icon color="indigo accent-2">{{ svgPath.memory }}</v-icon>
            </v-list-item-icon>
            <v-list-item-content>
              <v-list-item-title class="text--primary">Yuzu 金手指管理</v-list-item-title>
            </v-list-item-content>
          </v-list-item>
          <v-list-item link to="/yuzuSaveManagement" style="padding-left: 30px">
            <v-list-item-icon>
              <v-icon color="blue darken-1">{{ svgPath.mdiContentSaveMoveOutline }}</v-icon>
            </v-list-item-icon>
            <v-list-item-content>
              <v-list-item-title class="text--primary">Yuzu 存档备份</v-list-item-title>
            </v-list-item-content>
          </v-list-item>
          <v-list-item link to="/cloudflareST" style="padding-left: 30px">
            <v-list-item-icon>
              <v-icon color="error">{{ svgPath.speedmeter }}</v-icon>
            </v-list-item-icon>
            <v-list-item-content>
              <v-list-item-title class="text--primary">Cloudflare 节点选优</v-list-item-title>
            </v-list-item-content>
          </v-list-item>
        </v-list-group>

        <v-list-item link to="/settings">
          <v-list-item-icon>
            <v-icon color="blue-grey lighten-3">{{ svgPath.cog }}</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>设置</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/otherLinks">
          <v-list-item-icon>
            <v-icon color="warning">{{ svgPath.link }}</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>其他资源</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/faq">
          <v-list-item-icon>
            <v-icon color="light-green darken-2">{{ svgPath.help }}</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>常见问题</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/about">
          <v-list-item-icon>
            <v-icon color="info">{{ svgPath.info }}</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>About</v-list-item-title>
          </v-list-item-content>
        </v-list-item>
      </v-list>
    </v-navigation-drawer>

    <v-app-bar color="primary" app>
      <v-app-bar-nav-icon class="white--text" @click="drawer = !drawer"></v-app-bar-nav-icon>
      <v-toolbar-title class="white--text" style="min-width: 200px">NS EMU TOOLS</v-toolbar-title>
      <v-container>
        <v-btn class="float-right" icon @click="showConsoleDialog">
          <v-icon color="white">
            {{ svgPath.console }}
          </v-icon>
        </v-btn>
        <v-btn class="float-right" icon @click="switchDarkLight">
          <v-icon color="white">
            {{ svgPath.darkLightSwitch }}
          </v-icon>
        </v-btn>
      </v-container>
    </v-app-bar>

    <v-main style="overflow-y: hidden">
      <v-container fluid style="height: 100%; padding: 0; margin-top: 5px">
        <v-row class="child-flex" style="height: 100%; margin-bottom: 0">
          <v-col style="height: 100%; padding-bottom: 0">
            <router-view style="height: 100%"/>
          </v-col>
        </v-row>
      </v-container>
      <SpeedDial v-show="$vuetify.breakpoint.mdAndUp"></SpeedDial>
      <ConsoleDialog></ConsoleDialog>
      <NewVersionDialog></NewVersionDialog>
    </v-main>
  </v-app>
</template>

<script>
import 'core-js/stable';
import 'regenerator-runtime/runtime';
import router from "@/router";
import SpeedDial from "@/components/SpeedDial";
import ConsoleDialog from "@/components/ConsoleDialog";
import NewVersionDialog from "@/components/NewVersionDialog";
import '@/plugins/mixin';
import {
  mdiBrightness6, mdiConsole, mdiInformation, mdiKeyVariant, mdiNewBox, mdiCog, mdiTestTube,
  mdiMemory, mdiCommentQuestionOutline, mdiLinkVariant, mdiSpeedometer, mdiContentSaveMoveOutline,
} from '@mdi/js'
import Vue from "vue";

let pendingWriteSize = false

export default {
  components: {NewVersionDialog, SpeedDial, ConsoleDialog},
  data: () => ({
    drawer: null,
    svgPath: {
      darkLightSwitch: mdiBrightness6,
      console: mdiConsole,
      info: mdiInformation,
      key: mdiKeyVariant,
      newBox: mdiNewBox,
      cog: mdiCog,
      testTube: mdiTestTube,
      memory: mdiMemory,
      help: mdiCommentQuestionOutline,
      link: mdiLinkVariant,
      speedmeter: mdiSpeedometer,
      mdiContentSaveMoveOutline,
    }
  }),
  created() {
    if (!Vue.config.devtools) this.setupWebsocketConnectivityCheck()
    this.$store.dispatch('initCurrentVersion')
    this.checkUpdate(false)
    this.initAvailableFirmwareInfos()
    this.applyUiConfig()
    this.appendConsoleMessage('启动时间：' + new Date().toLocaleString())
    window.addEventListener('resize', this.rememberWindowSize);
  },
  methods: {
    openReleasePage() {
      if (this.hasNewVersion) {
        this.openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
      }
    },
    rememberWindowSize() {
      if (!pendingWriteSize) {
        pendingWriteSize = true
        setTimeout(() => {
          pendingWriteSize = false
          window.eel.update_window_size(window.outerWidth, window.outerHeight)()
        }, 1000)
      }
    },
    async applyUiConfig() {
      let config = await this.$store.dispatch('loadConfig')
      if (router.currentRoute.path === '/'
        && router.currentRoute.path !== '/' + config.setting.ui.lastOpenEmuPage) {
        await router.push('/' + config.setting.ui.lastOpenEmuPage)
      }
      this.$vuetify.theme.dark = config.setting.ui.dark
    },
    switchDarkLight() {
      this.$vuetify.theme.dark = !this.$vuetify.theme.dark
      window.eel.update_dark_state(this.$vuetify.theme.dark)()
    },
    setupWebsocketConnectivityCheck() {
      setInterval(() => {
        try {
          let ws = window.eel._websocket
          if (ws.readyState === ws.CLOSED || ws.readyState === ws.CLOSING) {
            this.cleanAndShowConsoleDialog()
            this.appendConsoleMessage('程序后端连接出错, 请关闭当前页面并重启程序以解决这个问题。')
          }
        } catch (e) {
          console.log(e)
          this.cleanAndShowConsoleDialog()
          this.appendConsoleMessage('程序后端连接出错, 请关闭当前页面并重启程序以解决这个问题。')
        }
      }, 5000)
    }
  },
  computed: {
    theme() {
      return (this.$vuetify.theme.dark) ? 'dark' : 'light'
    },
    hasNewVersion() {
      return this.$store.state.hasNewVersion
    }
  }
}
</script>

<style>
html ::-webkit-scrollbar {
  width: 0 ;
  height: 0 ;
}
div::-webkit-resizer, div::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar {
  width: 5px !important;
  height: 5px !important;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}

div::-webkit-resizer, div ::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}
</style>