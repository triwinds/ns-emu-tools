const vm = new Vue({
    el: '#root',
    data: {
        yuzuConfig: {},
        allYuzuReleaseInfos: [],
        availableFirmwareInfos: [],
        availableKeyInfos: [],
        targetYuzuVersion: "",
        targetFirmwareVersion: "",
        targetKeyName: "",
        topBarMsg: "",
        isRunningInstall: false,
        currentVersion: '',
    },
    created() {
        this.initCurrentVersion()
        this.updateYuzuConfig()
        this.updateYuzuReleaseInfos()
        this.updateKeysInfo()
        this.updateAvailableFirmwareInfos()
        this.topBarMsg = '启动完毕'
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
        updateYuzuConfig() {
            eel.get_yuzu_config()((config) => {
                this.yuzuConfig = config
            })
        },
        updateYuzuReleaseInfos() {
            eel.get_yuzu_release_infos()((data) => {
                if (data['code'] === 0) {
                    let infos = data['data']
                    this.allYuzuReleaseInfos = infos
                    this.targetYuzuVersion = infos[0]['tag_name'].substring(3)
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
        installYuzu() {
            this.isRunningInstall = true
            eel.install_yuzu(this.targetYuzuVersion)((resp) => {
                this.isRunningInstall = false
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            });
        },
        installFirmware() {
            this.isRunningInstall = true
            eel.install_firmware(this.targetFirmwareVersion)((resp) => {
                this.isRunningInstall = false
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            })
        },
        installKeys() {
            this.isRunningInstall = true
            eel.install_keys(this.targetKeyName)((resp) => {
                this.isRunningInstall = false
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            })
        },
        updateTopBarMsg(msg) {
            this.topBarMsg = msg
        },
        detectYuzuVersion() {
            eel.detect_yuzu_version()((info) => {
                this.updateYuzuConfig()
            })
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
    },
    computed: {
        latestFirmwareVersion: function () {
            if (this.availableFirmwareInfos.length > 0) {
                return this.availableFirmwareInfos[0]['version']
            }
            return "加载中"
        },
        latestYuzuVersion: function () {
            if (this.allYuzuReleaseInfos.length > 0) {
                return this.allYuzuReleaseInfos[0]['tag_name'].substring(3)
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
