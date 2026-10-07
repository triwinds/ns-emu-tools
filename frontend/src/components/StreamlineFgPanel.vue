<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import StreamlineFgLive from './StreamlineFgLive.vue'
import NativeNrControls from './NativeNrControls.vue'
import InputScaleControls from './InputScaleControls.vue'
import GraphicsAdvancedDialog from './GraphicsAdvancedDialog.vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { useProgressStore } from '@/stores/ProgressStore'
import { updateSetting } from '@/utils/tauri'
import { mdiCheckCircleOutline, mdiAlertCircleOutline, mdiClockOutline, mdiLayersOutline, mdiTuneVariant, mdiRefresh, mdiClipboardTextSearchOutline } from '@mdi/js'
import type { GraphicsApi } from '@/utils/graphics'
import type { GraphicsGpu } from '@/utils/graphicsGpu'
import { detectStreamlineFg, operateStreamlineFg, liveStreamlineFg, type FgLive, type FgCheck, type FgPreflight } from '@/utils/streamlineFg'
import { graphicsAdvanced, type FgOptions } from '@/utils/graphicsAdvanced'

const emit = defineEmits<{ busy: [value: boolean] }>()
const activeAction = ref<'install' | 'launch' | 'uninstall' | null>(null)
const props = defineProps<{ executable: string; api: GraphicsApi; disabled: boolean; gpu: GraphicsGpu }>()
const configStore = useConfigStore()
const progress = useProgressStore()
const savingNvof = ref(false)
const savingNr = ref(false)
const savingInput = ref(false)
const srAdvanced = ref(false)
const fgAdvanced = ref(false)
const currentLive = ref<FgLive>({ connected: false })
const nvofError = ref('')
const nvofFeedback = ref('自动保存；下次以画面增强启动时生效。')
const srFeedback = ref('自动保存；连接专用启动的游戏后实时应用。')
const srPending = ref(0)
const fgPending = ref(0)
const fgFeedback = ref('自动保存；连接专用启动的游戏后实时应用。')
let srDeadline = 0
let fgDeadline = 0
function receiveLive(value: FgLive) {
  currentLive.value = value
  if (fgPending.value && value.connected && value.fresh && (value.appliedRevision ?? 0) >= fgPending.value) {
    fgPending.value = 0
    fgFeedback.value = value.fg?.unsupportedMultiplier ? '已保存；当前 GPU / 运行库不支持所选倍数，FG 已暂停。请降低倍数。' : '当前游戏已应用 FG 设置。'
  } else if (fgPending.value && Date.now() > fgDeadline) {
    fgPending.value = 0
    fgFeedback.value = '已保存，尚未收到生效确认；回到游戏后查看实际状态。'
  }
  if (srPending.value && value.connected && value.fresh && (value.sr?.appliedRevision ?? 0) >= srPending.value) {
    srPending.value = 0
    srFeedback.value = value.sr?.active ? 'SR 已生效。' : `SR ${value.sr?.reason ?? '未运行'}`
  } else if (srPending.value && Date.now() > srDeadline) {
    srPending.value = 0
    srFeedback.value = '已保存，但尚未收到生效确认。请回到游戏恢复画面后查看运行状态。'
  }
}
const nvofEnabled = computed(() => configStore.config.setting.other?.streamline_nvof ?? true)
const fgEnabled = computed(() => configStore.config.setting.other?.streamline_fg ?? true)
const srEnabled = computed(() => configStore.config.setting.other?.streamline_sr ?? false)
const srMode = computed(() => configStore.config.setting.other?.streamline_sr_mode ?? 'balanced')
const srPreset = computed(() => configStore.config.setting.other?.streamline_sr_preset ?? 'j')
const advancedOptions = computed(() => graphicsAdvanced(configStore.config.setting.other?.streamline_advanced))
const srOptions = ref({ ...advancedOptions.value.sr })
const fgOptions = ref({ ...advancedOptions.value.fg })
watch(() => advancedOptions.value.sr, value => { srOptions.value = { ...value } })
watch(() => advancedOptions.value.fg, value => { fgOptions.value = { ...value } })
const fgMaximum = computed(() => {
  const runtime = currentLive.value.connected && currentLive.value.fresh ? currentLive.value.fg?.maximumGenerated : undefined
  return runtime === undefined ? props.gpu.fgMaxMultiplier : Math.min(props.gpu.fgMaxMultiplier, runtime > 0 ? runtime + 1 : 0)
})
const fgSupported = computed(() => fgMaximum.value >= 2)
const nvidiaNames = computed(() => [...new Set(props.gpu.adapters
  .filter(adapter => adapter.vendorId === 0x10de)
  .map(adapter => adapter.name.trim().replace(/\s+/g, ' '))
  .filter(Boolean))].join('、'))
