// Composables

export interface AppConfig {
  yuzu: YuzuConfig
  ryujinx: RyujinxConfig
  setting: Setting
}

export interface YuzuConfig {
  yuzu_path: string
  yuzu_version: string | null
  yuzu_firmware: string | null
  branch: string
}

export interface RyujinxConfig {
  path: string
  version: string | null
  firmware: string | null
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
  ryujinxGitLabDownloadMirror: string
  useDoh: boolean
  proxy: string
}

export interface DownloadSetting {
  autoDeleteAfterInstall: boolean
  disableAria2Ipv6: boolean
  removeOldAria2LogFile: boolean
}

export interface CommonResponse<T = unknown> {
  code: number
  msg?: string
  data?: T
}

// ============ Tauri 事件类型 ============

/** 下载进度 */
export interface DownloadProgress {
  downloaded: number
  total: number
  speed: number
  percentage: number
  eta?: number
}

/** 安装进度 */
export interface InstallProgress {
  stage: string
  step: number
  total_steps: number
  message?: string
  download?: DownloadProgress
}

/** 消息类型 */
export type MessageType = 'info' | 'success' | 'warning' | 'error'

/** 通知消息 */
export interface NotifyMessage {
  type: MessageType
  content: string
  persistent: boolean
}

// ============ Release 信息 ============

/** Release 资源 */
export interface ReleaseAsset {
  name: string
  download_url: string
  size: number
  content_type?: string
}

/** Release 信息 */
export interface ReleaseInfo {
  name: string
  tag_name: string
  description: string
  assets: ReleaseAsset[]
  published_at?: string
  prerelease: boolean
  html_url?: string
}

// ============ 存储类型 ============

/** 存储数据 */
export interface Storage {
  yuzu_history: Record<string, YuzuConfig>
  ryujinx_history: Record<string, RyujinxConfig>
  yuzu_save_backup_path: string
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
