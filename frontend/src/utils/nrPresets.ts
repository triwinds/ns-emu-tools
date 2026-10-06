import { graphicsCommand } from './graphics'
import { cloneNrOptions, graphicsAdvanced, nrLook, nrSecondPass, nrSpatialLook, nrTemporalLook, type NrOptions } from './graphicsAdvanced'
import type { FgLive } from './streamlineFg'

export type NrPresetEmulator = 'eden' | 'citron' | 'yuzu' | 'ryujinx' | 'other'
export interface NrPresetEnvironment {
  toolboxVersion: string; componentVersion: string; componentSha256: string | null
  modelVersion: string; modelSha256: string | null; targetSha256: string | null
}
export interface NrPreset {
  schemaVersion: 4; name: string; emulator: NrPresetEmulator; game: string; displayMode: string
  settings: { enabled: boolean; intensity: number; options: NrOptions }
  environment: NrPresetEnvironment
}
export function normalizeNrPreset(preset: NrPreset): NrPreset {
  return { ...preset, schemaVersion: 4, settings: { ...preset.settings, options: cloneNrOptions({ ...graphicsAdvanced().nr, ...preset.settings.options }) }, environment: { ...preset.environment } }
}
export const validateNrPreset = (json: string) => graphicsCommand<{ preset: NrPreset; migratedFrom: number | null }>('validate_nr_preset', { json })
export const nrPresetEnvironment = (executable: string) => graphicsCommand<NrPresetEnvironment>('nr_preset_environment', { executable })
export function presetNotices(preset: NrPreset, environment: NrPresetEnvironment | null, live: FgLive): string[] {
  const notes: string[] = []
  const recorded = preset.environment
  if (!recorded.componentSha256 || !recorded.modelSha256 || !recorded.targetSha256) notes.push('预设缺少部分版本记录，不能确认环境一致。')
  if (environment) {
    for (const [key, label] of [['componentSha256', '画面增强组件'], ['modelSha256', 'NR 模型'], ['targetSha256', '模拟器主程序']] as const) {
      if (recorded[key] && recorded[key] !== environment[key]) notes.push(`${label}与预设记录不同或未就绪；相同参数可能得到不同结果。`)
    }
  }
  if (!live.connected) return [...notes, '尚未连接游戏，应用后供下次专用启动使用。']
  if (!live.nrLiveSupported) notes.push('当前会话未准备 NR，需重新专用启动。')
  const settings = preset.settings
  const tuning = settings.options
  // Match the backend's full capability contract, including inactive overrides.
  if (settings.enabled && (!live.advancedSettingsSupported && (settings.intensity > 100 || JSON.stringify(tuning) !== JSON.stringify(graphicsAdvanced().nr)))) notes.push('当前组件不支持新增模型参数。')
  if (settings.enabled && JSON.stringify(tuning.secondPass) !== JSON.stringify(nrSecondPass()) && !live.nrTwoPassSupported) notes.push('当前组件不支持第二遍配置。')
  if (settings.enabled && JSON.stringify(tuning.look) !== JSON.stringify(nrLook()) && !live.nrLookSupported) notes.push('当前组件不支持此 Look 配置。')
  if (settings.enabled && JSON.stringify(tuning.look.spatial) !== JSON.stringify(nrSpatialLook()) && !live.nrSpatialLookSupported) notes.push('当前组件不支持空间 Look 配置。')
  if (settings.enabled && JSON.stringify(tuning.look.temporal) !== JSON.stringify(nrTemporalLook()) && !live.nrTemporalLookSupported) notes.push('当前组件不支持时间 Look 配置。')
  if (settings.enabled && tuning.look.scope !== 'final_pass' && !live.nrLookScopeSupported) notes.push('当前组件不支持整链 Look 作用范围。')
  if (settings.enabled && tuning.look.temporal.mode === 'optical_flow_plus' && !live.nrPersistenceSupported) notes.push('当前组件不支持光流累积＋。')
  if (settings.enabled && (tuning.look.algorithm !== 'log_delta' || tuning.look.diagnostic !== 'off' || Object.values(tuning.look.protection).some(value => value !== 0)) && !live.nrLookExperimentsSupported) notes.push('当前组件不支持 Look 颜色保护或诊断实验。')
  if (settings.enabled && (tuning.look.temporal.mode !== 'optical_flow' || tuning.look.temporal.sampling !== 'bilinear') && !live.nrTemporalModesSupported) notes.push('当前组件不支持所选时间模式或历史采样。')
  return notes
}
