<template>
  <v-container>
    <v-row>
      <v-col cols="7">
        <v-text-field label="备份文件存放的文件夹" variant="underlined" hide-details readonly v-model="yuzuSaveStore.yuzuSaveBackupPath"></v-text-field>
      </v-col>
      <v-col cols="2">
        <v-btn color="info" min-width="110" size="large" variant="outlined" @click="askAndUpdateYuzuBackupPath">选择文件夹</v-btn>
      </v-col>
      <v-col cols="2">
        <v-btn color="success" min-width="110" size="large" variant="outlined" @click="openYuzuBackupFolder">打开文件夹</v-btn>
      </v-col>
    </v-row>
    <v-row>
      <v-col>
        <v-autocomplete v-model="yuzuSaveStore.selectedUser" :items="yuzuSaveStore.userList" label="选择模拟器用户 ID"
                        hint="模拟器的用户 ID 可以在菜单 模拟->设置->系统->配置 中查看" persistent-hint
                        item-title="user_id" item-value="folder" variant="underlined"
                        style="margin-bottom: 20px"></v-autocomplete>
      </v-col>
    </v-row>

  </v-container>
</template>

<script lang="ts" setup>
import {onMounted} from "vue";
import {CommonResponse} from "@/types";
import {useYuzuSaveStore} from "@/store/YuzuSaveStore";

const yuzuSaveStore = useYuzuSaveStore()
onMounted(() => {
  window.eel.get_users_in_save()((resp: CommonResponse) => {
    if (resp.code === 0) {
      yuzuSaveStore.userList = resp.data
    }
  })
  loadYuzuSaveBackupPath()
})
async function askAndUpdateYuzuBackupPath() {
  await window.eel.ask_and_update_yuzu_save_backup_folder()()
  await loadYuzuSaveBackupPath()
}

async function loadYuzuSaveBackupPath() {
  let resp = await window.eel.get_storage()()
  yuzuSaveStore.yuzuSaveBackupPath = resp.data.yuzu_save_backup_path
}

function openYuzuBackupFolder() {
  window.eel.open_yuzu_save_backup_folder()()
}
</script>

<style scoped>

</style>
