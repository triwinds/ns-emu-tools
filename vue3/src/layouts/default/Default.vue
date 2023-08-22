<template>
  <v-app>
    <AppDrawer/>
    <default-bar/>

    <default-view/>
    <ConsoleDialog/>
    <NewVersionDialog/>
  </v-app>
</template>

<script lang="ts" setup>
import DefaultBar from './AppBar.vue'
import DefaultView from './View.vue'
import AppDrawer from "@/layouts/default/AppDrawer.vue";
import ConsoleDialog from "@/components/ConsoleDialog.vue";
import {useAppStore} from "@/store/app";
import NewVersionDialog from "@/components/NewVersionDialog.vue";

const appStore = useAppStore()
appStore.updateAvailableFirmwareInfos()
</script>
