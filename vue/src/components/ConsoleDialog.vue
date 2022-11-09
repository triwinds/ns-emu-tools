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

        <div style="padding-left: 10px; padding-right: 10px; padding-top: 10px;">
          <v-virtual-scroll ref="consoleBox" :items="lines" height="300" item-height="26"
                            style="background-color: #000; overflow-y: scroll">
            <template v-slot:default="{ item, index }">
              <v-list-item :key="index">
                <v-list-item-content>{{item}}</v-list-item-content>
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
        lines: [],
        count: 0
      }
    },
    created() {
      setInterval(() => {
        if (this.count > 20) {
          this.count = 0
          this.lines = []
        }
        this.lines.push("test" + this.count++)
      }, 300)
    },
    methods: {
      closeDialog() {
        this.$store.commit('SET_CONSOLE_DIALOG_FLAG', false)
      }
    },
    computed: {
      consoleText() {
        let str = ''
        this.lines.forEach((line) => {
          str += line + '\n'
        })
        return str
      }
    },
    updated() {
      this.$nextTick(() => {
        let consoleBox = this.$refs.consoleBox
        if (consoleBox) {
          // let ele = el.$el
          console.log(consoleBox.$el.scrollHeight)
          consoleBox.$el.scrollTop = consoleBox.$el.scrollHeight
        }
      })
    }
  }
</script>