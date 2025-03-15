// Utilities
import { defineStore } from 'pinia'
import type {CommonResponse, YuzuSaveUserListItem} from "@/types";



export const useYuzuSaveStore = defineStore('yuzuSave', {
  state: () => ({
    userList: [] as YuzuSaveUserListItem[],
    selectedUser: '',
    yuzuSaveBackupPath: ''
  }),
  actions: {

  }
})
