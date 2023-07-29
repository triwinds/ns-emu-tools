// Utilities
import { defineStore } from 'pinia'

export const useAppStore = defineStore('app', {
  state: () => ({
    targetFirmwareVersion: '' || null,
    availableFirmwareInfos: []
  }),
  getters: {
    availableFirmwareVersions(state) {
      return state.availableFirmwareInfos.map(info => info['version'])
    }
  }
})
