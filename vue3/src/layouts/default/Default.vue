<template>
  <v-app>
    <AppDrawer/>
    <default-bar/>

    <default-view/>
    <ConsoleDialog/>
  </v-app>
</template>

<script lang="ts" setup>
import DefaultBar from './AppBar.vue'
import DefaultView from './View.vue'
import AppDrawer from "@/layouts/default/AppDrawer.vue";
import ConsoleDialog from "@/components/ConsoleDialog.vue";
import {useAppStore} from "@/store/app";

const appStore = useAppStore()
appStore.updateAvailableFirmwareInfos()
</script>
