<template>
  <v-app id="inspire" :style="{background: $vuetify.theme.themes[theme].background}">
    <v-navigation-drawer
      v-model="drawer"
      app
    >
      <v-sheet
        class="pa-4"
        @click="clickTitle" :style="{cursor: hasNewVersion ? 'pointer' : 'default' }"
      >
        <v-avatar
          class="mb-4"
          color="#00000000"
          size="100"
          rounded
        >
          <img src="./assets/icon.png" alt="">
        </v-avatar>

        <div>版本：v{{ currentVersion }}
          <v-icon color="info" v-show="hasNewVersion">mdi-new-box</v-icon>
        </div>
      </v-sheet>

      <v-divider></v-divider>
      <!--  -->
      <v-list>
        <v-list-item link to="/yuzu">
          <v-list-item-icon>
            <v-img src="./assets/yuzu.png" max-height="24" max-width="24"></v-img>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Yuzu 模拟器</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/ryujinx">
          <v-list-item-icon>
            <v-img src="./assets/ryujinx.png" max-height="24" max-width="24"></v-img>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Ryujinx 模拟器</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/keys">
          <v-list-item-icon>
            <v-icon color="amber darken-2">mdi-key-variant</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>密钥管理</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/about">
          <v-list-item-icon>
            <v-icon color="info">mdi-information</v-icon>
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
            mdi-console
          </v-icon>
        </v-btn>
        <v-btn class="float-right" icon @click="$vuetify.theme.dark = !$vuetify.theme.dark">
          <v-icon color="white">
            mdi-brightness-6
          </v-icon>
        </v-btn>
      </v-container>
    </v-app-bar>

    <v-main>
      <router-view/>
      <ConsoleDialog></ConsoleDialog>
    </v-main>
  </v-app>
</template>

<script>
// import router from "@/router";
import ConsoleDialog from "@/components/ConsoleDialog";
import '@/plugins/mixin';

export default {
  components: {ConsoleDialog},
  data: () => ({
    drawer: null,
    currentVersion: '未知',
    hasNewVersion: false,
  }),
  created() {
    this.initCurrentVersion()
    this.checkUpdate()
    // router.push('/yuzu')
  },
  methods: {
    initCurrentVersion() {
      window.eel.get_current_version()((data) => {
        if (data['code'] === 0) {
          this.currentVersion = data['data']
        } else {
          this.currentVersion = '未知'
        }
      })
    },
    checkUpdate() {
      window.eel.check_update()((data) => {
        if (data['code'] === 0 && data['data']) {
          this.hasNewVersion = true
        }
      })
    },
    clickTitle() {
      if (this.hasNewVersion) {
        window.open('https://github.com/triwinds/ns-emu-tools/releases', '_blank');
      }
    },
  },
  computed: {
    theme() {
      return (this.$vuetify.theme.dark) ? 'dark' : 'light'
    }
  }
}
</script>