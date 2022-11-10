import Vue from "vue";


Vue.mixin({
    methods: {
        showConsoleDialog() {
            this.$store.commit('SET_CONSOLE_DIALOG_FLAG', true)
        },
        cleanAndShowConsoleDialog() {
            this.$store.commit('CLEAR_CONSOLE_MESSAGES', true)
            this.showConsoleDialog()
        },
        appendConsoleMessage(msg) {
            this.$store.commit("APPEND_CONSOLE_MESSAGE", msg)
        }
    }
})
