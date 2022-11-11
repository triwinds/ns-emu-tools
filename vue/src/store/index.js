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
}

const mutations = {
    SET_CONSOLE_DIALOG_FLAG(state, value) {
        state.consoleDialogFlag = value;
    },
    APPEND_CONSOLE_MESSAGE(state, value) {
        if (value && value.startsWith('下载速度: ') && state.consoleMessages.length > 0
            && state.consoleMessages[state.consoleMessages.length - 1].startsWith('下载速度: ')) {
            Vue.set(state.consoleMessages, state.consoleMessages.length - 1, value)
        } else {
            state.consoleMessages.push(value)
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
    }
}

const state = {
    consoleDialogFlag: false,
    consoleMessages: [],
    availableFirmwareInfos: [],
    targetFirmwareVersion: ''
}


export default new Vuex.Store({
    actions,
    mutations,
    state
})
