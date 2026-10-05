export interface NrLook {
  temporal: NrTemporalLook
  spatial: NrSpatialLook
  enabled: boolean
  schemaVersion: 1
  amount: number
  brighten: number
  darken: number
  brightenCap: number
  darkenCap: number
  color: number
  hue: number
  shadows: number
  midtones: number
  highlights: number
}
export interface NrTemporalLook { enabled: boolean; timeMs: number; strength: number; rejection: number }
export function nrTemporalLook(): NrTemporalLook { return { enabled: false, timeMs: 80, strength: 75, rejection: 50 } }
export interface NrSpatialLook {
  enabled: boolean
  lighting: number
  detail: number
  radius: number
  halo: number
}
export function nrSpatialLook(): NrSpatialLook {
  return { enabled: false, lighting: 100, detail: 100, radius: 8, halo: 0 }
}
export function nrLook(): NrLook {
  return { temporal: nrTemporalLook(), spatial: nrSpatialLook(), enabled: true, schemaVersion: 1, amount: 100, brighten: 100, darken: 100, brightenCap: 0, darkenCap: 0, color: 100, hue: 100, shadows: 100, midtones: 100, highlights: 100 }
}
function cloneNrLook(value?: NrLook): NrLook {
  return { ...nrLook(), ...value, temporal: { ...nrTemporalLook(), ...value?.temporal }, spatial: { ...nrSpatialLook(), ...value?.spatial } }
}
export function cloneNrOptions(value: NrOptions): NrOptions {
  return { ...value, look: cloneNrLook(value.look), secondPass: { ...nrSecondPass(), ...value.secondPass } }
}
/** Keep tuning values while selecting the original single-model output. */
export function bypassNrAdditions(value: NrOptions): NrOptions {
  const result = cloneNrOptions(value)
  result.secondPass.enabled = false
  result.look.enabled = false
  return result
}
export interface NrSecondPass {
  enabled: boolean; inherit: boolean; retry: number; intensity: number
  style: 'a' | 'b' | 'c'
  globalTone: number | null; localTone: number | null; localStructure: number | null
  skinStructure: number; autoMask: boolean
}
export function nrSecondPass(): NrSecondPass {
  return { enabled: false, inherit: true, retry: 0, intensity: 100, style: 'a', globalTone: null, localTone: null, localStructure: null, skinStructure: 0, autoMask: false }
}
export interface NrOptions {
  secondPass: NrSecondPass
  look: NrLook
  style: 'a' | 'b' | 'c'
  globalTone: number | null
  localTone: number | null
  localStructure: number | null
  skinStructure: number
  autoMask: boolean
}
export interface SrOptions { autoExposure: boolean; exposure: number }
export interface FgOptions {
  mode: 'fixed' | 'dynamic'
  multiplier: number
  targetFps: number
  reflex: 'low_latency' | 'boost'
  inputFps: number
}
export interface GraphicsAdvanced { nr: NrOptions; sr: SrOptions; fg: FgOptions }
export function graphicsAdvanced(value?: GraphicsAdvanced): GraphicsAdvanced {
  return {
    nr: { style: 'a', globalTone: null, localTone: null, localStructure: null, skinStructure: 0, autoMask: false, ...value?.nr, look: cloneNrLook(value?.nr?.look), secondPass: { ...nrSecondPass(), ...value?.nr?.secondPass } },
    sr: { autoExposure: true, exposure: 100, ...value?.sr },
    fg: { mode: 'fixed', multiplier: 2, targetFps: 0, reflex: 'low_latency', inputFps: 0, ...value?.fg },
  }
}
