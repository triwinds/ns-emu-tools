// Utilities
import { defineStore } from 'pinia'
import type {AppConfig, CommonResponse} from "@/types";

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
      const resp = await window.eel.get_config()() as CommonResponse<AppConfig>
      if (resp.code === 0 && resp.data) {
        this.config = resp.data
      }
    },
    initCurrentVersion() {
      window.eel.get_current_version()((resp: CommonResponse<string>) => {
        if (resp['code'] === 0) {
          this.currentVersion = resp.data || '未知'
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
