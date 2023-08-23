// Utilities
import { defineStore } from 'pinia'
import {CommonResponse, YuzuSaveUserListItem} from "@/types";



export const useYuzuSaveStore = defineStore('yuzuSave', {
  state: () => ({
    userList: [] as YuzuSaveUserListItem[],
    selectedUser: '',
    yuzuSaveBackupPath: ''
  }),
  actions: {

  }
})
