<template>
  <div class="text-center">
    <v-dialog
      v-model="$store.state.consoleDialogFlag"
      max-width="900"
    >
<!--      <template v-slot:activator="{ on, attrs }">-->
<!--        <v-btn-->
<!--          color="red lighten-2"-->
<!--          dark-->
<!--          v-bind="attrs"-->
<!--          v-on="on"-->
<!--        >-->
<!--          Click Me-->
<!--        </v-btn>-->
<!--      </template>-->

      <v-card>
        <v-card-title class="text-h5 primary">
          控制台日志
        </v-card-title>

        <div style="padding-left: 10px; padding-right: 10px; padding-top: 10px;" class="flex-grow-0">
          <v-virtual-scroll ref="consoleBox" :items="$store.state.consoleMessages" height="300" item-height="26"
                            style="background-color: #000; overflow-y: scroll; overflow-x: scroll;">
            <template v-slot:default="{ item, index }">
              <v-list-item :key="index">
                <v-list-item-content class="white--text" style="white-space: nowrap; display: inline-block;">{{item}}</v-list-item-content>
              </v-list-item>
            </template>
          </v-virtual-scroll>
        </div>

        <v-divider></v-divider>

        <v-card-actions>
          <v-spacer></v-spacer>
          <v-btn
            color="primary"
            text
            @click="closeDialog"
          >
            关闭
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </div>
</template>

<script>
  export default {
    name: 'ConsoleDialog',
    data () {
      return {

      }
    },
    created() {
      // this.showConsoleDialog()
      // setInterval(() => {
      //   this.appendConsoleMessage("test" + new Date().getTime())
      // }, 300)
    },
    methods: {
      closeDialog() {
        this.$store.commit('SET_CONSOLE_DIALOG_FLAG', false)
      }
    },
    computed: {

    },
    updated() {
      this.$nextTick(() => {
        let consoleBox = this.$refs.consoleBox
        if (consoleBox) {
          if (consoleBox.$el.scrollHeight > consoleBox.$el.offsetHeight) {
            consoleBox.$el.scrollTop = consoleBox.$el.scrollHeight
          }
        }
      })
    }
  }
</script>