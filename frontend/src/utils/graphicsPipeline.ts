export type GraphicsEngine = 'dlss' | 'xess'
export interface GraphicsSelection {
  srEnabled: boolean
  srEngine: GraphicsEngine
  fgEnabled: boolean
  fgEngine: GraphicsEngine
}
export function fgEngineNeedsRestart(selected: GraphicsEngine | undefined, running: GraphicsEngine | null, switchable = false) {
  return !!selected && !!running && selected !== running && !switchable
}
export function graphicsRoute(selection: GraphicsSelection, nr: boolean) {
  const nvidia = nr || (selection.srEnabled && selection.srEngine === 'dlss') || (selection.fgEnabled && selection.fgEngine === 'dlss')
  // NVIDIA sessions prepare the selected FG bridge at launch, even when paused.
  const xess = (selection.srEnabled && selection.srEngine === 'xess') || ((selection.fgEnabled || nvidia) && selection.fgEngine === 'xess')
  const conflict = selection.srEnabled && selection.srEngine === 'xess' && nvidia
  return { nvidia, xess, conflict, any: nr || selection.srEnabled || selection.fgEnabled }
}
export function readGraphicsSelection(value: unknown): GraphicsSelection | null {
  if (!value || typeof value !== 'object') return null
  const item = value as Record<string, unknown>
  if (typeof item.srEnabled !== 'boolean' || typeof item.fgEnabled !== 'boolean'
    || (item.srEngine !== 'dlss' && item.srEngine !== 'xess') || (item.fgEngine !== 'dlss' && item.fgEngine !== 'xess')) return null
  return { srEnabled: item.srEnabled, srEngine: item.srEngine, fgEnabled: item.fgEnabled, fgEngine: item.fgEngine }
}
