<template>
  <v-app id="inspire" :style="{background: $vuetify.theme.themes[theme].background}">
    <v-navigation-drawer
        v-model="drawer"
        app
    >
      <v-sheet
          color="secondary"
          class="pa-4"
      >
        <v-avatar
            class="mb-4"
            color="#00000000"
            size="100"
            rounded
        >
          <img src="./assets/icon.png" alt="">
        </v-avatar>

        <div>版本：v{{ currentVersion }}</div>
      </v-sheet>

      <v-divider></v-divider>
      <!--  -->
      <v-list>
        <v-list-item link to="/yuzu">
          <v-list-item-icon>
            <v-icon>mdi-alert-octagon</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Yuzu</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/ryujinx">
          <v-list-item-icon>
            <v-icon>mdi-alert-octagon</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>Ryujinx</v-list-item-title>
          </v-list-item-content>
        </v-list-item>

        <v-list-item link to="/about">
          <v-list-item-icon>
            <v-icon>mdi-alert-octagon</v-icon>
          </v-list-item-icon>
          <v-list-item-content>
            <v-list-item-title>About</v-list-item-title>
          </v-list-item-content>
        </v-list-item>
      </v-list>
    </v-navigation-drawer>

    <v-app-bar color="primary" app>
      <v-app-bar-nav-icon @click="drawer = !drawer"></v-app-bar-nav-icon>

      <v-toolbar-title>NS EMU TOOLS</v-toolbar-title>
    </v-app-bar>

    <v-main>
      <router-view/>
    </v-main>
  </v-app>
</template>

<script>
export default {
  data: () => ({
    drawer: null,
    currentVersion: '0.0.1',
  }),
  created() {
    this.initCurrentVersion()
  },
  methods: {
    initCurrentVersion() {
      window.eel.get_current_version()((data) => {
        console.log(data)
        if (data['code'] === 0) {
          this.currentVersion = data['data']
        } else {
          this.currentVersion = '未知'
        }
      })
    },
  },
  computed: {
    theme() {
      return (this.$vuetify.theme.dark) ? 'dark' : 'light'
    }
  }
}
</script>