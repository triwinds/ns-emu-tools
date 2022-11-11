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
          v-model="fab"
          color="primary"
          dark
          fab
        >
          <v-icon v-if="fab">
            {{ svgPath.close }}
          </v-icon>
          <v-icon v-else>
            {{ svgPath.creation }}
          </v-icon>
        </v-btn>
      </template>
      <v-btn
          @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools')"
          color="grey darken-3"
          dark
          fab
        >
          <v-icon>
            {{ svgPath.github }}
          </v-icon>
        </v-btn>
      <v-btn
        fab
        dark
        color="blue darken-1"
        @click="openUrlWithDefaultBrowser('https://t.me/+mxI34BRClLUwZDcx')"
      >
        <v-img src="@/assets/telegram.webp" max-height="24" max-width="24"></v-img>
      </v-btn>
    </v-speed-dial>
  </v-card>
</template>

<script>
import { mdiGithub, mdiClose, mdiCreation } from '@mdi/js'

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
      github: mdiGithub,
      close: mdiClose,
      creation: mdiCreation,
    }
  }),
}
</script>

<style scoped>
.v-speed-dial {
  position: fixed;
}

.v-btn--floating {
  position: relative;
}
</style>