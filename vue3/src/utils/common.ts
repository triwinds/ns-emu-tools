// Utilities


import {useAppStore} from "@/store/app";

const appStore = useAppStore()

export function openUrlWithDefaultBrowser(url: string) {
    window.eel.open_url_in_default_browser(url)()
}

export async function loadGameData() {
    if (appStore.gameDataInited && !('unknown' in appStore.gameData)) {
        return appStore.gameData
    }
    const resp = await window.eel.get_game_data()()
    const gameData = resp.code === 0 ? resp.data : {'unknown': 'unknown'}
    appStore.gameData = gameData
    return gameData
}
