<template>
  <v-app-bar flat color="primary">
    <v-app-bar-nav-icon class="white--text" @click="triggerDrawer"></v-app-bar-nav-icon>
    <v-app-bar-title>
      NS EMU TOOLS
    </v-app-bar-title>
    <v-spacer/>
    <v-btn class="float-right" icon @click="showConsoleDialog">
      <v-icon color="white" :icon="mdiConsole"></v-icon>
    </v-btn>
    <v-btn class="float-right" icon @click="switchDarkLight">
      <v-icon color="white" :icon="mdiBrightness6"></v-icon>
    </v-btn>
  </v-app-bar>
</template>

<script lang="ts" setup>
import {useEmitter} from "@/plugins/mitt";
import {mdiBrightness6, mdiConsole} from '@mdi/js'
import {useTheme} from "vuetify";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";

const emitter = useEmitter()
const theme = useTheme()
const cds = useConsoleDialogStore()

function triggerDrawer() {
  emitter.emit('triggerDrawer')
}
function showConsoleDialog() {
  cds.showConsoleDialog()
}

function switchDarkLight() {
  theme.global.name.value = theme.global.name.value === 'dark' ? 'light' : 'dark'
  window.eel.update_dark_state(theme.global.name.value === 'dark')()
}
</script>
