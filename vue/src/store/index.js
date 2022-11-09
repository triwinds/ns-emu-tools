import Vue from 'vue'
import Vuex from 'vuex'

Vue.use(Vuex)


const actions = {}

const mutations = {
    SET_CONSOLE_DIALOG_FLAG(state, value) {
        state.consoleDialogFlag = value;
    }
}

const state = {
    consoleDialogFlag: false,
}


export default new Vuex.Store({
    actions,
    mutations,
    state
})
