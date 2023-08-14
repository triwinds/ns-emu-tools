<template>
  <div style="padding: 15px;">
    <div id="text-box" v-html="mdHtml">

    </div>
  </div>
</template>

<script setup lang="ts">
import {nextTick, onMounted, onUpdated, ref} from "vue";
import showdown from "showdown";
import {openUrlWithDefaultBrowser} from "@/utils/common";

let mdHtml = ref('')
const props = defineProps(['content'])

const converter = new showdown.Converter({strikethrough: true})
onMounted(() => {
  mdHtml.value = converter.makeHtml(props.content)
})

onUpdated(() => {
  nextTick(() => {
    let aTags = document.getElementById("text-box")!.getElementsByTagName('a')
      for (let aTag of aTags) {
        let url = aTag.href
        aTag.href = 'javascript:;'
        aTag.onclick = () => {
          openUrlWithDefaultBrowser(url)
        }
      }
      let infoTags = [] as HTMLElement[]
      infoTags.push(...document.getElementById("text-box")!.getElementsByTagName('strong'))
      infoTags.push(...document.getElementById("text-box")!.getElementsByTagName('code'))
      for (let infoTag of infoTags) {
        infoTag.classList.add("text-info")
        infoTag.style.fontFamily = "'JetBrains Mono Variable', sans-serif"
      }
  })
})
</script>

<style >
#text-box details > p {
  margin-left: 30px;
}
</style>
