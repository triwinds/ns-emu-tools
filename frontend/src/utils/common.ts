// Utilities

import {useAppStore} from "@/stores/app";
import { openUrl, getGameData as getGameDataFromTauri } from "@/utils/tauri";

const appStore = useAppStore()

export function openUrlWithDefaultBrowser(url: string) {
    openUrl(url)
}

export async function loadGameData(): Promise<Record<string, any>> {
    if (appStore.gameDataInited && !('unknown' in appStore.gameData)) {
        return appStore.gameData
    }
    try {
        const gameData = await getGameDataFromTauri()
        appStore.gameData = gameData
        return gameData
    } catch (e) {
        console.error('Failed to load game data:', e)
        const fallback = {'unknown': 'unknown'}
        appStore.gameData = fallback
        return fallback
    }
}
