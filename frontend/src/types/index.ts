// Composables

export interface AppConfig {
  yuzu: YuzuConfig
  suyu: SuyuConfig
  ryujinx: RyujinxConfig
  setting: Setting
}

export interface YuzuConfig {
  yuzu_path: string
  yuzu_version: string
  yuzu_firmware: string
  branch: string
}

export interface SuyuConfig {
  path: string
  version: string
  firmware: string
  branch: string
}

export interface RyujinxConfig {
  path: string
  version: string
  firmware: string
  branch: string
}

export interface Setting {
  ui: UiSetting
  network: NetworkSetting
  download: DownloadSetting
  other: OtherSetting
}

export interface OtherSetting {
  rename_yuzu_to_cemu: boolean
}

export interface UiSetting {
  lastOpenEmuPage: string
  dark: boolean
  mode: string
  width: number
  height: number
}

export interface NetworkSetting {
  firmwareDownloadSource: string
  githubApiMode: string
  githubDownloadMirror: string
  useDoh: boolean
  proxy: string
}

export interface DownloadSetting {
  autoDeleteAfterInstall: boolean
  disableAria2Ipv6: boolean
  removeOldAria2LogFile: boolean
  verifyFirmwareMd5: boolean
}

export interface CommonResponse {
  code: number,
  msg: string,
  data: any
}

export interface CheatGameInfo {
  game_id: string,
  game_name: string,
  cheats_path: string,
}

export interface CheatItem {
  enable: boolean,
  title: string,
}

export interface CheatFileInfo {
  path: string,
  name: string,
}

export interface NameValueItem {
  name: string,
  value: string,
}

export interface YuzuSaveUserListItem {
  user_id: string,
  folder: string
}

export interface YuzuSaveBackupListItem {
  game_name: string,
  title_id: string,
  bak_time: number,
  filename: string,
  path: string,
}

export interface SaveGameInfo {
  title_id: string,
  game_name: string,
  folder: string,
}
