import Vue from 'vue'
import Vuex from 'vuex'

Vue.use(Vuex)


const actions = {}

const mutations = {
    SET_CONSOLE_DIALOG_FLAG(state, value) {
        state.consoleDialogFlag = value;
    },
    APPEND_CONSOLE_MESSAGE(state, value) {
        state.consoleMessages.push(value)
    },
    CLEAR_CONSOLE_MESSAGES(state) {
        state.consoleMessages = []
    }
}

const state = {
    consoleDialogFlag: false,
    consoleMessages: [],
}


export default new Vuex.Store({
    actions,
    mutations,
    state
})
