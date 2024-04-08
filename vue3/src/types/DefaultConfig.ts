import {AppConfig} from "@/types";

export const defaultConfig: AppConfig = {
  "yuzu": {
    "yuzu_path": "D:\\Yuzu",
    "yuzu_version": "",
    "yuzu_firmware": "",
    "branch": "ea"
  },
  "suyu": {
    "path": "D:\\Suyu",
    "version": "",
    "firmware": "",
    "branch": "dev"
  },
  "ryujinx": {
    "path": "D:\\Ryujinx",
    "version": "",
    "firmware": "",
    "branch": "ava"
  },
  "setting": {
    "ui": {
      "lastOpenEmuPage": "",
      "dark": true,
      "mode": "auto",
      "width": 1300,
      "height": 850
    },
    "network": {
      "firmwareDownloadSource": "github",
      "githubApiMode": "direct",
      "githubDownloadMirror": "cloudflare_load_balance",
      "useDoh": true,
      "proxy": "system"
    },
    "download": {
      "autoDeleteAfterInstall": true,
      "disableAria2Ipv6": true,
      "removeOldAria2LogFile": true,
      "verifyFirmwareMd5": true
    },
    "other": {
      "rename_yuzu_to_cemu": false
    }
  }
}
