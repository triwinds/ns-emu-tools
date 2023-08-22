<template>
  <v-app>
    <AppDrawer/>
    <default-bar/>

    <default-view/>
    <ConsoleDialog/>
    <NewVersionDialog/>
    <SpeedDial v-if="display.mdAndUp.value"/>
  </v-app>
</template>

<script lang="ts" setup>
import DefaultBar from './AppBar.vue'
import DefaultView from './View.vue'
import AppDrawer from "@/layouts/default/AppDrawer.vue";
import ConsoleDialog from "@/components/ConsoleDialog.vue";
import {useAppStore} from "@/store/app";
import NewVersionDialog from "@/components/NewVersionDialog.vue";
import SpeedDial from "@/components/SpeedDial.vue";
import {useDisplay} from "vuetify";

const appStore = useAppStore()
appStore.updateAvailableFirmwareInfos()
const display = useDisplay()

</script>
