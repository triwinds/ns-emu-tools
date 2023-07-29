<template>
  <v-container>
    <v-row>
      <v-spacer v-show="display.mdAndUp.value"></v-spacer>
      <v-col md="11" lg="9" xl="8" style="padding-bottom: 0">
        <slot></slot>
      </v-col>
      <v-spacer v-show="display.mdAndUp.value"></v-spacer>
    </v-row>
  </v-container>
</template>

<script lang="ts" setup>
import {useDisplay} from "vuetify";

const display = useDisplay()
</script>

<style scoped>

</style>
