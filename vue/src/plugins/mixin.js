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
        openUrlWithDefaultBrowser(url) {
            window.eel.open_url_in_default_browser(url)()
        },
        checkUpdate(forceShowDialog) {
            window.eel.check_update()((data) => {
                if (data['code'] === 0 && data['data']) {
                    this.$store.commit('UPDATE_HAS_NEW_VERSION', true)
                }
                if (forceShowDialog || this.$store.state.hasNewVersion) {
                    this.$bus.$emit('showNewVersionDialog',
                        {hasNewVersion: this.$store.state.hasNewVersion, latestVersion: data['msg']})
                }
            })
        },
        async loadGameData() {
            if (this.gameDataInited && !('unknown' in this.$store.state.gameData)) {
                return this.$store.state.gameData
            }
            let resp = await window.eel.get_game_data()()
            let gameData = resp.code === 0 ? resp.data : {'unknown': 'unknown'}
            this.$store.commit('UPDATE_GAME_DATA', gameData)
            return gameData
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
        availableFirmwareVersions: function () {
            return this.$store.state.availableFirmwareInfos.map(info => info['version'])
        },
        yuzuConfig() {
            return this.$store.state.config.yuzu
        },
        ryujinxConfig() {
            return this.$store.state.config.ryujinx
        },
        gameDataInited() {
            return Object.keys(this.$store.state.gameData).length !== 0
        },
    },
})
