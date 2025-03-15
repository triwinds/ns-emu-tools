<template>
  <div id="speed-dial" class="text-center">
    <v-menu open-delay="0" close-delay="100"
            open-on-hover
            transition="slide-y-reverse-transition">
      <template v-slot:activator="{ props }">
        <v-btn
            color="primary"
            v-bind="props"
            :icon="mdiCreation"
        />
      </template>
      <div class="btn-item">
        <v-btn
            color="grey-darken-3"
            :icon="mdiGithub"
            @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools')"
        />
      </div>
      <div class="btn-item">
        <v-btn
            color="blue-darken-1"
            :icon="mdiGithub"
            @click="openUrlWithDefaultBrowser('https://t.me/+mxI34BRClLUwZDcx')"
        >
          <v-img src="@/assets/telegram.webp" height="22" width="22"></v-img>
        </v-btn>
      </div>

    </v-menu>
  </div>
</template>

<script setup lang="ts">
import {mdiCreation, mdiGithub} from '@mdi/js'
import {openUrlWithDefaultBrowser} from "@/utils/common";
</script>

<style scoped>
#speed-dial {
  position: absolute;
  right: 24px;
  bottom: 24px;
}

.btn-item {
  padding-bottom: 12px;
}
</style>
