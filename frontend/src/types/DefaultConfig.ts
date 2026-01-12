import type {AppConfig} from "@/types";

// 注意：这些是示例默认值，实际路径会根据运行平台自动设置
// Windows: D:\Yuzu, D:\Ryujinx
// macOS: /Applications
// Linux: ~/Yuzu, ~/Ryujinx
export const defaultConfig: AppConfig = {
  "yuzu": {
    "yuzu_path": "D:\\Yuzu",
    "yuzu_version": "",
    "yuzu_firmware": "",
    "branch": "eden"
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
      "ryujinxGitLabDownloadMirror": "direct",
      "useDoh": true,
      "proxy": "system"
    },
    "download": {
      "autoDeleteAfterInstall": true,
      "disableAria2Ipv6": true,
      "removeOldAria2LogFile": true,
      "backend": "auto"
    },
    "other": {
      "rename_yuzu_to_cemu": false
    }
  }
}
