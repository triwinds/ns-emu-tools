// Utilities
import { defineStore } from 'pinia'
import type { AppConfig } from "@/types";
import { getAppVersion, getConfig, checkUpdate, type UpdateCheckResult } from "@/utils/tauri";

export const useConfigStore = defineStore('config', {
  state: () => ({
    config: {
      yuzu: {},
      ryujinx: {},
      setting: {}
    } as AppConfig,
    currentVersion: '',
    hasNewVersion: false,
    updateInfo: null as UpdateCheckResult | null,
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
        console.log('[ConfigStore] 开始检查更新...')
        const result = await checkUpdate(false)
        console.log('[ConfigStore] 更新检查结果:', {
          hasUpdate: result.hasUpdate,
          currentVersion: result.currentVersion,
          latestVersion: result.latestVersion,
          downloadUrl: result.downloadUrl,
          htmlUrl: result.htmlUrl
        })

        this.hasNewVersion = result.hasUpdate
        this.updateInfo = result
        console.log('[ConfigStore] updateInfo 已保存:', this.updateInfo)

        if (forceShowDialog || this.hasNewVersion) {
          console.log('[ConfigStore] 显示更新对话框')
          window.$bus.emit('showNewVersionDialog', {
            hasNewVersion: this.hasNewVersion,
            latestVersion: result.latestVersion
          })
        }
      } catch (e) {
        console.error('[ConfigStore] 检查更新失败:', e)
      }
    },
  },
  getters: {
    yuzuConfig(state) {
      return state.config.yuzu
    }
  }
})
