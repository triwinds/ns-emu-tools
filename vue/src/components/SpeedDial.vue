<template>
  <v-card id="create">
    <v-speed-dial
      v-model="fab"
      :top="top"
      :bottom="bottom"
      :right="right"
      :left="left"
      :direction="direction"
      :open-on-hover="hover"
      :transition="transition"
    >
      <template v-slot:activator>
        <v-btn
          @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools')"
          color="primary"
          dark
          fab
        >
          <v-icon>
            {{ svgPath.github }}
          </v-icon>
        </v-btn>
      </template>
      <v-btn
        fab
        dark
        small
        color="blue darken-1"
        @click="openUrlWithDefaultBrowser('https://t.me/+mxI34BRClLUwZDcx')"
      >
        <v-img src="@/assets/telegram.webp" max-height="24" max-width="24"></v-img>
      </v-btn>
    </v-speed-dial>
  </v-card>
</template>

<script>
import { mdiGithub } from '@mdi/js'

export default {
  name: "SpeedDial",
  data: () => ({
    direction: 'top',
    fab: false,
    fling: false,
    hover: true,
    tabs: null,
    top: false,
    right: true,
    bottom: true,
    left: false,
    transition: 'slide-y-reverse-transition',
    svgPath: {
      github: mdiGithub
    }
  }),
}
</script>

<style scoped>
.v-speed-dial {
  position: absolute;
}

.v-btn--floating {
  position: relative;
}
</style>