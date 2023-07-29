// Utilities
import { defineStore } from 'pinia'
import {AppConfig, CommonResponse} from "@/types";

export const useConfigStore = defineStore('config', {
  state: () => ({
    config: {
      yuzu: {},
      ryujinx: {},
      setting: {}
    } as AppConfig,
    currentVersion: '',
    hasNewVersion: false,
  }),
  actions: {
    async reloadConfig() {
      const resp = await window.eel.get_config()()
      if (resp.code === 0) {
        this.config = resp.data
      }
    },
    initCurrentVersion() {
      window.eel.get_current_version()((resp: CommonResponse) => {
        if (resp['code'] === 0) {
          this.currentVersion = resp.data
        } else {
          this.currentVersion = '未知'
        }
      })
    },
    checkUpdate(forceShowDialog: boolean) {
      window.eel.check_update()((data: CommonResponse) => {
        if (data['code'] === 0 && data['data']) {
            this.hasNewVersion = true
        }
        if (forceShowDialog || this.hasNewVersion) {
            window.$bus.emit('showNewVersionDialog',
                {hasNewVersion: this.hasNewVersion, latestVersion: data['msg']})
        }
      })
    },
  },
  getters: {
    yuzuConfig(state) {
      return state.config.yuzu
    }
  }
})
