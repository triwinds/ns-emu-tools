// Utilities
import { defineStore } from 'pinia'
import type { AppConfig } from "@/types";
import { getAppVersion, getConfig, checkUpdate } from "@/utils/tauri";

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
      try {
        const config = await getConfig()
        this.config = config as unknown as AppConfig
      } catch (e) {
        console.error('Failed to load config:', e)
      }
    },
    async initCurrentVersion() {
      try {
        this.currentVersion = await getAppVersion()
      } catch (e) {
        console.error('Failed to get app version:', e)
        this.currentVersion = '未知'
      }
    },
    async checkUpdate(forceShowDialog: boolean) {
      try {
        const result = await checkUpdate(false)
        this.hasNewVersion = result.hasUpdate

        if (forceShowDialog || this.hasNewVersion) {
          window.$bus.emit('showNewVersionDialog', {
            hasNewVersion: this.hasNewVersion,
            latestVersion: result.latestVersion
          })
        }
      } catch (e) {
        console.error('Failed to check update:', e)
      }
    },
  },
  getters: {
    yuzuConfig(state) {
      return state.config.yuzu
    }
  }
})