const fgMultipliers = computed(() => [2, 3, 4, 5, 6].map(value => {
  const disabled = value > fgMaximum.value
  return { title: `${value}×${disabled ? (value > 2 && props.gpu.fgMaxMultiplier < 3 ? '（需要 RTX 50 系列）' : '（当前不可用）') : ''}`, value, props: { disabled, 'aria-disabled': disabled } }
}))
const fgModes = computed(() => [{ title: '固定倍数', value: 'fixed' }, { title: `动态目标帧率${fgMaximum.value < 3 ? '（需要 RTX 50 系列及运行库支持）' : ''}`, value: 'dynamic', props: { disabled: fgMaximum.value < 3 } }])
const hardwareIssue = computed(() =>
  (!props.gpu.nrSupported && configStore.config.setting.other?.streamline_nr)
  || (!props.gpu.srSupported && srEnabled.value)
  || (fgEnabled.value && (!fgSupported.value || advancedOptions.value.fg.multiplier > fgMaximum.value || (advancedOptions.value.fg.mode === 'dynamic' && fgMaximum.value < 3))),
)
async function useSupportedEffects() {
  await saveGraphics({
    streamline_nr: props.gpu.nrSupported && !!configStore.config.setting.other?.streamline_nr,
    streamline_sr: props.gpu.srSupported && srEnabled.value,
    streamline_fg: fgSupported.value && fgEnabled.value,
    streamline_advanced: { ...advancedOptions.value, fg: { ...advancedOptions.value.fg, multiplier: Math.max(2, Math.min(fgMaximum.value, advancedOptions.value.fg.multiplier)) as FgOptions['multiplier'], mode: fgMaximum.value < 3 ? 'fixed' : advancedOptions.value.fg.mode } },
  })
}
const reflexModes = [{ title: '低延迟（默认）', value: 'low_latency' }, { title: '低延迟 + Boost', value: 'boost' }]
const fgSummary = computed(() => advancedOptions.value.fg.mode === 'dynamic' ? `动态帧生成 · 目标 ${advancedOptions.value.fg.targetFps || '显示器刷新率'}${advancedOptions.value.fg.targetFps ? ' FPS' : ''} · 最高 ${advancedOptions.value.fg.multiplier}×` : `${advancedOptions.value.fg.multiplier}× 帧生成`)
const srDefault = computed(() => srMode.value === 'balanced' && savedScale.value === 172 && srPreset.value === 'j' && JSON.stringify(advancedOptions.value.sr) === JSON.stringify(graphicsAdvanced().sr))
const fgDefault = computed(() => nvofEnabled.value && JSON.stringify(advancedOptions.value.fg) === JSON.stringify(graphicsAdvanced().fg))
const srModes = [
  { title: 'DLAA · 等尺寸抗锯齿', value: 'dlaa', scale: 100 },
  { title: 'Quality · 画质', value: 'quality', scale: 150 },
  { title: 'Balanced · 均衡', value: 'balanced', scale: 172 },
  { title: 'Performance · 性能', value: 'performance', scale: 200 },
] as const
const srPresets = [
  { title: '自动（运行库默认）', value: 'default' },
  { title: 'J（默认）', value: 'j' },
  { title: 'K', value: 'k' },
  { title: 'M', value: 'm' },
  { title: 'L', value: 'l' },
]
async function setSrPreset(value: string) {
  if (srPresets.some(p => p.value === value)) {
    await saveGraphics({ streamline_sr_preset: value as NonNullable<GraphicsSettings['streamline_sr_preset']> })
  }
}
const savedScale = computed(() => configStore.config.setting.other?.streamline_sr_scale ?? ({ quality: 150, balanced: 172, performance: 200, dlaa: 100 }[srMode.value]))
const sliderScale = (value: number) => Math.min(2, Math.max(1, value / 100))
const srScale = ref(sliderScale(savedScale.value))
watch(savedScale, value => { srScale.value = sliderScale(value) })
const srModeTitle = computed(() => srModes.find(mode => mode.value === srMode.value)?.title ?? srMode.value)
const srPresetTitle = computed(() => srPreset.value === 'default' ? '自动模型' : `${srPreset.value.toUpperCase()} 模型`)
type GraphicsSettings = Partial<Pick<typeof configStore.config.setting.other, 'streamline_nr' | 'streamline_nvof' | 'streamline_fg' | 'streamline_sr' | 'streamline_sr_mode' | 'streamline_sr_scale' | 'streamline_sr_preset' | 'streamline_advanced'>>
async function saveSrOptions() {
  await saveGraphics({ streamline_advanced: { ...advancedOptions.value, sr: { ...srOptions.value } } })
}
async function saveFgOptions() {
  const next: FgOptions = { ...fgOptions.value, inputFps: Number(fgOptions.value.inputFps), targetFps: Number(fgOptions.value.targetFps) }
  if (!Number.isInteger(next.targetFps) || (next.targetFps !== 0 && (next.targetFps < 30 || next.targetFps > 360)) || !Number.isInteger(next.inputFps) || (next.inputFps !== 0 && (next.inputFps < 15 || next.inputFps > 240))) {
    nvofError.value = '目标帧率需为 0 或 30～360，输入帧率上限需为 0 或 15～240。'
    return
  }
  if (next.multiplier > fgMaximum.value || (next.mode === 'dynamic' && fgMaximum.value < 3)) {
    nvofError.value = '当前显卡／运行库不支持所选 FG 倍数或动态模式。2× 需要 RTX 40／50 系列；更高倍数与动态模式需要 RTX 50 系列。'
    return
  }
  await saveGraphics({ streamline_advanced: { ...advancedOptions.value, fg: next } })
}
function srModeForScale(scale: number) {
  if (scale === 100) return 'dlaa'
  return srModes.filter(mode => mode.value !== 'dlaa').reduce((closest, mode) =>
    Math.abs(mode.scale - scale) < Math.abs(closest.scale - scale) ? mode : closest,
  ).value
}
async function setFg(value: boolean | null) {
  if (value === null || (value && !fgSupported.value)) return
  await saveGraphics({ streamline_fg: value })
}
async function setNvof(value: boolean | null) {
  if (value === null) return
  await saveGraphics({ streamline_nvof: value })
  if (!nvofError.value) nvofFeedback.value = '已保存；下次以画面增强启动时生效。当前游戏会话保持原设置。'
}
async function setSr(value: boolean | null) {
  if (value && !props.gpu.srSupported) return
  const scale = Math.round(srScale.value * 100)
  if (value !== null) await saveGraphics({ streamline_sr: value, streamline_sr_scale: scale, streamline_sr_mode: srModeForScale(scale) })
}
async function setSrScale() {
  const scale = Math.round(srScale.value * 100)
  if (!Number.isFinite(scale) || scale < 100 || scale > 200 || scale === savedScale.value) return
  await saveGraphics({ streamline_sr_scale: scale, streamline_sr_mode: srModeForScale(scale) })
  srScale.value = sliderScale(savedScale.value)
}
async function saveGraphics(patch: GraphicsSettings) {
  if (props.disabled || working.value || !configStore.config.setting.other) return
  if (Object.entries(patch).every(([key, value]) => JSON.stringify(configStore.config.setting.other[key as keyof GraphicsSettings]) === JSON.stringify(value))) return
  const affectsSr = 'streamline_sr' in patch || 'streamline_sr_mode' in patch || 'streamline_sr_scale' in patch || 'streamline_sr_preset' in patch || (patch.streamline_advanced && JSON.stringify(patch.streamline_advanced.sr) !== JSON.stringify(advancedOptions.value.sr))
  const affectsFg = 'streamline_fg' in patch || (patch.streamline_advanced && JSON.stringify(patch.streamline_advanced.fg) !== JSON.stringify(advancedOptions.value.fg))
  const affectsNr = 'streamline_nr' in patch
  if ((affectsSr && srPending.value) || (affectsFg && fgPending.value)) return
  const executable = props.executable
  savingNvof.value = true
  nvofError.value = ''
  try {
    const setting = configStore.config.setting
    await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
    Object.assign(configStore.config.setting.other, patch)
    if (affectsSr || affectsFg || affectsNr) {
      if (executable !== props.executable) return
      if (affectsSr) srFeedback.value = '已保存，正在连接游戏…'
      if (affectsFg) fgFeedback.value = '已保存，正在连接游戏…'
      try {
        const status = executable ? await liveStreamlineFg(executable) : { connected: false }
        if (executable !== props.executable) return
        if (!status.connected) {
          if (affectsSr) srFeedback.value = '已保存；尚未连接游戏，下次专用启动时使用。'
          if (affectsFg) fgFeedback.value = '已保存；尚未连接游戏，下次专用启动时使用。'
        } else {
          if (affectsNr && status.nrLiveSupported) {
            await liveStreamlineFg(executable, undefined, undefined, undefined, undefined, patch.streamline_nr)
            if (executable !== props.executable) return
          }
          if (affectsSr) {
            if (!status.advancedSettingsSupported && JSON.stringify(advancedOptions.value.sr) !== JSON.stringify(graphicsAdvanced().sr)) {
              srFeedback.value = '已保存；当前组件不支持曝光配置，请更新画面增强组件后重新专用启动。'
            } else {
              const result = await liveStreamlineFg(executable, undefined, srEnabled.value ? srMode.value : 'off', savedScale.value, srPreset.value, undefined, undefined, status.advancedSettingsSupported ? { sr: advancedOptions.value.sr } : undefined)
              if (executable !== props.executable) return
              srPending.value = result.sentSrRevision ?? 0
              srDeadline = Date.now() + 10000
              srFeedback.value = srPending.value ? '已保存，正在应用到当前游戏…' : '已保存，但未收到应用请求确认。请查看游戏运行状态。'
            }
          }
          if (affectsFg) {
            if (!status.advancedSettingsSupported && JSON.stringify(advancedOptions.value.fg) !== JSON.stringify(graphicsAdvanced().fg)) {
              fgFeedback.value = '已保存；当前组件不支持新增参数，请更新画面增强组件后重新专用启动。'
            } else {
              const result = await liveStreamlineFg(executable, fgEnabled.value, undefined, undefined, undefined, undefined, undefined, status.advancedSettingsSupported ? { fg: advancedOptions.value.fg } : undefined)
              if (executable !== props.executable) return
              fgPending.value = result.sentRevision ?? 0
              fgDeadline = Date.now() + 10000
              fgFeedback.value = fgPending.value ? '已保存，正在应用到当前游戏…' : '已保存，但未收到应用请求确认。请查看游戏运行状态。'
            }
          }
        }
      } catch (e) {
        const feedback = `已保存，实时应用失败：${e instanceof Error ? e.message : String(e)}`
        if (affectsSr) srFeedback.value = feedback
        if (affectsFg) fgFeedback.value = feedback
      }
    }
  } catch (e) {
    nvofError.value = `图形设置保存失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    srScale.value = sliderScale(savedScale.value)
    srOptions.value = { ...advancedOptions.value.sr }
    fgOptions.value = { ...advancedOptions.value.fg }
    savingNvof.value = false
  }
}
const report = ref<FgPreflight | null>(null)
const loading = ref(false)
const error = ref('')
const message = ref('')
const session = ref('')
const details = ref(false)
const allowUnverified = ref(false)
let revision = 0
let inspectionPending = false
let componentRefreshTimer: ReturnType<typeof setTimeout> | undefined
function cancelComponentRefresh() {
  clearTimeout(componentRefreshTimer)
  componentRefreshTimer = undefined
}
function followComponentUpdate(token: number, attempt = 0) {
  cancelComponentRefresh()
  if (token !== revision || !report.value?.componentUpdatePending || attempt >= 6) return
  componentRefreshTimer = setTimeout(async () => {
    componentRefreshTimer = undefined
    if (token !== revision) return
    if (working.value || props.disabled) {
      followComponentUpdate(token, attempt + 1)
      return
    }
    try {
      const result = await detectStreamlineFg(props.executable, props.api)
      if (token !== revision) return
      if (result.targetSha256 !== report.value?.targetSha256) allowUnverified.value = false
      report.value = result
      followComponentUpdate(token, attempt + 1)
    } catch (e) {
      if (token === revision) error.value = e instanceof Error ? e.message : String(e)
    }
  }, 2000)
}
const installed = computed(() => report.value?.installationState === 'installed')
const working = computed(() => loading.value || savingNvof.value || savingNr.value || savingInput.value)
watch(working, value => emit('busy', value), { flush: 'sync' })
const installLabel = computed(() => activeAction.value === 'install' ? '正在下载并安装…' : '下载并安装全部组件')
const nextStep = computed(() => !props.executable ? '先在上方选择模拟器主程序。' : loading.value ? (activeAction.value ? '操作进行中，下载进度与取消操作见进度窗口。' : '正在检查模拟器与组件…') : !report.value ? '检查未完成，请重新检查后继续。' : blocked.value.length ? '请先解决下方列出的安装条件。' : report.value.requiresTrialConfirmation && !allowUnverified.value ? '当前程序尚无适配记录，请先阅读并确认尝试。' : installed.value ? '组件已安装。选择下方效果，再通过此面板启动模拟器。' : report.value.installationState === 'damaged' ? '组件不完整。先移除损坏安装，再下载并安装全部组件。' : !report.value.packageAvailable ? '组件包暂不可用，请查看检测详情并重新检查。' : '一次安装 NR、SR / DLAA 和帧生成所需组件，无需手动查找 DLL。')
const blocked = computed(() => report.value?.checks.filter(c => c.status === 'blocked') ?? [])
const targetAdapted = computed(() => report.value?.compatibility === 'verified' || report.value?.compatibility === 'adapted')
const targetFamilyLabel = computed(() => report.value?.targetFamily === 'yuzu' ? 'Eden / Citron / yuzu 系列' : 'Ryujinx 系列')
const statusLabel = computed(() => loading.value ? activeAction.value === 'install' ? '正在安装' : activeAction.value === 'launch' ? '正在启动' : activeAction.value === 'uninstall' ? '正在卸载' : '正在检查' : error.value ? '操作失败' : !report.value ? '等待检查' : blocked.value.length ? '不满足启用条件' : report.value.requiresTrialConfirmation && !allowUnverified.value ? '需要确认尝试' : report.value.installationState === 'installed' ? '已安装' : report.value.installationState === 'damaged' ? '安装需检查' : report.value.packageAvailable ? '可以安装' : '缺少组件包')
const canUse = computed(() => !!report.value && !blocked.value.length && (!report.value.requiresTrialConfirmation || allowUnverified.value))
const checkIcon = (status: FgCheck['status']) => ({ passed: mdiCheckCircleOutline, blocked: mdiAlertCircleOutline, pending: mdiClockOutline })[status]
const checkLabel = (status: FgCheck['status']) => ({ passed: '通过', blocked: '不满足', pending: '待确认' })[status]
const checkColor = (status: FgCheck['status']) => ({ passed: 'success', blocked: 'error', pending: 'warning' })[status]
const cleanPath = (value: string) => value.replace(/^\\\\\?\\UNC\\/i, '\\\\').replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const checkedTime = computed(() => report.value ? new Date(report.value.checkedAt).toLocaleString('zh-CN', { hour12: false }) : '')

// Clear old evidence immediately, including when an earlier request is still running.
watch(() => [props.executable, props.api], () => {
  revision++
  cancelComponentRefresh()
  inspectionPending = true
  srPending.value = 0
  fgPending.value = 0
  srAdvanced.value = false
  fgAdvanced.value = false
  currentLive.value = { connected: false }
  srFeedback.value = '自动保存；连接专用启动的游戏后实时应用。'
  fgFeedback.value = '自动保存；连接专用启动的游戏后实时应用。'
  nvofFeedback.value = '自动保存；下次以画面增强启动时生效。'
  allowUnverified.value = false
  report.value = null
  error.value = ''
  message.value = ''
  session.value = ''
  loading.value = false
  activeAction.value = null
  details.value = false
}, { immediate: true, flush: 'sync' })

async function inspect() {
  if (!props.executable || props.disabled || loading.value) return
  const token = ++revision
  cancelComponentRefresh()
  allowUnverified.value = false
  report.value = null
  error.value = ''
  loading.value = true
  try {
    const result = await detectStreamlineFg(props.executable, props.api)
    if (token === revision) {
      report.value = result
      followComponentUpdate(token)
    }
  } catch (e) {
    if (token === revision) error.value = e instanceof Error ? e.message : String(e)
  } finally {
    if (token === revision) loading.value = false
  }
}
// Wait for the parent page's detection/operation lock to clear before checking.
watch(() => [props.executable, props.api, props.disabled], () => {
  if (!inspectionPending || !props.executable || props.disabled) return
  inspectionPending = false
  void inspect()
}, { immediate: true, flush: 'post' })
onBeforeUnmount(() => { revision++; cancelComponentRefresh(); emit('busy', false) })
async function operate(action: 'install' | 'launch' | 'uninstall') {
  if (!report.value || working.value || props.disabled) return
  if (action === 'launch' && hardwareIssue.value) return
  const token = ++revision
  cancelComponentRefresh()
  const executable = props.executable
  const api = props.api
  activeAction.value = action
  loading.value = true
  error.value = ''
  message.value = ''
  try {
    const result = await operateStreamlineFg(action, executable, api, allowUnverified.value, report.value.targetSha256 ?? '')
    if (token !== revision) return
    message.value = result.message
    session.value = result.session ?? ''
    const refreshed = await detectStreamlineFg(executable, api)
    if (token === revision) {
      report.value = refreshed
      followComponentUpdate(token)
    }
  } catch (e) {
    if (token === revision) error.value = e instanceof Error ? e.message : String(e)
  } finally {
    progress.closeDialog()
    if (token === revision) { loading.value = false; activeAction.value = null }
  }
}
</script>

<template>
  <section
    class="fg-panel"
    aria-labelledby="fg-title"
    :aria-busy="loading"
  >
    <div class="fg-intro">
      <div class="fg-title-line">
        <v-icon
          :icon="mdiLayersOutline"
          color="secondary"
          size="24"
        />
        <h2 id="fg-title">
          DLSS 画面增强
        </h2>
        <v-chip
          size="x-small"
          variant="outlined"
        >
          实验版
        </v-chip>
      </div>
      <p class="fg-description">
        先安装，再选择效果，最后从这里启动模拟器。
      </p>
      <p>检测到：{{ nvidiaNames }}。多显卡电脑请在模拟器中选择支持对应功能的 NVIDIA 显卡，实际支持范围以运行状态为准。</p>
      <v-alert
        class="mt-3"
        type="warning"
        variant="tonal"
        density="compact"
      >
        当前画面增强实现无法获取游戏真实的深度和运动向量信息。HUD-less 模式下运行的 FG（帧生成）可能产生鬼影、闪烁、UI 变形等问题。有条件时，建议优先通过 MOD 提升画质、解锁帧数限制。
      </v-alert>
      <v-alert
        v-if="hardwareIssue"
        type="warning"
        variant="tonal"
      >
        已保存的效果或 FG 参数超出当前显卡／运行库支持范围。调整后才能以画面增强启动。
        <v-btn
          variant="text"
          :disabled="disabled || working"
          @click="useSupportedEffects"
        >
          改用支持的效果和倍率
        </v-btn>
      </v-alert>
      <ol
        class="fg-steps"
        aria-label="画面增强设置流程"
      >
        <li :class="{ complete: !!report && !blocked.length, current: !report || !!blocked.length }">
          <span class="fg-step-number">1</span><div><strong>核对模拟器</strong><span>{{ !executable ? '在上方选择主程序' : report && !blocked.length ? '已检查所选主程序' : '等待条件检查' }}</span></div>
        </li>
        <li :class="{ complete: installed, current: !!report && !blocked.length && !installed }">
          <span class="fg-step-number">2</span><div><strong>安装全部组件</strong><span>{{ installed ? '组件已就绪' : '自动下载与校验' }}</span></div>
        </li>
        <li :class="{ complete: currentLive.connected, current: installed && !currentLive.connected }">
          <span class="fg-step-number">3</span><div><strong>选择效果并启动</strong><span>{{ currentLive.connected ? '已连接游戏' : '通过专用入口生效' }}</span></div>
        </li>
      </ol>
    </div>

    <div class="fg-body">
      <div class="fg-setup">
        <div class="fg-status-line">
          <h3>{{ installed ? '组件已就绪' : '安装画面增强组件' }}</h3><span
            class="fg-status"
            role="status"
          >{{ statusLabel }}</span>
        </div>
        <p v-if="!executable">
          在上方选择模拟器主程序后，自动检查安装条件。
        </p>
        <p
          v-else
          class="fg-path"
        >
          {{ cleanPath(executable) }}
        </p>
        <p
          v-if="report && targetAdapted"
          class="fg-match"
        >
          <v-icon
            :icon="mdiCheckCircleOutline"
            size="16"
            color="success"
          /> 已适配 {{ targetFamilyLabel }}
        </p>
        <p v-if="report && targetAdapted">
          <template v-if="report.buildTest">
            本版本实测通过：{{ report.buildTest.version }}。
          </template>
          <template v-else>
            当前版本尚无实测记录；满足运行条件即可安装和启动。
          </template>
        </p>
        <div
          v-if="report?.requiresTrialConfirmation && !blocked.length"
          class="fg-trial"
        >
          <p>未识别为已适配的模拟器。选择尝试仍需通过运行时能力检查。</p>
          <v-checkbox
            v-model="allowUnverified"
            label="允许尝试此程序"
            density="compact"
            hide-details
            :disabled="disabled || loading"
          />
          <p v-if="allowUnverified">
            已选择尝试；本次选择仅适用于当前检测结果。
          </p>
        </div>
        <p
          v-if="error"
          class="fg-error"
          role="alert"
        >
          {{ error }}
        </p>
        <ul
          v-if="blocked.length"
          class="fg-blockers"
        >
          <li
            v-for="item in blocked"
            :key="item.id"
          >
            {{ item.detail }}
          </li>
        </ul>
        <p
          class="fg-next-step"
          role="status"
        >
          {{ nextStep }}
        </p>
        <div class="fg-install-actions">
          <v-btn
            v-if="!installed"
            color="primary"
            variant="flat"
            size="large"
            :loading="!!activeAction && activeAction !== 'launch'"
            :disabled="disabled || working || !report || (report.installationState !== 'damaged' && (!canUse || !report.packageAvailable))"
            @click="operate(report?.installationState === 'damaged' ? 'uninstall' : 'install')"
          >
            {{ report?.installationState === 'damaged' ? '移除损坏组件' : installLabel }}
          </v-btn>
          <v-btn
            class="fg-utility-button"
            variant="outlined"
            height="44"
            :prepend-icon="mdiRefresh"
            :disabled="!executable || disabled || working"
            @click="inspect"
          >
            重新检查
          </v-btn>
          <v-btn
            class="fg-utility-button"
            variant="outlined"
            height="44"
            :prepend-icon="mdiClipboardTextSearchOutline"
            :disabled="working || disabled"
            @click="details = true"
          >
            检测与安装详情
          </v-btn>
        </div>
        <p
          v-if="!installed"
          class="fg-install-hint"
        >
          安装后可分别启用画面重建、抗锯齿和帧生成。下载可在进度窗口取消。
        </p>
        <p
          v-if="message"
          role="status"
        >
          {{ message }}
        </p>
        <p
          v-if="session"
          class="fg-path"
        >
          本次运行记录：{{ cleanPath(session) }}
        </p>
        <div
          v-if="installed"
          class="fg-launch"
        >
          <div><h3>以画面增强启动</h3><p>启动模拟器后，在模拟器内打开游戏。普通启动不会加载这些效果。</p></div>
          <v-btn
            color="primary"
            variant="flat"
            size="large"
            :loading="activeAction === 'launch'"
            :disabled="!canUse || disabled || working || !!hardwareIssue"
            @click="operate('launch')"
          >
            以画面增强启动
          </v-btn>
        </div>
        <div
          v-show="installed"
          class="fg-effects"
        >
          <InputScaleControls
            :executable="executable"
            :disabled="disabled || loading || savingNvof || savingNr"
            :live="currentLive"
            @busy="savingInput = $event"
          />
          <div class="fg-effects-heading">
            <h3>选择要启用的效果</h3><p>三个效果可独立使用。设置自动保存，实际效果以游戏运行状态为准。</p>
          </div>
          <NativeNrControls
            :supported="gpu.nrSupported"
            :executable="executable"
            :refresh-key="report?.checkedAt"
            :disabled="disabled || loading || savingNvof || savingInput || !gpu.nrSupported"
            :live="currentLive"
            @busy="savingNr = $event"
          />
          <div class="fg-effect">
            <h3>改善锯齿 · SR / DLAA</h3>
            <p id="fg-sr-description">
              重建画面以改善锯齿，文字和界面也会参与处理，会增加 GPU 开销。
              需要 GeForce RTX 20／30／40／50 系列。{{ gpu.srSupported ? '' : '本机未检测到支持的显卡。' }}
            </p>
            <div class="fg-effect-controls">
              <v-switch
                :model-value="gpu.srSupported && srEnabled"
                label="启用 SR / DLAA"
                color="primary"
                density="compact"
                hide-details
                inset
                :loading="savingNvof"
                :disabled="disabled || working || !!srPending || !configStore.config.setting.other || !gpu.srSupported"
                aria-describedby="fg-sr-description"
                @update:model-value="setSr"
              />
              <v-btn
                variant="text"
                size="small"
                :prepend-icon="mdiTuneVariant"
                aria-label="SR 高级配置"
                aria-haspopup="dialog"
                :disabled="disabled || working || !gpu.srSupported"
                @click="srAdvanced = true"
              >
                高级配置
              </v-btn>
            </div>
            <p class="fg-effect-summary">
              {{ srModeTitle }} · {{ (savedScale / 100).toFixed(2) }}× · {{ srPresetTitle }}
            </p>
            <p role="status">
              {{ srFeedback }}
            </p>
          </div>
          <div class="fg-effect">
            <h3>提升流畅度 · FG 帧生成</h3>
            <p>2× 帧生成需要 GeForce RTX 40／50 系列；3×～6× 和动态多帧生成需要 RTX 50 系列及运行库支持。{{ fgSupported ? '' : '当前显卡／运行库不支持 FG，相关选项不可用。' }}</p>
            <div class="fg-effect-controls">
              <v-switch
                :model-value="fgSupported && fgEnabled"
                label="启用 FG 帧生成"
                color="primary"
                density="compact"
                hide-details
                inset
                :disabled="disabled || working || !!fgPending || !configStore.config.setting.other || !fgSupported"
                @update:model-value="setFg"
              />
              <v-btn
                variant="text"
                size="small"
                :prepend-icon="mdiTuneVariant"
                aria-label="FG 高级配置"
                aria-haspopup="dialog"
                :disabled="disabled || working || !fgSupported"
                @click="fgAdvanced = true"
              >
                高级配置
              </v-btn>
            </div>
            <p class="fg-effect-summary">
              {{ fgSummary }} · NVIDIA 光流辅助{{ nvofEnabled ? '开启' : '关闭（NR 会话除外）' }}
            </p>
            <p role="status">
              {{ fgFeedback }}
            </p>
          </div>
          <p
            v-if="nvofError && !srAdvanced && !fgAdvanced"
            class="fg-error"
            role="alert"
          >
            {{ nvofError }}
          </p>
        </div>
        <details
          v-if="installed"
          class="fg-maintenance"
        >
          <summary>组件维护</summary>
          <p>卸载后需重新安装才能通过画面增强入口启动。</p>
          <v-btn
            variant="text"
            :disabled="disabled || working"
            @click="operate('uninstall')"
          >
            卸载全部画面增强组件
          </v-btn>
        </details>
      </div>
    </div>
    <StreamlineFgLive
      v-show="currentLive.connected"
      :executable="executable"
      @status="receiveLive"
    />
    <p
      v-if="installed && !currentLive.connected"
      class="fg-awaiting"
      role="status"
    >
      游戏尚未连接。通过“以画面增强启动”打开模拟器并运行游戏后，这里会显示实时状态与帧率。
    </p>
    <div class="fg-footer">
      <span>支持 Ryujinx / Ryubing</span>
      <span>需要 Vulkan 图形接口</span>
    </div>

    <GraphicsAdvancedDialog
      v-model="srAdvanced"
      title="SR / DLAA 高级配置"
      title-id="sr-advanced-title"
      description="调整倍率、模型和曝光。设置自动保存；连接专用启动的游戏后实时应用，效果关闭时也可预先配置。"
      :busy="savingNvof"
    >
      <div class="fg-sr-options">
        <div class="fg-scale-section">
          <div class="fg-sr-scale-label">
            <label id="fg-sr-scale-label">自定义放大倍率</label><output>{{ srScale.toFixed(2) }}×</output>
          </div>
          <v-slider
            v-model="srScale"
            :min="1.0"
            :max="2.0"
            :step="0.01"
            color="primary"
            thumb-label
            hide-details
            aria-labelledby="fg-sr-scale-label"
            :disabled="disabled || working || !!srPending"
            @end="setSrScale"
            @keyup="setSrScale"
          />
          <div class="fg-sr-scale-label fg-muted">
            <span>1.00× · DLAA</span><span>2.00× · 四倍像素</span>
          </div>
          <p class="fg-option-note">
            1.00× 使用 DLAA。更高倍率先重建，再缩回窗口，不改变模拟器内部渲染分辨率。倍率越高，处理像素越多。
          </p>
          <p class="fg-option-note">
            所选倍率需运行库支持当前画面尺寸；未生效时可恢复默认配置后重试。
          </p>
        </div>
        <v-select
          :model-value="srPreset"
          :items="srPresets"
          label="模型预设"
          variant="outlined"
          density="comfortable"
          :disabled="disabled || working || !!srPending"
          hint="默认使用 J。自动使用运行库默认模型；J / K / M / L 可逐项对比画质和耗时。"
          persistent-hint
          @update:model-value="setSrPreset"
        />
        <div class="fg-advanced-section">
          <h3>曝光</h3>
          <v-switch
            v-model="srOptions.autoExposure"
            label="自动曝光"
            color="primary"
            density="comfortable"
            inset
            hide-details
            :disabled="disabled || working || !!srPending"
            @update:model-value="saveSrOptions"
          />
          <p class="fg-option-note">
            默认由运行库估算曝光。关闭后使用下方比例处理输入画面。
          </p>
          <div class="fg-sr-scale-label fg-control-label">
            <label id="sr-exposure-label">曝光比例</label><output>{{ (srOptions.exposure / 100).toFixed(2) }}×</output>
          </div>
          <v-slider
            v-model="srOptions.exposure"
            :min="25"
            :max="400"
            :step="5"
            color="primary"
            hide-details
            aria-labelledby="sr-exposure-label"
            :disabled="disabled || working || !!srPending || srOptions.autoExposure"
            @end="saveSrOptions"
            @keyup="saveSrOptions"
          />
          <div class="fg-sr-scale-label fg-muted">
            <span>0.25×</span><span>1.00× 标准</span><span>4.00×</span>
          </div>
        </div>
      </div>
      <p class="fg-option-note">
        优先处理模拟器缩放前的画面，无法识别时使用窗口画面。光流辅助可在 FG 高级配置中调整；不可用时逐帧重置重建历史。
      </p>
      <p
        class="fg-apply-feedback"
        role="status"
      >
        {{ srFeedback }} 调整倍率或模型可能短暂停顿。
      </p>
      <p
        v-if="nvofError"
        class="fg-error"
        role="alert"
      >
        {{ nvofError }}
      </p>
      <template #actions>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!srPending || srDefault"
          @click="saveGraphics({ streamline_sr_mode: 'balanced', streamline_sr_scale: 172, streamline_sr_preset: 'j', streamline_advanced: { ...advancedOptions, sr: graphicsAdvanced().sr } })"
        >
          恢复默认配置
        </v-btn>
      </template>
    </GraphicsAdvancedDialog>

    <GraphicsAdvancedDialog
      v-model="fgAdvanced"
      title="FG 高级配置"
      title-id="fg-advanced-title"
      description="调整生成倍数、目标帧率和延迟。设置自动保存；连接支持高级配置的游戏会话后实时应用。"
      :busy="savingNvof"
    >
      <div class="fg-sr-options">
        <v-select
          v-model="fgOptions.mode"
          :items="fgModes"
          label="帧生成方式"
          variant="outlined"
          :disabled="disabled || working || !!fgPending"
          @update:model-value="saveFgOptions"
        />
        <v-select
          v-model="fgOptions.multiplier"
          :items="fgMultipliers"
          :label="fgOptions.mode === 'dynamic' ? '最高生成倍数' : '生成倍数'"
          variant="outlined"
          :disabled="disabled || working || !!fgPending"
          hint="2× 需要 RTX 40／50 系列；3×～6× 需要 RTX 50 系列及运行库支持。倍数包含原始画面。"
          persistent-hint
          @update:model-value="saveFgOptions"
        />
        <p
          v-if="fgSupported"
          class="fg-capability"
          role="status"
        >
          当前显卡／运行库最高可选 {{ fgMaximum }}×
        </p>
        <v-text-field
          v-if="fgOptions.mode === 'dynamic'"
          v-model.number="fgOptions.targetFps"
          label="动态目标帧率"
          type="number"
          min="0"
          max="360"
          suffix="FPS"
          variant="outlined"
          :disabled="disabled || working || !!fgPending"
          hint="0 自动匹配显示器刷新率；自定义范围 30～360。运行库在最高倍数内调整生成帧数。"
          persistent-hint
          @change="saveFgOptions"
          @keyup.enter="saveFgOptions"
        />
        <div class="fg-advanced-section">
          <h3>延迟与帧率</h3>
          <v-select
            v-model="fgOptions.reflex"
            :items="reflexModes"
            label="Reflex 延迟模式"
            variant="outlined"
            :disabled="disabled || working || !!fgPending"
            hint="Boost 让 GPU 保持较高频率，可能增加功耗。"
            persistent-hint
            @update:model-value="saveFgOptions"
          />
          <v-text-field
            v-model.number="fgOptions.inputFps"
            label="原始帧率上限"
            type="number"
            min="0"
            max="240"
            suffix="FPS"
            variant="outlined"
            :disabled="disabled || working || !!fgPending"
            hint="通过 Reflex 限制送入处理链的原始帧率。0 不限制；自定义范围 15～240，不是生成后的帧率。"
            persistent-hint
            @change="saveFgOptions"
            @keyup.enter="saveFgOptions"
          />
        </div>
      </div>
      <p
        v-if="currentLive.connected && !currentLive.advancedSettingsSupported"
        class="fg-option-note"
      >
        当前组件不支持新增参数。设置可先保存，更新画面增强组件并重新专用启动后使用。
      </p>
      <v-switch
        :model-value="nvofEnabled"
        label="NVIDIA 光流辅助"
        color="primary"
        density="comfortable"
        hide-details
        inset
        :loading="savingNvof"
        :disabled="disabled || working || !configStore.config.setting.other"
        aria-describedby="fg-motion-description"
        @update:model-value="setNvof"
      />
      <p
        id="fg-motion-description"
        class="fg-option-note"
      >
        利用 NVIDIA 硬件估算画面运动，辅助 FG 和 SR。默认开启，会增加处理开销，可关闭对比效果。
      </p>
      <div class="fg-shared-setting">
        <strong>此设置也影响 SR</strong>
        <p>NR 会话始终使用硬件光流。安装 NR 组件后，专用启动会为实时切换准备 NR，因此即使此开关关闭，该会话仍会启用光流。</p>
      </div>
      <p
        class="fg-apply-feedback"
        role="status"
      >
        {{ fgFeedback }}
      </p>
      <p class="fg-option-note">
        光流辅助：{{ nvofFeedback }}
      </p>
      <p
        v-if="nvofError"
        class="fg-error"
        role="alert"
      >
        {{ nvofError }}
      </p>
      <template #actions>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!fgPending || fgDefault"
          @click="saveGraphics({ streamline_nvof: true, streamline_advanced: { ...advancedOptions, fg: graphicsAdvanced().fg } })"
        >
          恢复默认配置
        </v-btn>
      </template>
    </GraphicsAdvancedDialog>

    <v-dialog
      v-model="details"
      max-width="760"
      aria-labelledby="fg-plan-title"
      scrollable
    >
      <v-card class="fg-dialog">
        <v-card-title id="fg-plan-title">
          检测与安装详情
        </v-card-title>
        <v-card-text class="fg-dialog-body">
          <p class="fg-dialog-lead">
            自动下载并部署 NR、SR / DLAA、FG 与配套图层，通过工具箱的专用入口启动游戏。
          </p>
          <div class="fg-plan-target">
            <span>目标模拟器</span><strong>{{ executable ? cleanPath(executable) : '尚未选择，请先返回选择模拟器' }}</strong>
          </div>
          <h3>安装条件</h3>
          <p
            v-if="!report"
            class="fg-muted"
          >
            {{ loading ? '正在核对所选主程序…' : error ? '自动检查失败，请关闭此窗口后点击“重新检查”。' : executable ? '等待自动检查所选主程序…' : '选择模拟器主程序后自动检查。' }}
          </p>
          <ul
            v-else
            class="fg-checks"
          >
            <li
              v-for="item in report.checks"
              :key="item.id"
            >
              <v-icon
                :icon="checkIcon(item.status)"
                :color="checkColor(item.status)"
                size="20"
              />
              <div><strong>{{ item.label }}<span class="fg-check-label">{{ checkLabel(item.status) }}</span></strong><p>{{ item.detail }}</p></div>
            </li>
          </ul>
          <h3>部署内容</h3>
          <dl class="fg-deployment">
            <div><dt>组件</dt><dd>NR 调用桥、画面增强图层、Streamline、DLSS SR / FG 与 Reflex</dd></div>
            <div>
              <dt>安装位置</dt><dd class="fg-path">
                {{ report ? cleanPath(report.plannedDestination) : '工具箱配置目录 / graphics / streamline-fg' }}<small>组件部署到独立版本目录，运行记录单独保存。</small>
              </dd>
            </div>
            <div><dt>生效方式</dt><dd>点击“以画面增强启动”，按三个独立开关运行。普通启动不加载此图层。</dd></div>
          </dl>
          <p class="fg-muted">
            不替换模拟器主程序，不修改存档或全局 Vulkan 注册。
          </p>
          <div
            class="fg-release-note"
            :class="{ 'fg-release-note-installed': installed }"
          >
            <v-icon
              :icon="installed ? mdiCheckCircleOutline : mdiClockOutline"
              size="20"
            /><div><strong>{{ installed ? '已安装画面增强组件' : report?.packageAvailable ? '自动下载画面增强组件' : '组件包未就绪' }}</strong><p>{{ report?.packageMessage ?? '选择主程序后自动检查组件包。' }}</p></div>
          </div>
          <details
            v-if="report"
            class="fg-evidence"
          >
            <summary>查看检测记录</summary><p>检查时间：{{ checkedTime }}</p><p class="fg-path">
              主程序 SHA-256：{{ report.targetSha256 }}
            </p><p>本次只检查安装条件，未检测游戏内的实时帧生成状态。</p>
          </details>
        </v-card-text>
        <v-card-actions class="fg-dialog-actions">
          <v-btn
            variant="text"
            @click="details = false"
          >
            关闭
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </section>
</template>

<style scoped>
.fg-panel { margin-top: 28px; overflow: hidden; border: 1px solid rgba(var(--v-theme-on-background), .16); border-radius: 16px; background: rgb(var(--v-theme-surface)); }
.fg-intro { padding: 24px 26px 0; }
.fg-title-line, .fg-tags, .fg-status-line, .fg-actions, .fg-footer { display: flex; align-items: center; }
.fg-title-line { gap: 10px; flex-wrap: wrap; }
.fg-title-line h2 { font-size: 23px; font-weight: 650; letter-spacing: -.4px; }
.fg-description { margin-top: 10px; line-height: 1.7; font-size: 15px; }
.fg-tags { gap: 18px; margin-top: 12px; font-size: 12px; color: rgba(var(--v-theme-on-surface), .68); }
.fg-body { padding: 24px 26px; }
.fg-setup { min-width: 0; }
.fg-status-line { gap: 12px; justify-content: space-between; flex-wrap: wrap; }
.fg-status-line h3 { font-size: 15px; font-weight: 600; }
.fg-status { font-size: 12px; color: rgba(var(--v-theme-on-surface), .7); }
.fg-setup > p, .fg-blockers { font-size: 13px; line-height: 1.75; margin-top: 10px; }
.fg-path { overflow-wrap: anywhere; word-break: break-word; }
.fg-package-note { color: rgba(var(--v-theme-on-surface), .7); }
.fg-blockers { padding-left: 18px; }
.fg-trial { margin-top: 12px; font-size: 13px; line-height: 1.7; padding: 12px; background: rgba(var(--v-theme-warning), .09); border-radius: 8px; }
.fg-error { color: rgb(var(--v-theme-error)); overflow-wrap: anywhere; }
.fg-effect { padding: 20px 0; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-effect h3 { font-size: 15px; font-weight: 600; margin-bottom: 8px; }
.fg-effect p { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.fg-effect-controls { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.fg-effect-controls .v-switch { flex: 1 1 180px; }
.fg-effect-controls .v-btn { flex-shrink: 0; }
.fg-effect-summary { font-variant-numeric: tabular-nums; }
.fg-sr-scale-label { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; font-variant-numeric: tabular-nums; }
.fg-sr-scale-label output { font-weight: 600; }
.fg-sr-options { display: grid; gap: 24px; margin-bottom: 24px; }
.fg-advanced-section { border-top: 1px solid rgba(var(--v-theme-on-surface), .12); padding-top: 20px; }
.fg-advanced-section h3 { font-size: 15px; font-weight: 600; margin-bottom: 18px; }
.fg-advanced-section .v-text-field { margin-top: 24px; }
.fg-control-label { margin-top: 20px; margin-bottom: 14px; }
.fg-capability { font-size: 13px; color: rgb(var(--v-theme-secondary)); margin-top: -10px; }
.fg-scale-section { padding: 18px 0; border-block: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-scale-section .v-slider { margin-top: 14px; }
.fg-option-note { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 14px; }
.fg-apply-feedback { margin-top: 20px; padding-top: 16px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); font-size: 13px; line-height: 1.7; }
.fg-shared-setting { margin-top: 22px; padding-left: 14px; border-left: 3px solid rgb(var(--v-theme-secondary)); font-size: 13px; line-height: 1.7; }
.fg-shared-setting strong { font-weight: 600; }
.fg-shared-setting p { margin-top: 6px; color: rgba(var(--v-theme-on-surface), .72); }
.fg-actions { gap: 6px; margin-top: 18px; flex-wrap: wrap; }
.fg-footer { border-top: 1px solid rgba(var(--v-theme-on-surface), .1); gap: 24px; padding: 12px 26px; font-size: 12px; color: rgba(var(--v-theme-on-surface), .65); flex-wrap: wrap; }
.fg-dialog { font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; }
.fg-dialog-body { font-size: 14px; line-height: 1.7; }
.fg-dialog-lead { margin-bottom: 20px; }
.fg-plan-target { padding: 14px 16px; background: rgba(var(--v-theme-on-surface), .05); border-radius: 8px; }
.fg-plan-target span, .fg-plan-target strong { display: block; overflow-wrap: anywhere; }
.fg-plan-target span { color: rgba(var(--v-theme-on-surface), .65); font-size: 12px; margin-bottom: 4px; }
.fg-dialog-body h3 { font-size: 16px; margin: 24px 0 12px; }
.fg-checks { list-style: none; padding: 0; }
.fg-checks li { display: flex; align-items: flex-start; gap: 12px; padding: 10px 0; }
.fg-checks .v-icon { margin-top: 3px; flex-shrink: 0; }
.fg-checks strong { font-size: 14px; font-weight: 600; }
.fg-checks p, .fg-muted { color: rgba(var(--v-theme-on-surface), .7); font-size: 13px; }
.fg-check-label { font-weight: 400; margin-left: 12px; font-size: 12px; }
.fg-deployment { margin-bottom: 16px; }
.fg-deployment > div { display: grid; grid-template-columns: 80px minmax(0, 1fr); gap: 12px; padding: 9px 0; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .1); }
.fg-deployment dt { color: rgba(var(--v-theme-on-surface), .65); }
.fg-deployment small { display: block; margin-top: 4px; color: rgba(var(--v-theme-on-surface), .65); }
.fg-release-note { display: flex; gap: 12px; align-items: flex-start; background: rgba(var(--v-theme-warning), .09); border-radius: 8px; padding: 16px; margin-top: 22px; }
.fg-release-note-installed { background: rgba(var(--v-theme-success), .09); }
.fg-release-note .v-icon { margin-top: 3px; flex-shrink: 0; }
.fg-release-note p { margin-top: 4px; font-size: 13px; }
.fg-evidence { margin-top: 18px; font-size: 12px; }
.fg-evidence summary { cursor: pointer; padding: 8px 0; }
.fg-evidence summary:focus-visible { outline: 2px solid rgb(var(--v-theme-primary)); outline-offset: 3px; }
.fg-dialog-actions { padding: 16px 24px; border-top: 1px solid rgba(var(--v-theme-on-surface), .1); }
.fg-steps { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); list-style: none; padding: 0; margin-top: 24px; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-steps li { display: flex; gap: 12px; align-items: center; padding: 16px 0; border-bottom: 3px solid transparent; color: rgba(var(--v-theme-on-surface), .65); }
.fg-steps li.current { border-bottom-color: rgb(var(--v-theme-secondary)); color: rgb(var(--v-theme-on-surface)); }
.fg-step-number { display: grid; place-items: center; width: 30px; height: 30px; flex-shrink: 0; border: 1px solid rgba(var(--v-theme-on-surface), .3); border-radius: 50%; font-weight: 600; }
.fg-steps .current .fg-step-number { background: rgb(var(--v-theme-secondary)); border-color: transparent; color: rgb(var(--v-theme-on-secondary)); }
.fg-steps .complete .fg-step-number { border-color: rgb(var(--v-theme-success)); color: rgb(var(--v-theme-success)); }
.fg-steps strong, .fg-steps li div > span { display: block; }
.fg-steps strong { font-size: 14px; font-weight: 600; }
.fg-steps li div > span { font-size: 12px; margin-top: 3px; }
.fg-install-actions { display: flex; gap: 12px; flex-wrap: wrap; margin-top: 18px; }
.fg-install-actions .fg-utility-button { padding: 0 16px; border: 1px solid rgba(var(--v-theme-on-surface), .28); border-radius: 8px; background: rgba(var(--v-theme-on-surface), .07); color: rgb(var(--v-theme-on-surface)); font-size: 14px; font-weight: 600; letter-spacing: 0; text-transform: none; transition: background-color .16s ease, border-color .16s ease; }
.fg-utility-button :deep(.v-icon) { color: rgb(var(--v-theme-secondary)); font-size: 20px; }
.fg-install-actions .fg-utility-button:hover:not(:disabled) { background: rgba(var(--v-theme-secondary), .12); border-color: rgba(var(--v-theme-secondary), .65); }
.fg-install-actions .fg-utility-button:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 3px; }
.fg-next-step { max-width: 68ch; }
.fg-install-hint { color: rgba(var(--v-theme-on-surface), .7); }
.fg-awaiting { padding: 16px 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); }
.fg-effects { margin-top: 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .16); padding-top: 22px; }
.fg-effects-heading h3, .fg-launch h3 { font-size: 17px; font-weight: 600; }
.fg-effects-heading p, .fg-launch p, .fg-maintenance p { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.fg-launch { display: flex; justify-content: space-between; align-items: center; gap: 24px; padding: 20px; margin-top: 24px; border-radius: 10px; background: rgba(var(--v-theme-primary), .1); border: 1px solid rgba(var(--v-theme-primary), .3); }
.fg-launch .v-btn { flex-shrink: 0; }
.fg-maintenance { margin-top: 18px; }
.fg-maintenance summary { cursor: pointer; padding: 10px 0; font-size: 13px; }
summary:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 4px; border-radius: 3px; }
@media (max-width: 650px) { .fg-steps { grid-template-columns: 1fr; }.fg-steps li { padding: 10px 0; gap: 10px; }.fg-launch { flex-direction: column; align-items: stretch; gap: 16px; } }
@media (max-width: 800px) { .fg-footer { gap: 8px 18px; } }
@media (max-width: 450px) { .fg-intro { padding: 20px 18px 0; }.fg-body { padding: 20px 18px; }.fg-footer { padding: 12px 18px; }.fg-title-line h2 { font-size: 21px; }.fg-actions, .fg-install-actions { align-items: stretch; flex-direction: column; }.fg-deployment > div { grid-template-columns: 1fr; gap: 3px; } }
@media (prefers-reduced-motion: reduce) { .fg-install-actions .fg-utility-button { transition: none; } }
</style>
