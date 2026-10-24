import { describe, expect, test } from 'bun:test'
import { graphicsRoute, readGraphicsSelection, fgEngineNeedsRestart, type GraphicsSelection } from '../src/utils/graphicsPipeline'

const selection = (srEngine: 'dlss' | 'xess', fgEngine: 'dlss' | 'xess', srEnabled = true, fgEnabled = true): GraphicsSelection => ({ srEngine, fgEngine, srEnabled, fgEnabled })
describe('unified graphics launch routing', () => {
  test('only an explicitly switchable runtime can change FG without restarting', () => {
    expect(fgEngineNeedsRestart('dlss', 'xess', true)).toBe(false)
    expect(fgEngineNeedsRestart('xess', 'dlss', true)).toBe(false)
    expect(fgEngineNeedsRestart('dlss', 'xess', false)).toBe(true)
    expect(fgEngineNeedsRestart('xess', 'dlss', false)).toBe(true)
  })
  test('persisting a different engine does not change the running engine or allow its controls', () => {
    expect(fgEngineNeedsRestart('dlss', 'xess')).toBe(true)
    expect(fgEngineNeedsRestart('xess', 'dlss')).toBe(true)
    expect(fgEngineNeedsRestart('dlss', 'dlss')).toBe(false)
    expect(fgEngineNeedsRestart('xess', 'xess')).toBe(false)
    expect(fgEngineNeedsRestart('dlss', null)).toBe(false)
    expect(fgEngineNeedsRestart(undefined, 'xess')).toBe(false)
  })
  test('routes existing combinations and explicitly rejects the missing reverse mix', () => {
    expect(graphicsRoute(selection('dlss', 'dlss'), false)).toEqual({ nvidia: true, xess: false, conflict: false, any: true })
    expect(graphicsRoute(selection('dlss', 'xess'), false)).toEqual({ nvidia: true, xess: true, conflict: false, any: true })
    expect(graphicsRoute(selection('xess', 'xess'), false)).toEqual({ nvidia: false, xess: true, conflict: false, any: true })
    expect(graphicsRoute(selection('xess', 'dlss'), false).conflict).toBe(true)
  })
  test('standalone XeSS FG needs no NVIDIA route, while NR requires the NVIDIA component', () => {
    expect(graphicsRoute(selection('dlss', 'xess', false), false)).toEqual({ nvidia: false, xess: true, conflict: false, any: true })
    expect(graphicsRoute(selection('dlss', 'xess', false), true)).toEqual({ nvidia: true, xess: true, conflict: false, any: true })
    expect(graphicsRoute(selection('xess', 'xess'), true).conflict).toBe(true)
  })
  test('prepares the XeSS bridge for later resume in an NVIDIA SR session', () => {
    expect(graphicsRoute(selection('dlss', 'xess', true, false), false)).toEqual({ nvidia: true, xess: true, conflict: false, any: true })
    expect(graphicsRoute(selection('dlss', 'xess', false, false), false)).toEqual({ nvidia: false, xess: false, conflict: false, any: false })
  })
  test('rejects corrupt persisted choices without coercing booleans or unknown engines', () => {
    expect(readGraphicsSelection(selection('dlss', 'xess'))).toEqual(selection('dlss', 'xess'))
    for (const value of [null, {}, { ...selection('dlss', 'xess'), srEnabled: 'false' }, { ...selection('dlss', 'xess'), fgEngine: 'fsr' }, { ...selection('dlss', 'xess'), srEngine: ['dlss'] }]) expect(readGraphicsSelection(value)).toBeNull()
  })
})
