<template>
  <div style=" padding: 15px; overflow-y: auto; max-height: 50vh">
    <div ref="text-box" class="text--primary" v-html="mdHtml">

    </div>
  </div>
</template>

<script>
import * as showdown from 'showdown';

export default {
  name: "MarkdownContentBox",
  data() {
    return {
      dialog: false,
      mdHtml: '',
    }
  },
  props: ['content'],
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