<template>
  <v-dialog
      v-model="dialog"
      max-width="900"
  >
    <template v-slot:activator="{ on, attrs }">
      <slot name="activator" v-bind:on="on" v-bind:attrs="attrs">

      </slot>
    </template>

    <v-card>
      <v-card-title class="text-h5 primary white--text">
        {{ title }}
      </v-card-title>

      <div style=" padding: 15px; overflow-y: auto; max-height: 50vh">
        <div ref="text-box" class="text--primary" v-html="mdHtml">

        </div>
      </div>

      <v-divider></v-divider>

      <v-card-actions>
        <v-spacer></v-spacer>
        <v-btn
            color="primary"
            text
            @click="dialog = false"
        >
          关闭
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script>
import * as showdown from 'showdown';

export default {
  name: "MarkdownDialog",
  data() {
    return {
      dialog: false,
      mdHtml: '',
    }
  },
  props: ['title', 'content'],
  mounted() {
    const converter = new showdown.Converter({strikethrough: true})
    this.mdHtml = converter.makeHtml(this.content)
  },
  updated() {
    this.$nextTick(() => {
      let aTags = this.$refs["text-box"].getElementsByTagName('a')
      for (let aTag of aTags) {
        let url = aTag.href
        aTag.href = 'javascript:;'
        aTag.onclick = () => {
          this.openUrlWithDefaultBrowser(url)
        }
      }
      let infoTags = []
      infoTags.push(...this.$refs["text-box"].getElementsByTagName('strong'))
      infoTags.push(...this.$refs["text-box"].getElementsByTagName('code'))
      for (let infoTag of infoTags) {
        infoTag.classList.add("info--text")
      }
    })
  },
}
</script>

<style scoped>

</style>