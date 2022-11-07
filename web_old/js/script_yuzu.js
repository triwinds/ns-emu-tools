const vm = new Vue({
    el: '#root',
    data: {
        yuzuConfig: {},
        branch: 'ea',
        allYuzuReleaseVersions: [],
        availableFirmwareInfos: [],
        targetYuzuVersion: "",
        targetFirmwareVersion: "",
        targetKeyName: "",
        topBarMsg: "",
        isRunningInstall: false,
        currentVersion: '',
        hasNewVersion: false,
    },
    async created() {
        this.initCurrentVersion()
        this.updateAvailableFirmwareInfos()
        await this.updateYuzuConfig()
        this.updateYuzuReleaseVersions()
        this.topBarMsg = '启动完毕'
        this.checkUpdate()
        eel.update_last_open_emu_page('yuzu')()
    },
    methods: {
        initCurrentVersion() {
            eel.get_current_version()((data) => {
                if (data['code'] === 0) {
                    this.currentVersion = data['data']
                } else {
                    this.currentVersion = '未知'
                }
            })
        },
        async updateYuzuConfig() {
            this.yuzuConfig = await eel.get_yuzu_config()()
            this.branch = this.yuzuConfig.branch
        },
        switchYuzuBranch() {
            eel.switch_yuzu_branch()((config) => {
                this.yuzuConfig = config
                this.branch = config.branch
                this.updateYuzuReleaseVersions()
            })
        },
        updateYuzuReleaseVersions() {
            eel.get_all_yuzu_release_versions()((data) => {
                if (data['code'] === 0) {
                    let infos = data['data']
                    this.allYuzuReleaseVersions = infos
                    this.targetYuzuVersion = infos[0]
                } else {
                    this.topBarMsg = 'yuzu 版本信息加载异常.'
                }
            })
        },
        updateAvailableFirmwareInfos() {
            eel.get_available_firmware_infos()((data) => {
                if (data['code'] === 0) {
                    let infos = data['data']
                    this.availableFirmwareInfos = infos
                    this.targetFirmwareVersion = infos[0]['version']
                } else {
                    this.topBarMsg = '固件信息加载异常.'
                }
            })
        },
        installYuzu() {
            this.isRunningInstall = true
            eel.install_yuzu(this.targetYuzuVersion, this.branch)((resp) => {
                this.isRunningInstall = false
                this.updateYuzuConfig()
            });
        },
        installFirmware() {
            this.isRunningInstall = true
            eel.install_yuzu_firmware(this.targetFirmwareVersion)((resp) => {
                this.isRunningInstall = false
                if (resp['msg']) {
                    this.topBarMsg = resp['msg']
                }
                this.updateYuzuConfig()
            })
        },
        updateTopBarMsg(msg) {
            this.topBarMsg = msg
        },
        async detectYuzuVersion() {
            let previousBranch = this.branch
            let data = await eel.detect_yuzu_version()()
            if (data['code'] === 0) {
                await this.updateYuzuConfig()
                if (previousBranch !== this.branch) {
                    this.updateYuzuReleaseVersions()
                }
            } else {
                this.topBarMsg = '检测 yuzu 版本时发生异常'
            }
        },
        modifyYuzuPath() {
            eel.ask_and_update_yuzu_path()((data) => {
                if (data['code'] === 0) {
                    this.updateYuzuConfig()
                }
                this.topBarMsg = data['msg']
            })
        },
        startYuzu() {
            eel.start_yuzu()((data) => {
                if (data['code'] === 0) {
                    this.topBarMsg = 'yuzu 启动成功'
                } else {
                    this.topBarMsg = 'yuzu 启动失败'
                }
            })
        },
        checkUpdate() {
            eel.check_update()((data) => {
                if (data['code'] === 0 && data['data']) {
                    this.topBarMsg = `检测到新版本 [${data['msg']}], 点击下方标题查看更新`;
                    this.hasNewVersion = true
                }
            })
        },
        clickTitle() {
            if (this.hasNewVersion) {
                window.open('https://github.com/triwinds/ns-emu-tools/releases', '_blank');
            }
        },
        async detectFirmwareVersion() {
            eel.detect_firmware_version("yuzu")((data) => {
                if (data['code'] === 0) {
                    this.updateYuzuConfig()
                }
            })
        },
    },
    computed: {
        latestFirmwareVersion: function () {
            if (this.availableFirmwareInfos.length > 0) {
                return this.availableFirmwareInfos[0]['version']
            }
            return "加载中"
        },
        latestYuzuVersion: function () {
            if (this.allYuzuReleaseVersions.length > 0) {
                return this.allYuzuReleaseVersions[0]
            }
            return "加载中"
        },
        displayTopBarMsg: function () {
            const maxLength = 42
            if (this.topBarMsg.length > maxLength) {
                return this.topBarMsg.substring(0, maxLength) + '...'
            }
            return this.topBarMsg
        },
        displayBranch: function () {
            if (this.branch === 'ea') {
                return 'EA'
            } else if (this.branch === 'mainline') {
                return '主线'
            }
            return '未知'
        }
    }
});

let tooltipTriggerList = [].slice.call(document.querySelectorAll('[data-bs-toggle="tooltip"]'));
let tooltipList = tooltipTriggerList.map(function (tooltipTriggerEl) {
    return new bootstrap.Tooltip(tooltipTriggerEl)
})

eel.expose(updateTopBarMsg);

function updateTopBarMsg(msg) {
    vm.updateTopBarMsg(msg)
}
