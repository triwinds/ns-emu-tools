<template>
  <v-app>
    <AppDrawer/>
    <default-bar/>

    <default-view/>
  </v-app>
</template>

<script lang="ts" setup>
import DefaultBar from './AppBar.vue'
import DefaultView from './View.vue'
import AppDrawer from "@/layouts/default/AppDrawer.vue";


</script>
