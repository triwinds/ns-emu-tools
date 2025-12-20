// Utilities
import { defineStore } from 'pinia'
import type {CheatGameInfo} from "@/types";
import {useConsoleDialogStore} from "@/stores/ConsoleDialogStore";
import { getAvailableFirmwareInfos, getGameData, type FirmwareInfo } from '@/utils/tauri'

const cds = useConsoleDialogStore()

export const useAppStore = defineStore('app', {
  state: () => ({
    targetFirmwareVersion: null as string | null,
    availableFirmwareInfos: [] as FirmwareInfo[],
    gameData: {} as {[key: string]: string}
  }),
  getters: {
    gameDataInited(state) {
        return Object.keys(state.gameData).length !== 0
    }
  },
  actions: {
    async updateAvailableFirmwareInfos() {
        this.targetFirmwareVersion = null
        try {
          const infos = await getAvailableFirmwareInfos()
          this.availableFirmwareInfos = infos
          this.targetFirmwareVersion = infos[0]?.version ?? null
        } catch (error) {
          cds.showConsoleDialog()
          cds.appendConsoleMessage('固件信息加载异常: ' + error)
          console.error('获取固件信息失败:', error)
        }
    },
    async loadGameData() {
      if (this.gameDataInited && !('unknown' in this.gameData)) {
          return this.gameData
      }
      try {
        const gameData = await getGameData()
        this.gameData = gameData && Object.keys(gameData).length > 0 ? gameData : {'unknown': 'unknown'}
        return this.gameData
      } catch (error) {
        console.error('获取游戏数据失败:', error)
        this.gameData = {'unknown': 'unknown'}
        return this.gameData
      }
    }
  }
})
