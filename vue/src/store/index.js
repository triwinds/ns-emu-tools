import Vue from 'vue'
import Vuex from 'vuex'

Vue.use(Vuex)


const actions = {
    updateAvailableFirmwareInfos(context) {
        window.eel.get_available_firmware_infos()((data) => {
            if (data['code'] === 0) {
                let infos = data['data']
                context.commit('UPDATE_AVAILABLE_FIRMWARE_INFOS', infos)
                context.commit('UPDATE_TARGET_FIRMWARE_VERSION', infos[0]['version'])
            } else {
                this.showConsoleDialog()
                this.appendConsoleMessage('固件信息加载异常.')
            }
        })
    },
    async loadConfig(context) {
        let resp = await window.eel.get_config()()
        if (resp.code === 0) {
            let config = resp.data
            context.commit('UPDATE_CONFIG', config)
            return config
        } else {
            console.log(`fail to get config, resp: ${resp}`)
        }
        return {}
    },
    initCurrentVersion(context) {
        window.eel.get_current_version()((data) => {
            if (data['code'] === 0) {
                context.commit('UPDATE_CURRENT_VERSION', data['data'])
            } else {
                context.commit('UPDATE_CURRENT_VERSION', '未知')
            }
        })
    },
}

const mutations = {
    UPDATE_CURRENT_VERSION(state, value) {
        state.currentVersion = value
    },
    UPDATE_HAS_NEW_VERSION(state, value) {
        state.hasNewVersion = value
    },
    SET_CONSOLE_DIALOG_FLAG(state, value) {
        state.consoleDialogFlag = value;
    },
    APPEND_CONSOLE_MESSAGE(state, message) {
        if (!message) {
            return
        }
        let splits = message.split('\n')
        for (let value of splits) {
            if (value.length < 1) {
                continue
            }
            if (value && value.startsWith('下载速度: ') && state.consoleMessages.length > 0
                && state.consoleMessages[state.consoleMessages.length - 1].startsWith('下载速度: ')) {
                Vue.set(state.consoleMessages, state.consoleMessages.length - 1, value)
            } else {
                state.consoleMessages.push(value)
            }
        }
    },
    CLEAR_CONSOLE_MESSAGES(state) {
        state.consoleMessages = []
    },
    UPDATE_AVAILABLE_FIRMWARE_INFOS(state, value) {
        state.availableFirmwareInfos = value
    },
    UPDATE_TARGET_FIRMWARE_VERSION(state, value) {
        state.targetFirmwareVersion = value
    },
    UPDATE_CONFIG(state, config) {
        state.config = config
    }
}

const state = {
    consoleDialogFlag: false,
    consoleMessages: [],
    availableFirmwareInfos: [],
    targetFirmwareVersion: '',
    currentVersion: '',
    hasNewVersion: false,
    config: {
        yuzu: {
            yuzu_path: "",
            yuzu_version: "",
            yuzu_firmware: "",
            branch: ""
        },
        ryujinx: {
            path: "",
            version: "",
            firmware: "",
            branch: ""
        },
        setting: {
            ui: {
                lastOpenEmuPage: "",
                dark: true
            },
            network: {
                firmwareSource: 'auto-detect',
                githubApiMode: 'direct',
                githubDownloadSource: "self"
            },
            download: {
                autoDeleteAfterInstall: true,
                disableAria2Ipv6: true,
                removeOldAria2LogFile: true,
                useDoh: true,
            }
        },
    },
}


export default new Vuex.Store({
    actions,
    mutations,
    state
})
