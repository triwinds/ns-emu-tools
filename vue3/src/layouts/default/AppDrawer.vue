<script setup lang="ts">
import {ref} from "vue";
import {
  mdiInformation, mdiKeyVariant, mdiNewBox, mdiCog, mdiTestTube,
  mdiMemory, mdiCommentQuestionOutline, mdiLinkVariant, mdiSpeedometer, mdiContentSaveMoveOutline,
} from '@mdi/js'
import {useEmitter} from "@/plugins/mitt";

const emitter = useEmitter()
let drawer = ref(true)
let open = ref(['experiment'])

emitter.on('triggerDrawer', () => {
  drawer.value = !drawer.value
})
</script>

<template>
  <v-navigation-drawer
    v-model="drawer"
    app
  >
    <!--      <v-sheet-->
    <!--        class="pa-4"-->
    <!--        @click="openReleasePage" :style="{cursor: hasNewVersion ? 'pointer' : 'default' }"-->
    <!--      >-->
    <v-sheet
      class="pa-4"
    >
      <v-avatar
        class="mb-4"
        color="#00000000"
        size="100"
        rounded
      >
        <img src="@/assets/icon.png" alt="">
      </v-avatar>

      <div>版本：v111
        <v-icon color="info" v-show="true">{{ mdiNewBox }}</v-icon>
      </div>
      <!--        <div v-show="hasNewVersion" class="info--text">-->
      <!--          点击查看新版本-->
      <!--        </div>-->
    </v-sheet>

    <v-divider></v-divider>
    <!--  -->
    <v-list v-model:opened="open">
      <v-list-item link to="/yuzu">
        <template v-slot:prepend>
          <v-img src="@/assets/yuzu.webp" style="margin-right: 12px" height="24" width="24"></v-img>
        </template>
        <v-list-item-title>Yuzu 模拟器</v-list-item-title>
      </v-list-item>

      <v-list-item link to="/ryujinx">
        <template v-slot:prepend>
          <v-img src="@/assets/ryujinx.webp" style="margin-right: 12px" height="24" width="24"></v-img>
        </template>
        <v-list-item-title>Ryujinx 模拟器</v-list-item-title>
      </v-list-item>

      <v-list-item link to="/keys">
        <template v-slot:prepend>
          <div class="my-prepend-box">
            <v-icon color="amber darken-2" :icon="mdiKeyVariant"></v-icon>
          </div>
        </template>
        <v-list-item-title>密钥管理</v-list-item-title>
      </v-list-item>


      <v-list-group value="experiment">
        <template v-slot:activator="{ props }">
          <v-list-item v-bind="props" title="实验性功能">
            <template v-slot:prepend>
              <div class="my-prepend-box">
                <v-icon color="blue lighten-2" :icon="mdiTestTube"></v-icon>
              </div>
            </template>
          </v-list-item>
        </template>
        <v-list-item link to="/yuzuCheatsManagement">
          <template v-slot:prepend>
            <div class="my-prepend-box">
              <v-icon color="indigo accent-2" :icon="mdiMemory"></v-icon>
            </div>
          </template>
          <v-list-item-title class="text--primary">Yuzu 金手指管理</v-list-item-title>
        </v-list-item>
        <v-list-item link to="/yuzuSaveManagement" style="padding-left: 30px">
          <template v-slot:prepend>
            <div class="my-prepend-box">
              <v-icon color="blue darken-1" :icon="mdiContentSaveMoveOutline"></v-icon>
            </div>
          </template>
          <v-list-item-title class="text--primary">Yuzu 存档备份</v-list-item-title>
        </v-list-item>
        <v-list-item link to="/cloudflareST" style="padding-left: 30px">
          <template v-slot:prepend>
            <div class="my-prepend-box">
              <v-icon color="error" :icon="mdiSpeedometer"></v-icon>
            </div>
          </template>
          <v-list-item-title class="text--primary">Cloudflare 节点选优</v-list-item-title>
        </v-list-item>
      </v-list-group>

      <v-list-item link to="/settings">
        <template v-slot:prepend>
          <div class="my-prepend-box">
            <v-icon color="blue-grey lighten-3" :icon="mdiCog"></v-icon>
          </div>
        </template>
        <v-list-item-title>设置</v-list-item-title>
      </v-list-item>

      <v-list-item link to="/otherLinks">
        <template v-slot:prepend>
          <div class="my-prepend-box">
            <v-icon color="warning" :icon="mdiLinkVariant"></v-icon>
          </div>
        </template>
        <v-list-item-title>其他资源</v-list-item-title>
      </v-list-item>

      <v-list-item link to="/faq">
        <template v-slot:prepend>
          <div class="my-prepend-box">
            <v-icon color="light-green darken-2" :icon="mdiCommentQuestionOutline"></v-icon>
          </div>
        </template>
        <v-list-item-title>常见问题</v-list-item-title>
      </v-list-item>

      <v-list-item link to="/about">
        <template v-slot:prepend>
          <div class="my-prepend-box">
            <v-icon color="info" :icon="mdiInformation"></v-icon>
          </div>
        </template>
        <v-list-item-title>About</v-list-item-title>
      </v-list-item>
    </v-list>
  </v-navigation-drawer>
</template>

<style scoped>
.my-prepend-box {
  margin-right: 12px;
}

div.v-list-group__items > a.v-list-item {
  padding-left: 36px !important;
}
</style>
