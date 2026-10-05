import { graphicsCommand, type GraphicsApi } from './graphics'
export interface NrLookStatus { temporal?: { active: boolean; requested: boolean; evaluated?: boolean; reason: string; error?: string | null; textureCount: number; allocationBytes: number; sourceFrameId?: number | null; intervalMs?: number | null; maximumHistoryWeight?: number | null; resetReason?: string | null } | null; active: boolean; requested: boolean; reason: string; error?: string | null; codec: string; spatial?: { active: boolean; requested: boolean; error?: string | null; reason: string; radiusPixels: number; textureCount: number; allocationBytes: number } | null }
export interface NrPipelineStatus { requestedPasses: number; actualPasses: number; secondActive: boolean; secondEvaluated: boolean; error?: string | null; reason: string; retry: number; passes?: { pass: number; evaluated: boolean; reset: boolean; sourceFrameId: number; intensity: number }[] }
import type { GraphicsAdvanced, FgOptions } from './graphicsAdvanced'

export interface FgCheck {
  id: string
  label: string
  status: 'passed' | 'blocked' | 'pending'
  detail: string
}
export interface FgPreflight {
  executable: string
  checkedAt: string
  targetVersion: string | null
  targetFamily: 'yuzu' | 'ryujinx' | 'unknown'
  buildTest: { version: string; detail: string } | null
  compatibility: 'verified' | 'adapted' | 'unverified' | 'incompatible'
  requiresTrialConfirmation: boolean
  targetSha256: string | null
  checks: FgCheck[]
  packageAvailable: boolean
  packageMessage: string
  plannedDestination: string
  installationState: 'unmanaged' | 'installed' | 'damaged'
  runtimeState: 'unknown'
}
export function detectStreamlineFg(executable: string, graphicsApi: GraphicsApi) {
  return graphicsCommand<FgPreflight>('detect_streamline_fg', { executable, graphicsApi })
}

export interface FgOperation { message: string; session: string | null }
export function operateStreamlineFg(action: 'install' | 'launch' | 'uninstall', executable: string, graphicsApi: GraphicsApi, allowUnverified: boolean, expectedSha256: string) {
  return graphicsCommand<FgOperation>(`${action}_streamline_fg`, { executable, graphicsApi, allowUnverified, expectedSha256 })
}

export interface FgSample { time: number; appFps: number | null; presentFps: number | null }
export interface FgLive {
  advancedSettingsSupported?: boolean
  nrLookSupported?: boolean
  nrSpatialLookSupported?: boolean; nrTwoPassSupported?: boolean
  nrTemporalLookSupported?: boolean
  fg?: { maximumGenerated?: number; options?: FgOptions; unsupportedMultiplier?: boolean } | null
  nrLiveSupported?: boolean
  sentNrRevision?: number
  nr?: { pipeline?: NrPipelineStatus | null; look?: NrLookStatus | null; active: boolean; requested?: boolean; appliedRevision?: number | null; intensity?: number; appliedIntensity?: number | null; evaluated?: boolean; outputReused?: boolean; sourceFrameId?: number | null; sourceFrameBasis?: string; controlsPending?: boolean; reason?: string; source?: string; depth?: string; motionValid?: boolean; input?: number[]; error?: string; reset?: boolean; resetReason?: string } | null
  srLiveSupported?: boolean
  srPresetSupported?: boolean
  sentSrRevision?: number
  sr?: { preset?: string; source?: string; fallbackReason?: string; scale?: number; mode?: number; appliedRevision?: number; active: boolean; original_input?: number[]; processing_output?: number[]; input?: number[]; output?: number[]; motion?: boolean; reason?: string } | null
  connected: boolean
  fresh?: boolean
  requested?: boolean
  active?: boolean
  reason?: string
  updatedAt?: number
  revision?: number
  appliedRevision?: number
  sentRevision?: number
  samples?: FgSample[]
}
export function liveStreamlineFg(executable: string, enabled?: boolean, srMode?: string, srScale?: number, srPreset?: string, nrEnabled?: boolean, nrIntensity?: number, advanced?: Partial<GraphicsAdvanced>) {
  return graphicsCommand<FgLive>('live_streamline_fg', { executable, enabled, srMode, srScale, srPreset, nrEnabled, nrIntensity, advanced })
}

export interface NativeNrComponent {
  supported: boolean
  packageReady: boolean
  installed: boolean
  runtimeSha256: string | null
  runtimeVersion: string
  architecture: string
  depth: string
  message: string
}
export const getNativeNrComponent = () => graphicsCommand<NativeNrComponent>('get_native_nr_component', {})
export const installNativeNrRuntime = () => graphicsCommand<NativeNrComponent>('install_native_nr_runtime', {})
export const uninstallNativeNrRuntime = () => graphicsCommand<NativeNrComponent>('uninstall_native_nr_runtime', {})
