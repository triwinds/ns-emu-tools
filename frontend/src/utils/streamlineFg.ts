import { graphicsCommand, type GraphicsApi } from './graphics'

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
  compatibility: 'verified' | 'unverified' | 'incompatible'
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
export function liveStreamlineFg(executable: string, enabled?: boolean, srMode?: string, srScale?: number, srPreset?: string) {
  return graphicsCommand<FgLive>('live_streamline_fg', { executable, enabled, srMode, srScale, srPreset })
}
