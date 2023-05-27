<template>
  <v-container>
    <v-row>
      <v-col cols="7">
        <v-text-field label="备份文件存放的文件夹" hide-details readonly v-model="yuzuSaveBackupPath"></v-text-field>
      </v-col>
      <v-col cols="2">
        <v-btn color="info" outlined @click="askAndUpdateYuzuBackupPath">选择文件夹</v-btn>
      </v-col>
      <v-col cols="2">
        <v-btn color="success" outlined @click="openYuzuBackupFolder">打开文件夹</v-btn>
      </v-col>
    </v-row>
    <v-row>
      <v-col>
        <v-autocomplete v-model="selectedUser" :items="userList" label="选择模拟器用户 ID"
                        hint="模拟器的用户 ID 可以在菜单 模拟->设置->系统->配置 中查看" persistent-hint
                        item-text="user_id" item-value="folder" @change="onUserChange"
                        style="margin-bottom: 20px"></v-autocomplete>
      </v-col>
    </v-row>

  </v-container>
</template>

<script>
export default {
  name: "YuzuSaveCommonPart",
  data() {
    return {
      userList: [],
      selectedUser: '',
      yuzuSaveBackupPath: '',
    }
  },
  mounted() {
    window.eel.get_users_in_save()((resp) => {
      if (resp.code === 0) {
        this.userList = resp.data
      }
    })
    this.$bus.$on('yuzuSave:selectedUser', newUser => {
      this.selectedUser = newUser
    })
    this.loadYuzuSaveBackupPath()
  },
  beforeDestroy() {
    this.$bus.$off('yuzuSave:selectedUser')
  },
  methods: {
    onUserChange() {
      this.$bus.$emit('yuzuSave:selectedUser', this.selectedUser)
    },
    async askAndUpdateYuzuBackupPath() {
      await window.eel.ask_and_update_yuzu_save_backup_folder()()
      await this.loadYuzuSaveBackupPath()
    },
    async loadYuzuSaveBackupPath() {
      let resp = await window.eel.get_storage()()
      this.yuzuSaveBackupPath = resp.data.yuzu_save_backup_path
    },
    openYuzuBackupFolder() {
      window.eel.open_yuzu_save_backup_folder()()
    },
  }
}
</script>

<style scoped>

</style>