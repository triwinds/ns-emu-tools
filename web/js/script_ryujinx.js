const vm = new Vue({
    el: '#root',
    data: {
        ryujinxConfig: {},
        allRyujinxReleaseInfos: [],
        availableFirmwareInfos: [],
        availableKeyInfos: [],
        targetRyujinxVersion: "",
        targetFirmwareVersion: "",
        targetKeyName: "",
        topBarMsg: "",
        isRunningInstall: false,
        currentVersion: '',
        hasNewVersion: false,
    },
    created() {
        this.initCurrentVersion()
        this.updateRyujinxConfig()
        this.updateRyujinxReleaseInfos()
        this.updateKeysInfo()
        this.updateAvailableFirmwareInfos()
        this.topBarMsg = '启动完毕'
        this.checkUpdate()
        eel.update_last_open_emu_page('ryujinx')()
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
        updateRyujinxConfig() {
            eel.get_ryujinx_config()((config) => {
                this.ryujinxConfig = config
            })
        },
        updateRyujinxReleaseInfos() {
            eel.get_ryujinx_release_infos()((data) => {
                if (data['code'] === 0) {
                    let infos = data['data']
                    this.allRyujinxReleaseInfos = infos
                    this.targetRyujinxVersion = infos[0]['tag_name']
                } else {
                    this.topBarMsg = 'ryujinx 版本信息加载异常.'
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
        updateKeysInfo() {
            eel.get_available_keys_info()((data) => {
                if (data['code'] === 0) {
                    let info = data['data']
                    res = []
                    for (let key in info) {
                        // console.log(key, info[key]);
                        res.push(info[key])
                    }
                    this.availableKeyInfos = res.reverse()
                    this.targetKeyName = this.availableKeyInfos[0]['name']
                } else {
                    this.topBarMsg = 'key 信息加载异常.'
                }
            })
        },
        installRyujinx() {
            this.isRunningInstall = true
            eel.install_ryujinx(this.targetRyujinxVersion)((resp) => {
                this.isRunningInstall = false
                this.topBarMsg = resp['msg']
                this.updateRyujinxConfig()
            });
        },
        installFirmware() {
            this.isRunningInstall = true
            eel.install_ryujinx_firmware(this.targetFirmwareVersion)((resp) => {
                this.isRunningInstall = false
                if (resp['msg']) {
                    this.topBarMsg = resp['msg']
                }
                this.updateRyujinxConfig()
            })
        },
        installKeys() {
            this.isRunningInstall = true
            eel.install_keys(this.targetKeyName)((resp) => {
                this.isRunningInstall = false
                this.topBarMsg = resp['msg']
                this.updateRyujinxConfig()
            })
        },
        updateTopBarMsg(msg) {
            this.topBarMsg = msg
        },
        detectRyujinxVersion() {
            eel.detect_ryujinx_version()((data) => {
                if (data['code'] === 0) {
                    this.updateRyujinxConfig()
                } else {
                    this.topBarMsg = '检测 Ryujinx 版本时发生异常'
                }
            })
        },
        modifyRyujinxPath() {
            eel.ask_and_update_ryujinx_path()((data) => {
                if (data['code'] === 0) {
                    this.updateRyujinxConfig()
                }
                this.topBarMsg = data['msg']
            })
        },
        startRyujinx() {
            eel.start_ryujinx()((data) => {
                if (data['code'] === 0) {
                    this.topBarMsg = 'Ryujinx 启动成功'
                } else {
                    this.topBarMsg = 'Ryujinx 启动失败'
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
            eel.detect_firmware_version("ryujinx")((data) => {
                if (data['code'] === 0) {
                    this.updateRyujinxConfig()
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
        latestRyujinxVersion: function () {
            if (this.allRyujinxReleaseInfos.length > 0) {
                return this.allRyujinxReleaseInfos[0]['tag_name']
            }
            return "加载中"
        },
        displayTopBarMsg: function () {
            const maxLength = 42
            if (this.topBarMsg.length > maxLength) {
                return this.topBarMsg.substring(0, maxLength) + '...'
            }
            return this.topBarMsg
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
