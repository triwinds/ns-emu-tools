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
        topBarMsg: ""
    },
    created() {
        this.updateYuzuConfig()
        this.updateYuzuReleaseInfos()
        this.updateAvailableFirmwareInfos()
        this.updateKeysInfo()
        this.topBarMsg = '启动完毕'
    },
    methods: {
        updateYuzuConfig() {
            eel.get_yuzu_config()((config) => {
                this.yuzuConfig = config
            })
        },
        updateYuzuReleaseInfos() {
            eel.get_yuzu_release_infos()((infos) => {
                this.allYuzuReleaseInfos = infos
                this.targetYuzuVersion = infos[0]['tag_name'].substring(3)
            })
        },
        updateAvailableFirmwareInfos() {
            eel.get_available_firmware_infos()((infos) => {
                this.availableFirmwareInfos = infos
                this.targetFirmwareVersion = infos[0]['version']
            })
        },
        updateKeysInfo() {
            eel.get_available_keys_info()((info) => {
                res = []
                for (let key in info) {
                    // console.log(key, info[key]);
                    res.push(info[key])
                }
                this.availableKeyInfos = res.reverse()
                this.targetKeyName = this.availableKeyInfos[0]['name']
            })
        },
        installYuzu() {
            eel.install_yuzu(this.targetYuzuVersion)((resp) => {
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            });
        },
        installFirmware() {
            eel.install_firmware(this.targetFirmwareVersion)((resp) => {
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            })
        },
        installKeys() {
            eel.install_keys(this.targetKeyName)((resp) => {
                this.topBarMsg = resp['msg']
                this.updateYuzuConfig()
            })
        },
        updateTopBarMsg(msg) {
            this.topBarMsg = msg
        }
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
    }
});

eel.expose(updateTopBarMsg);
function updateTopBarMsg(msg) {
    vm.updateTopBarMsg(msg)
}
