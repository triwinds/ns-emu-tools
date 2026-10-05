import { describe, expect, test } from 'bun:test'
import { bypassNrAdditions, graphicsAdvanced, type NrOptions } from '../src/utils/graphicsAdvanced'
import { normalizeNrPreset, presetNotices, type NrPreset } from '../src/utils/nrPresets'

function fixture(): NrPreset {
  return { schemaVersion: 2, name: 'test', emulator: 'eden', game: '', displayMode: 'SDR', settings: { enabled: true, intensity: 150, options: graphicsAdvanced().nr }, environment: { toolboxVersion: '0.6.3', componentVersion: 'test', componentSha256: 'a'.repeat(64), modelVersion: '310.8.0', modelSha256: 'b'.repeat(64), targetSha256: 'c'.repeat(64) } }
}
describe('NR preset integration', () => {
  test('old neutral wire options restore omitted defaults without losing zero', () => {
    const preset = fixture()
    preset.settings.options = { style: 'b', globalTone: 0, localTone: null, localStructure: 150, skinStructure: 0, autoMask: false } as NrOptions
    const normalized = normalizeNrPreset(preset)
    expect(normalized.settings.options.globalTone).toBe(0)
    expect(normalized.settings.options.localStructure).toBe(150)
    expect(normalized.settings.options.look).toEqual(graphicsAdvanced().nr.look)
    expect(normalized.settings.options.secondPass).toEqual(graphicsAdvanced().nr.secondPass)
  })
  test('applying/editing a preview cannot mutate the stored preset', () => {
    const preset = fixture()
    const draft = normalizeNrPreset(preset)
    draft.settings.options.look.temporal.timeMs = 100
    draft.settings.options.look.spatial.radius = 15
    draft.settings.options.secondPass.intensity = 50
    draft.environment.componentVersion = 'changed'
    expect(preset).toEqual(fixture())
  })
  test('bypass retains model and all saved tuning, independently of defaults', () => {
    const preset = fixture()
    const options = preset.settings.options
    options.style = 'c'
    options.secondPass.enabled = true
    options.secondPass.intensity = 50
    options.look.amount = 130
    options.look.spatial.enabled = true
    options.look.temporal.enabled = true
    const bypass = bypassNrAdditions(options)
    expect(bypass).toEqual({ ...options, secondPass: { ...options.secondPass, enabled: false }, look: { ...options.look, enabled: false } })
    expect(options.secondPass.enabled).toBe(true)
    expect(options.look.enabled).toBe(true)
    expect(bypass).not.toEqual(graphicsAdvanced().nr)
  })
  test('missing versions and disconnected games are explicit', () => {
    const preset = fixture()
    preset.environment.modelSha256 = null
    const notes = presetNotices(preset, null, { connected: false })
    expect(notes.some(note => note.includes('缺少'))).toBe(true)
    expect(notes.some(note => note.includes('尚未连接'))).toBe(true)
  })
  test('component/model/target mismatches are distinguished', () => {
    const preset = fixture()
    const environment = { ...preset.environment, componentSha256: null, modelSha256: 'd'.repeat(64), targetSha256: 'e'.repeat(64) }
    const notes = presetNotices(preset, environment, { connected: false })
    expect(notes.filter(note => note.includes('不同或未就绪')).length).toBe(3)
  })
  test('nondefault inactive overrides require capability but NR off stays available', () => {
    const preset = fixture()
    preset.settings.options.secondPass.intensity = 50
    preset.settings.options.look.temporal.timeMs = 100
    const live = { connected: true, nrLiveSupported: true, advancedSettingsSupported: true, nrLookSupported: true }
    const notes = presetNotices(preset, preset.environment, live)
    expect(notes.some(note => note.includes('第二遍'))).toBe(true)
    expect(notes.some(note => note.includes('时间 Look'))).toBe(true)
    preset.settings.enabled = false
    expect(presetNotices(preset, preset.environment, live)).toEqual([])
  })
})
