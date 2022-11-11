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
          <img src="./assets/icon.webp" alt="">
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
        <v-btn class="float-right" icon @click="$vuetify.theme.dark = !$vuetify.theme.dark">
          <v-icon color="white">
            {{ svgPath.darkLightSwitch }}
          </v-icon>
        </v-btn>
      </v-container>
    </v-app-bar>

    <v-main>
      <v-container fluid style="height: 100%">
        <v-row class="child-flex">
          <v-col>
            <router-view/>
          </v-col>
        </v-row>
      </v-container>
      <SpeedDial></SpeedDial>
      <ConsoleDialog></ConsoleDialog>
      <NewVersionDialog></NewVersionDialog>
    </v-main>
  </v-app>
</template>

<script>
import router from "@/router";
import SpeedDial from "@/components/SpeedDial";
import ConsoleDialog from "@/components/ConsoleDialog";
import NewVersionDialog from "@/components/NewVersionDialog";
import '@/plugins/mixin';
import {mdiBrightness6, mdiConsole, mdiInformation, mdiKeyVariant, mdiNewBox} from '@mdi/js'

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
    }
  }),
  created() {
    this.$store.dispatch('initCurrentVersion')
    this.checkUpdate(false)
    this.initAvailableFirmwareInfos()
    this.gotoLatestOpenEmuPage()
    this.appendConsoleMessage('启动时间：' + new Date().toLocaleString())
  },
  methods: {
    openReleasePage() {
      if (this.hasNewVersion) {
        this.openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/releases');
      }
    },
    async gotoLatestOpenEmuPage() {
      let config = await this.$store.dispatch('loadConfig')
      if (router.currentRoute.path === '/'
        && router.currentRoute.path !== '/' + config.setting.lastOpenEmuPage) {
        await router.push('/' + config.setting.lastOpenEmuPage)
      }
    },
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
  width: 0 !important;
  height: 0 !important;
  background: transparent !important;
}

html ::-webkit-scrollbar-corner, html ::-webkit-scrollbar-track {
  background: transparent !important;
}

html ::-webkit-scrollbar-corner, html ::-webkit-scrollbar-track {
  background: transparent !important;
}

html ::-webkit-resizer, html ::-webkit-scrollbar-thumb {
  background: #0000;
  border-radius: 3px;
}
</style>