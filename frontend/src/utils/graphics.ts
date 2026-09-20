import { invoke } from '@tauri-apps/api/core'
import type { ApiResponse } from './tauri'

export type GraphicsApi = 'vulkan' | 'openGl'
export type ComponentState = 'notInstalled' | 'installed' | 'incomplete' | 'modified' | 'external' | 'unknown' | 'unsupported' | 'error'
export interface GraphicsTarget { family: string; executable: string }
export interface Detection {
  executable: string
  architecture: string
  supportedTarget: boolean
  installationAvailable: boolean
  reshadeState: ComponentState
  feederState: ComponentState
  diagnostics: string[]
}
export interface InstallPreview {
  planId: string | null
  blockers: string[]
  diagnostics: string[]
  version?: string
  bundle?: string
  destination?: string
  files?: string[]
  sourceUrl?: string
  sources?: { name: string; url: string; sha256: string }[]
  requiresExternalOverwriteConfirmation?: boolean
  requiresVulkanScopeConfirmation?: boolean
  affectedTargets?: string[]
}
export interface Operation { message: string; preservedFiles?: string[] }
export async function graphicsCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const result = await invoke<ApiResponse<T>>(command, args)
  if (result.code !== 0) throw new Error(result.msg || '操作失败，请重试')
  if (result.data === undefined) throw new Error('后端未返回操作结果')
  return result.data
}
