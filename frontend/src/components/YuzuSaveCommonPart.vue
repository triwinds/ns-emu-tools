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
import type {CommonResponse} from "@/types";
import {useYuzuSaveStore} from "@/stores/YuzuSaveStore";
import {invoke} from '@tauri-apps/api/core';
import {open} from '@tauri-apps/plugin-dialog';

const yuzuSaveStore = useYuzuSaveStore()

onMounted(async () => {
  const resp = await invoke<CommonResponse<{user_id: string, folder: string}[]>>('get_users_in_save_cmd')
  if (resp.code === 0) {
    yuzuSaveStore.userList = resp.data || []
  }
  await loadYuzuSaveBackupPath()
})

async function askAndUpdateYuzuBackupPath() {
  const selected = await open({
    directory: true,
    multiple: false,
  });

  if (selected && typeof selected === 'string') {
    await invoke('update_yuzu_save_backup_folder_cmd', {
      folder: selected
    })
    await loadYuzuSaveBackupPath()
  }
}

async function loadYuzuSaveBackupPath() {
  const resp = await invoke<CommonResponse<string>>('get_yuzu_save_backup_folder_cmd')
  if (resp.code === 0) {
    yuzuSaveStore.yuzuSaveBackupPath = resp.data || ''
  }
}

async function openYuzuBackupFolder() {
  await invoke('open_yuzu_save_backup_folder_cmd')
}
</script>

<style scoped>

</style>
