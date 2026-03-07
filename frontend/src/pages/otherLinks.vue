<template>
  <SimplePage>
    <OtherLinkItem v-for="item in linkItems" :key="item.link" style="margin-bottom: 10px">
      <template v-slot:title>
        <v-icon style="margin-right: 5px">{{
            item.link.startsWith('https://github.com') ? mdiGithub : mdiWeb
          }}
        </v-icon>
        <span class="text-warning">{{ item.title }}</span>
      </template>
      <template v-slot:description>{{ item.description }}</template>
      <template v-slot:actions>
        <v-btn variant="text" color="info" @click="openUrlWithDefaultBrowser(item.link)">
          打开链接
        </v-btn>
        <div v-if="item.subLinks">
          <v-btn v-for="subLink in item.subLinks" variant="text" color="info"
                 :key="subLink.link" @click="openUrlWithDefaultBrowser(subLink.link)">
            {{ subLink.name }}
          </v-btn>
        </div>
      </template>
    </OtherLinkItem>
  </SimplePage>
</template>

<script setup lang="ts">

import {mdiGithub, mdiWeb} from '@mdi/js';
import {openUrlWithDefaultBrowser} from "@/utils/common";
import SimplePage from "@/components/SimplePage.vue";
import OtherLinkItem from "@/components/OtherLinkItem.vue";

let linkItems = [
  {
    link: 'https://github.com/dezem/SAK',
    title: 'Switch Army Knife (SAK)',
    description: '一个用于转换 XCI / NSP / NSZ 格式的工具',
  }, {
    link: 'https://github.com/BeyondDimension/SteamTools',
    title: 'Watt Toolkit (原 steam++)',
    description: '一个开源跨平台的多功能 Steam 工具箱, 可以本地代理 Steam 社区、 Github 、谷歌验证码等国内难以访问的网页',
    subLinks: [
      {
        name: '官网',
        link: 'https://steampp.net/'
      }
    ]
  }, {
    link: 'https://github.com/theboy181/switch-ptchtxt-mods',
    title: 'theboy181 的 MOD 合集',
    description: '由 theboy181 整理的 MOD 合集',
    subLinks: [
      {
        name: 'Discord',
        link: 'https://discord.gg/B56sXVVRmC'
      }
    ]
  }, {
    link: 'https://github.com/ChanseyIsTheBest/NX-60FPS-RES-GFX-Cheats/blob/main/GAMES.md',
    title: '图形类金手指合集',
    description: '由 ChanseyIsTheBest 整理的金手指合集',
  }, {
    link: 'https://www.cheatslips.com/',
    title: '作弊类金手指合集',
    description: '由 cheatslips 整理的金手指合集',
  }, {
    link: 'http://www.ffhome.com/works/1814.html',
    title: 'FFHOME Nintendo Switch Game Manager',
    description: '软件全称 FFHOME Nintendo Switch Game Manager，它是一款整理（处理）你拥有的NSP格式、XCI格式游戏文件的一款小工具，包括文件信息查看、批量更名和文件处理等功能。',
  },
]

</script>

<style scoped>

</style>
