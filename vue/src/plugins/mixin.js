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
        },
        updateAvailableFirmwareInfos() {
            this.$store.dispatch('updateAvailableFirmwareInfos')
        },
        initAvailableFirmwareInfos() {
            if (this.$store.state.availableFirmwareInfos.length === 0) {
                this.updateAvailableFirmwareInfos()
            }
        },
    },
    computed: {
        targetFirmwareVersion: {
            get() {
                return this.$store.state.targetFirmwareVersion
            },
            set(value) {
                this.$store.commit('UPDATE_TARGET_FIRMWARE_VERSION', value)
            }
        },
        latestFirmwareVersion: function () {
            if (this.$store.state.availableFirmwareInfos.length > 0) {
                return this.$store.state.availableFirmwareInfos[0]['version']
            }
            return "加载中"
        },
    },
})
