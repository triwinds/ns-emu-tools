/**
 * Tauri API 封装层
 *
 * 提供统一的 API 调用接口，封装 Tauri invoke 和事件监听
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn, type EventCallback } from '@tauri-apps/api/event'

// ============ 类型定义 ============

/** API 响应格式 */
export interface ApiResponse<T> {
  code: number
  data?: T
  msg?: string
}

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

// ============ 事件名称常量 ============

export const Events = {
  INSTALL_PROGRESS: 'install-progress',
  DOWNLOAD_PROGRESS: 'download-progress',
  NOTIFY_MESSAGE: 'notify-message',
  LOG_MESSAGE: 'log-message',
} as const

// ============ Tauri 命令调用封装 ============

/**
 * 调用 Tauri 命令
 * @param cmd 命令名称
 * @param args 命令参数
 * @returns Promise<T>
 */
export async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    console.error(`Tauri command error [${cmd}]:`, error)
    throw error
  }
}

// ============ 通用 API ============

/** 获取配置 */
export async function getConfig() {
  return invokeCommand<Config>('get_config')
}

/** 保存配置 */
export async function saveConfig(config: Config) {
  return invokeCommand<void>('save_config', { config })
}

/** 获取存储数据 */
export async function getStorage() {
  return invokeCommand<Storage>('get_storage')
}

/** 获取应用版本 */
export async function getAppVersion() {
  return invokeCommand<string>('get_app_version')
}

/** 打开文件夹 */
export async function openFolder(path: string) {
  return invokeCommand<void>('open_folder', { path })
}

/** 打开 URL */
export async function openUrl(url: string) {
  return invokeCommand<void>('open_url', { url })
}

/** 更新设置 */
export async function updateSetting(setting: CommonSetting) {
  return invokeCommand<void>('update_setting', { setting })
}

/** 更新上次打开的模拟器页面 */
export async function updateLastOpenEmuPage(page: string) {
  return invokeCommand<void>('update_last_open_emu_page', { page })
}

/** 更新深色模式状态 */
export async function updateDarkState(dark: boolean) {
  return invokeCommand<void>('update_dark_state', { dark })
}

// ============ 事件监听封装 ============

/**
 * 监听安装进度事件
 */
export async function onInstallProgress(
  callback: EventCallback<InstallProgress>
): Promise<UnlistenFn> {
  return listen<InstallProgress>(Events.INSTALL_PROGRESS, callback)
}

/**
 * 监听下载进度事件
 */
export async function onDownloadProgress(
  callback: EventCallback<DownloadProgress>
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(Events.DOWNLOAD_PROGRESS, callback)
}

/**
 * 监听消息通知事件
 */
export async function onNotifyMessage(
  callback: EventCallback<NotifyMessage>
): Promise<UnlistenFn> {
  return listen<NotifyMessage>(Events.NOTIFY_MESSAGE, callback)
}

/**
 * 监听日志消息事件
 */
export async function onLogMessage(callback: EventCallback<string>): Promise<UnlistenFn> {
  return listen<string>(Events.LOG_MESSAGE, callback)
}

// ============ 配置类型定义 ============

/** Yuzu 配置 */
export interface YuzuConfig {
  yuzu_path: string
  yuzu_version?: string
  yuzu_firmware?: string
  branch: string
}

/** Ryujinx 配置 */
export interface RyujinxConfig {
  path: string
  version?: string
  firmware?: string
  branch: string
}

/** 网络设置 */
export interface NetworkSetting {
  firmwareDownloadSource: string
  githubApiMode: string
  githubDownloadMirror: string
  ryujinxGitLabDownloadMirror: string
  useDoh: boolean
  proxy: string
}

/** 下载设置 */
export interface DownloadSetting {
  autoDeleteAfterInstall: boolean
  disableAria2Ipv6: boolean
  removeOldAria2LogFile: boolean
  verifyFirmwareMd5: boolean
}

/** UI 设置 */
export interface UiSetting {
  lastOpenEmuPage: string
  dark: boolean
  mode: string
  width: number
  height: number
}

/** 其他设置 */
export interface OtherSetting {
  rename_yuzu_to_cemu: boolean
}

/** 通用设置 */
export interface CommonSetting {
  ui: UiSetting
  network: NetworkSetting
  download: DownloadSetting
  other: OtherSetting
}

/** 应用配置 */
export interface Config {
  yuzu: YuzuConfig
  ryujinx: RyujinxConfig
  setting: CommonSetting
}

/** 存储数据 */
export interface Storage {
  yuzu_history: Record<string, YuzuConfig>
  ryujinx_history: Record<string, RyujinxConfig>
  yuzu_save_backup_path: string
}

// ============ 工具函数 ============

/**
 * 检查是否在 Tauri 环境中运行
 */
export function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window
}

/**
 * 格式化文件大小
 */
export function formatSize(bytes: number): string {
  const KB = 1024
  const MB = KB * 1024
  const GB = MB * 1024

  if (bytes >= GB) {
    return `${(bytes / GB).toFixed(2)} GB`
  } else if (bytes >= MB) {
    return `${(bytes / MB).toFixed(2)} MB`
  } else if (bytes >= KB) {
    return `${(bytes / KB).toFixed(2)} KB`
  } else {
    return `${bytes} B`
  }
}

/**
 * 格式化速度
 */
export function formatSpeed(bytesPerSec: number): string {
  return `${formatSize(bytesPerSec)}/s`
}

/**
 * 格式化持续时间
 */
export function formatDuration(seconds: number): string {
  if (seconds >= 3600) {
    const hours = Math.floor(seconds / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    return `${hours}h ${minutes}m`
  } else if (seconds >= 60) {
    const minutes = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${minutes}m ${secs}s`
  } else {
    return `${seconds}s`
  }
}
