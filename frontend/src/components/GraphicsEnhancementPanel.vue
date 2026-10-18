<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { mdiPlay, mdiRefresh, mdiTuneVariant, mdiCheckCircleOutline } from '@mdi/js'
import StreamlineFgPanel from './StreamlineFgPanel.vue'
import XessPanel from './XessPanel.vue'
import NativeNrControls from './NativeNrControls.vue'
import InputSizingControls from './InputSizingControls.vue'
import GraphicsAdvancedDialog from './GraphicsAdvancedDialog.vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { updateSetting } from '@/utils/tauri'
import { graphicsCommand, type GraphicsApi } from '@/utils/graphics'
import { graphicsAdvanced } from '@/utils/graphicsAdvanced'
import type { GraphicsGpu } from '@/utils/graphicsGpu'
import { liveStreamlineFg } from '@/utils/streamlineFg'
import { graphicsRoute, readGraphicsSelection, fgEngineNeedsRestart, type GraphicsSelection, type GraphicsEngine } from '@/utils/graphicsPipeline'

const props = defineProps<{ executable: string; api: GraphicsApi; disabled: boolean; gpu: GraphicsGpu | null }>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const config = useConfigStore()
const nvidia = ref<InstanceType<typeof StreamlineFgPanel> | null>(null)
const intel = ref<InstanceType<typeof XessPanel> | null>(null)
const nv = computed(() => nvidia.value?.state)
const xe = computed(() => intel.value?.state)
const safeGpu = computed<GraphicsGpu>(() => props.gpu ?? { hasNvidia: false, srSupported: false, nrSupported: false, fgMaxMultiplier: 0, adapters: [] })
const storageKey = 'ns-emu-tools:graphics-effects:v1'
let saved: GraphicsSelection | null = null
try { saved = readGraphicsSelection(JSON.parse(localStorage.getItem(storageKey) ?? 'null')) } catch { /* Use installed settings on first visit. */ }
const selection = ref<GraphicsSelection>(saved ?? { srEnabled: false, srEngine: 'dlss', fgEnabled: false, fgEngine: 'dlss' })
const initialized = ref(!!saved)
watch([xe, () => props.gpu, () => config.config.setting.other], () => {
  if (initialized.value || !xe.value || (!xe.value.report && !xe.value.error) || !config.config.setting.other) return
  const other = config.config.setting.other
  selection.value = props.gpu?.hasNvidia
    ? { srEnabled: other.streamline_sr ?? false, srEngine: 'dlss', fgEnabled: other.streamline_fg ?? true, fgEngine: graphicsAdvanced(other.streamline_advanced).fg.backend ?? 'dlss' }
    : { srEnabled: xe.value.settings.nativeAa, srEngine: 'xess', fgEnabled: xe.value.settings.fg, fgEngine: 'xess' }
  initialized.value = true
}, { immediate: true })
const nrEnabled = computed(() => !!config.config.setting.other?.streamline_nr)
const route = computed(() => {
  const value = graphicsRoute(selection.value, nrEnabled.value)
  // Prefer the dual-engine NVIDIA session when both components are installed.
  const dual = props.gpu?.hasNvidia && (props.gpu.fgMaxMultiplier ?? 0) >= 2
    && !(selection.value.srEnabled && selection.value.srEngine === 'xess')
    && selection.value.fgEnabled && nv.value?.installed && xe.value?.report?.installed
  return dual ? { ...value, nvidia: true } : value
})
const busy = ref(false)
const nvBusy = ref(false)
const xeBusy = ref(false)
const nrBusy = ref(false)
const locked = computed(() => props.disabled || busy.value || nvBusy.value || xeBusy.value || nrBusy.value || !initialized.value)
watch(() => busy.value || nvBusy.value || xeBusy.value || nrBusy.value, value => emit('busy', value))
let switchTimer: ReturnType<typeof setTimeout> | undefined
onBeforeUnmount(() => { clearTimeout(switchTimer); emit('busy', false) })
const feedback = ref('设置自动保存。支持实时切换的会话可直接更换 FG 引擎。')
const error = ref('')
const memoryWarning = ref('')
const sizingOpen = ref(false)
const maintenanceOpen = ref(false)
const xessAdvanced = ref(false)
const pauseError = ref('')
const pauseRevision = ref<number | null>(null)
const switchRevision = ref<number | null>(null)
const switchError = ref('')
const inputScale = computed(() => config.config.setting.other?.streamline_input_scale ?? 100)
const inputCap = computed(() => config.config.setting.other?.streamline_input_max_edge ?? 0)
const sizing = computed(() => ({ scalePercent: inputScale.value, maxEdge: inputCap.value }))
const runtimeSizing = computed(() => nv.value?.live.connected ? nv.value.live.inputScale : xe.value?.live.connected ? xe.value.live.runtime?.inputScale : undefined)
const nvRunning = computed(() => !!nv.value?.live.connected || ['launching', 'started', 'running'].includes(nv.value?.live.launchState ?? ''))
const running = computed(() => nvRunning.value || xe.value?.running || xe.value?.live.connected)
const runningFgEngine = computed<GraphicsEngine | null>(() => nvRunning.value && nv.value?.live.fg
  ? nv.value.live.fg.backend ?? 'dlss'
  : (xe.value?.running || xe.value?.live.connected) && xe.value.live.availableFg ? 'xess' : null)
const canSwitchFg = computed(() => !!nv.value?.live.connected && !!nv.value.live.fgBackendSwitchSupported
  && !(selection.value.srEnabled && selection.value.srEngine === 'xess'))
const fgRestartPending = computed(() => fgEngineNeedsRestart(selection.value.fgEngine, runningFgEngine.value, canSwitchFg.value))
const fgSwitchPending = computed(() => canSwitchFg.value && (runningFgEngine.value !== selection.value.fgEngine
  || (switchRevision.value !== null && (nv.value?.live.appliedRevision ?? 0) < switchRevision.value)))
watch(fgSwitchPending, pending => { if (!pending) { clearTimeout(switchTimer); switchError.value = '' } })
const engineName = (engine: GraphicsEngine | null) => engine === 'xess' ? 'XeSS FG' : engine === 'dlss' ? 'DLSS FG' : '等待运行状态'
const oldFgPaused = computed(() => nvRunning.value
  ? nv.value?.live.connected && nv.value.live.fresh && nv.value.live.requested === false
    && (pauseRevision.value === null || (nv.value.live.appliedRevision ?? 0) >= pauseRevision.value)
  : xe.value?.live.connected && xe.value.live.runtime?.fgRequested === false && !xe.value.live.runtime.fgActive)
watch(() => props.executable, () => { clearTimeout(switchTimer); pauseError.value = ''; pauseRevision.value = null; switchRevision.value = null; switchError.value = ''; xessAdvanced.value = false })
const issue = computed(() => {
  if (props.api !== 'vulkan') return '画面增强仅支持 Vulkan，请在模拟器的图形设置中启用。'
  if (route.value.conflict) return nrEnabled.value
    ? 'XeSS Native AA 暂不能与 NR 组合。请关闭 NR，或将重建引擎改为 DLSS；如需 FG，请选择 XeSS FG。'
    : 'XeSS Native AA 暂不能与 DLSS FG 组合。请将 FG 改为 XeSS，或将重建引擎改为 DLSS。'
  if (route.value.nvidia && !props.gpu?.hasNvidia) return '所选效果需要 NVIDIA 显卡。可选择 XeSS Native AA 与 XeSS FG，或关闭对应效果。'
  if (nrEnabled.value && !props.gpu?.nrSupported) return '当前显卡不支持 NR，请关闭 NR 后启动。'
  if (selection.value.srEnabled && selection.value.srEngine === 'dlss' && !props.gpu?.srSupported) return 'DLSS 重建需要 GeForce RTX 20／30／40／50 系列。'
  if (route.value.nvidia && selection.value.fgEnabled && (props.gpu?.fgMaxMultiplier ?? 0) < 2) return '当前 NVIDIA 混合启动路径需要 RTX 40／50 才能启用 FG；仅使用 XeSS 效果可走独立启动路径。'
  const fg = graphicsAdvanced(config.config.setting.other?.streamline_advanced).fg
  if (selection.value.fgEnabled && selection.value.fgEngine === 'dlss'
    && (fg.multiplier > (props.gpu?.fgMaxMultiplier ?? 0) || (fg.mode === 'dynamic' && (props.gpu?.fgMaxMultiplier ?? 0) < 3))) return '已保存的 DLSS FG 倍数或动态模式超出显卡支持范围，请在 FG 高级设置中调整。'
  return ''
})
const ready = computed(() => (!route.value.nvidia || (nv.value?.installed && nv.value.canUse)) && (!route.value.xess || (xe.value?.report?.installed && xe.value.report.targetReady)))
const canLaunch = computed(() => !locked.value && !!props.executable && props.api === 'vulkan' && route.value.any && !issue.value && ready.value && !running.value)
const routeLabel = computed(() => [nrEnabled.value ? 'NR' : '', selection.value.srEnabled ? selection.value.srEngine === 'dlss' ? 'DLSS SR / DLAA' : 'XeSS Native AA' : '', selection.value.fgEnabled ? `${selection.value.fgEngine === 'dlss' ? 'DLSS' : 'XeSS'} FG` : ''].filter(Boolean).join(' → ') || '尚未启用效果')
const restartPending = computed(() => fgRestartPending.value || (nv.value?.live.connected
  ? (!route.value.nvidia && !canSwitchFg.value) || ((nv.value.live.fg?.backend ?? 'dlss') !== selection.value.fgEngine && !canSwitchFg.value)
  : !!xe.value?.live.connected && (route.value.nvidia || (selection.value.srEnabled && !xe.value.live.availableNativeAa) || (selection.value.fgEnabled && !xe.value.live.availableFg))))
const srEngines = [{ title: 'DLSS SR / DLAA', value: 'dlss' }, { title: 'XeSS Native AA', value: 'xess' }]
const fgEngines = [{ title: 'DLSS FG', value: 'dlss' }, { title: 'XeSS FG · 2×', value: 'xess' }]
const message = (e: unknown) => e instanceof Error ? e.message : String(e)
async function work(action: () => Promise<void>) {
  if (locked.value) return
  busy.value = true; error.value = ''
  try { await action() } catch (e) { error.value = message(e) }
  finally { busy.value = false }
}
async function persist() {
  if (!config.config.setting.other || !intel.value) throw new Error('配置尚未读取完成，请重新检查组件。')
  const other = config.config.setting.other
  const advanced = graphicsAdvanced(other.streamline_advanced)
  const patch = { streamline_sr: selection.value.srEnabled && selection.value.srEngine === 'dlss',
    streamline_fg: selection.value.fgEnabled && route.value.nvidia,
    streamline_advanced: { ...advanced, fg: { ...advanced.fg, backend: selection.value.fgEngine } } }
  await updateSetting({ ...config.config.setting, other: { ...other, ...patch } })
  Object.assign(other, patch)
  await intel.value.configure({ nativeAa: selection.value.srEnabled && selection.value.srEngine === 'xess',
    fg: selection.value.fgEnabled && selection.value.fgEngine === 'xess', inputSizing: sizing.value })
  memoryWarning.value = ''
  try { localStorage.setItem(storageKey, JSON.stringify(selection.value)) }
  catch { memoryWarning.value = '无法记住本页引擎选择，重新打开后请核对组合。' }
}
async function change(patch: Partial<GraphicsSelection>) {
  await work(async () => {
    const previous = { ...selection.value }
    clearTimeout(switchTimer)
    switchError.value = ''; switchRevision.value = null
    Object.assign(selection.value, patch)
    await persist()
    feedback.value = '已保存，应用于下次画面增强启动。'
    if (previous.fgEngine !== selection.value.fgEngine && fgRestartPending.value) await pausePreviousFg()
    if (restartPending.value || issue.value) return
    try {
      if (canSwitchFg.value && !nv.value?.live.fresh) throw new Error('当前游戏画面未就绪，请恢复游戏画面后重试')
      if (nv.value?.live.connected && nv.value.live.fresh && (route.value.nvidia || canSwitchFg.value)) {
        const other = config.config.setting.other
        const result = await liveStreamlineFg(props.executable, selection.value.fgEnabled,
          selection.value.srEnabled && selection.value.srEngine === 'dlss' ? other.streamline_sr_mode ?? 'balanced' : 'off',
          other.streamline_sr_scale, other.streamline_sr_preset, undefined, undefined,
          canSwitchFg.value ? { fg: graphicsAdvanced(other.streamline_advanced).fg } : undefined)
        if (canSwitchFg.value) {
          if (!result.sentRevision) throw new Error('未收到 FG 切换请求确认')
          switchRevision.value = result.sentRevision
          switchTimer = setTimeout(() => {
            if (fgSwitchPending.value) switchError.value = '尚未确认切换完成，请恢复游戏画面后重试。'
          }, 8000)
        }
        feedback.value = '已保存并发送至当前游戏，等待运行状态确认。'
      } else if (xe.value?.live.connected && !route.value.nvidia) {
        await graphicsCommand('live_xess', { executable: props.executable,
          ...(xe.value.live.availableNativeAa ? { nativeAa: selection.value.srEnabled } : {}),
          ...(xe.value.live.availableFg ? { fg: selection.value.fgEnabled } : {}) })
        feedback.value = '已保存并发送至当前游戏，实际效果请查看本次运行。'
      }
    } catch (e) { switchError.value = `实时应用未确认：${message(e)}`; feedback.value = `设置已保存；${switchError.value}` }
  })
}
async function pausePreviousFg() {
  pauseError.value = ''; pauseRevision.value = null
  if (!fgRestartPending.value) return
  try {
    if (nvRunning.value) {
      const result = await liveStreamlineFg(props.executable, false)
      if (!result.sentRevision) throw new Error('尚未收到旧 FG 的暂停请求确认')
      pauseRevision.value = result.sentRevision
    } else {
      await graphicsCommand('live_xess', { executable: props.executable, fg: false })
      await intel.value?.poll()
    }
    feedback.value = '新引擎已保存，已请求暂停旧 FG；退出模拟器后从这里重新启动以应用。'
  } catch (e) {
    pauseError.value = `旧 FG 暂停请求失败：${message(e)}。请退出模拟器后重新启动。`
  }
}
function showFgOptions() {
  if (selection.value.fgEngine === 'xess') xessAdvanced.value = true
  else nvidia.value?.showFgOptions()
}
async function saveInput(scalePercent: number, maxEdge: number) {
  await work(async () => {
    const patch = { streamline_input_scale: scalePercent, streamline_input_max_edge: maxEdge }
    await updateSetting({ ...config.config.setting, other: { ...config.config.setting.other, ...patch } })
    Object.assign(config.config.setting.other, patch)
    await persist()
    feedback.value = '输入尺寸已保存。'
    if (nv.value?.live.connected && nv.value.live.inputScalingLiveSupported) {
      await liveStreamlineFg(props.executable, undefined, undefined, undefined, undefined, undefined, undefined, undefined, sizing.value)
      feedback.value = '输入尺寸已保存并发送，等待游戏下一帧应用。'
    }
    if (xe.value?.live.connected && xe.value.live.inputScalingLiveSupported) {
      await graphicsCommand('live_xess', { executable: props.executable, inputSizing: sizing.value })
      feedback.value = '输入尺寸已保存并发送，等待游戏下一帧应用。'
    }
  })
}
async function launch() {
  if (!canLaunch.value) return
  await work(async () => {
    await persist()
    if (route.value.nvidia) await nvidia.value?.operate('launch')
    else await intel.value?.start()
  })
}
async function disableUnsupportedNr() {
  await work(async () => {
    const other = { ...config.config.setting.other, streamline_nr: false }
    await updateSetting({ ...config.config.setting, other })
    config.config.setting.other.streamline_nr = false
    await persist()
    feedback.value = 'NR 已关闭，下次启动使用当前组合。'
  })
}
async function refresh() {
  await work(async () => {
    const results = await Promise.allSettled([nvidia.value?.inspect(), intel.value?.refresh()])
    for (const result of results) if (result.status === 'rejected') throw result.reason
  })
}
</script>

<template>
  <section
    class="enhancement"
    aria-label="画面增强配置"
    :aria-busy="locked"
  >
    <header class="pipeline-heading">
      <div><h2>组合你的画面增强</h2><p>按效果选择引擎，组件和启动方式会随组合匹配。</p></div>
      <v-btn
        variant="text"
        :prepend-icon="mdiRefresh"
        :disabled="locked"
        @click="refresh"
      >
        重新检查
      </v-btn>
    </header>
    <div
      class="pipeline-path"
      aria-label="画面处理顺序"
    >
      <button
        type="button"
        :aria-expanded="sizingOpen"
        @click="sizingOpen = !sizingOpen"
      >
        输入 {{ inputCap ? `长边 ${inputCap}` : `${inputScale}%` }}
      </button>
      <span aria-hidden="true">→</span><span :class="{ active: nrEnabled }">NR{{ nrEnabled ? ' 开启' : ' 关闭' }}</span>
      <span aria-hidden="true">→</span><span :class="{ active: selection.srEnabled }">{{ selection.srEnabled ? selection.srEngine === 'dlss' ? 'DLSS 重建' : 'XeSS Native AA' : '重建关闭' }}</span>
      <span aria-hidden="true">→</span><span :class="{ active: selection.fgEnabled }">{{ selection.fgEnabled ? `${selection.fgEngine === 'dlss' ? 'DLSS' : 'XeSS'} FG` : 'FG 关闭' }}</span>
    </div>
    <div
      v-if="sizingOpen"
      class="sizing-drawer"
    >
      <InputSizingControls
        :scale-percent="inputScale"
        :max-edge="inputCap"
        :disabled="locked"
        description="先调整画面输入尺寸，再执行所选效果。FG 始终接收恢复到窗口尺寸的画面。"
        :feedback="feedback"
        :runtime="runtimeSizing"
        @save="saveInput"
      />
    </div>

    <div class="effect-workbench">
      <section
        class="effect-row"
        aria-labelledby="unified-sr-title"
      >
        <div class="effect-heading">
          <div>
            <span
              class="effect-symbol"
              aria-hidden="true"
            >SR</span><h3 id="unified-sr-title">
              抗锯齿与细节重建
            </h3>
          </div><v-switch
            :model-value="selection.srEnabled"
            label="启用重建"
            color="primary"
            hide-details
            :disabled="locked"
            @update:model-value="change({ srEnabled: !!$event })"
          />
        </div>
        <div class="effect-options">
          <v-select
            :model-value="selection.srEngine"
            :items="srEngines"
            label="重建引擎"
            variant="outlined"
            hide-details
            :disabled="locked"
            @update:model-value="change({ srEngine: $event })"
          /><v-btn
            v-if="selection.srEngine === 'dlss'"
            variant="text"
            :prepend-icon="mdiTuneVariant"
            :disabled="locked || !gpu?.srSupported"
            @click="nvidia?.showSrOptions()"
          >
            高级设置
          </v-btn>
        </div>
        <p v-if="selection.srEngine === 'dlss'">
          {{ nv?.srSummary }}。重建窗口画面，文字与界面也会参与处理。
        </p>
        <p v-else>
          XeSS 当前提供同尺寸 Native AA，尚不提供超分辨率。零运动向量、无 jitter，每帧重置历史。
        </p>
      </section>
      <section
        class="effect-row"
        aria-labelledby="unified-fg-title"
      >
        <div class="effect-heading">
          <div>
            <span
              class="effect-symbol"
              aria-hidden="true"
            >FG</span><h3 id="unified-fg-title">
              帧生成
            </h3>
          </div><v-switch
            :model-value="selection.fgEnabled"
            label="启用帧生成"
            color="primary"
            hide-details
            :disabled="locked"
            @update:model-value="change({ fgEnabled: !!$event })"
          />
        </div>
        <div class="effect-options">
          <v-select
            :model-value="selection.fgEngine"
            :items="fgEngines"
            label="帧生成引擎"
            variant="outlined"
            :hint="canSwitchFg ? '当前会话支持实时切换 DLSS FG 与 XeSS FG。' : '同时安装两套组件后，以 NVIDIA 画面增强启动可实时切换 FG；旧会话需重新启动一次。'"
            persistent-hint
            :disabled="locked"
            @update:model-value="change({ fgEngine: $event })"
          /><v-btn
            variant="text"
            :prepend-icon="mdiTuneVariant"
            :disabled="locked || (selection.fgEngine === 'dlss' && !gpu?.hasNvidia)"
            @click="showFgOptions"
          >
            高级设置
          </v-btn>
        </div>
        <p v-if="selection.fgEngine === 'dlss'">
          {{ nv?.fgSummary }}。2× 需要 RTX 40／50；更高倍数和动态模式需要 RTX 50。
        </p>
        <p v-else>
          固定 2×，使用 XeLL 控制节奏。沿用零运动向量和固定深度；可接在 DLSS SR／DLAA 后使用。
        </p>
        <v-alert
          v-if="fgSwitchPending"
          :type="switchError ? 'warning' : 'info'"
          variant="tonal"
          density="compact"
          class="pipeline-notice"
          role="status"
        >
          当前：{{ engineName(runningFgEngine) }}；请求切换至 {{ engineName(selection.fgEngine) }}。
          {{ switchError || '等待游戏下一帧完成切换。' }}
          <v-btn
            v-if="switchError"
            variant="text"
            size="small"
            :disabled="locked"
            @click="change({ fgEngine: selection.fgEngine })"
          >
            重试切换
          </v-btn>
        </v-alert>
        <v-alert
          v-if="fgRestartPending"
          type="warning"
          variant="tonal"
          density="compact"
          class="pipeline-notice"
          role="status"
        >
          本次引擎：{{ engineName(runningFgEngine) }}；下次启动：{{ engineName(selection.fgEngine) }}。
          {{ pauseError || (oldFgPaused ? '旧 FG 已确认暂停。退出模拟器后重新以画面增强启动，才能加载新引擎。' : '尚未确认旧 FG 已暂停。请退出模拟器后重新启动，新引擎才能生效。') }}
          <v-btn
            v-if="!oldFgPaused"
            variant="text"
            size="small"
            :disabled="locked"
            @click="work(pausePreviousFg)"
          >
            暂停旧 FG
          </v-btn>
        </v-alert>
        <p
          v-else-if="runningFgEngine"
          class="save-status"
          role="status"
        >
          本次 FG 引擎：{{ engineName(runningFgEngine) }}。
        </p>
      </section>
    </div>
    <details
      class="nr-drawer"
      :open="nrEnabled"
    >
      <summary>神经渲染 NR <span>{{ nrEnabled ? '已开启' : gpu?.nrSupported ? '可选，RTX 50 系列' : '需要 RTX 50 系列' }}</span></summary>
      <NativeNrControls
        :executable="executable"
        :supported="!!gpu?.nrSupported"
        :disabled="locked || !gpu?.nrSupported"
        :live="nv?.live ?? { connected: false }"
        :refresh-key="nv?.report?.checkedAt"
        @busy="nrBusy = $event"
      />
      <v-btn
        v-if="nrEnabled && !gpu?.nrSupported"
        variant="text"
        :disabled="locked"
        @click="disableUnsupportedNr"
      >
        关闭不受支持的 NR
      </v-btn>
    </details>
    <v-alert
      v-if="issue"
      type="warning"
      variant="tonal"
      class="pipeline-notice"
      role="alert"
    >
      {{ issue }}
    </v-alert>
    <p
      class="save-status"
      role="status"
    >
      {{ feedback }} {{ memoryWarning }}<span v-if="restartPending && !fgRestartPending"> 当前会话使用另一套效果，请退出模拟器后重新启动。</span>
    </p>

    <section
      class="components"
      aria-labelledby="unified-components-title"
    >
      <div class="components-heading">
        <h3 id="unified-components-title">
          配套组件
        </h3><span>按当前组合检查</span>
      </div>
      <div
        v-if="gpu?.hasNvidia"
        class="component-line"
      >
        <v-icon
          :icon="mdiCheckCircleOutline"
          :color="nv?.installed ? 'success' : undefined"
          size="20"
        />
        <div><strong>NVIDIA 画面增强</strong><p>{{ nv?.status ?? '等待检查' }}<span v-if="route.nvidia">，当前组合需要</span></p></div>
        <v-btn
          v-if="!nv?.installed"
          variant="tonal"
          :disabled="locked || !!running || !nv?.report || (nv.report.installationState !== 'damaged' && (!nv.canUse || !nv.report.packageAvailable))"
          @click="nvidia?.operate(nv?.report?.installationState === 'damaged' ? 'uninstall' : 'install')"
        >
          {{ nv?.report?.installationState === 'damaged' ? '移除损坏安装' : '安装组件' }}
        </v-btn>
        <v-btn
          variant="text"
          :disabled="locked"
          @click="nvidia?.showDetails()"
        >
          详情
        </v-btn>
      </div>
      <div class="component-line">
        <v-icon
          :icon="mdiCheckCircleOutline"
          :color="xe?.report?.installed ? 'success' : undefined"
          size="20"
        />
        <div><strong>Intel XeSS</strong><p>{{ !xe?.report ? '等待检查' : xe.report.installed ? '已安装' : '未安装' }}<span v-if="route.xess">，当前组合需要</span></p></div>
        <v-btn
          v-if="!xe?.report?.installed"
          variant="tonal"
          :disabled="locked || !xe?.report?.supported || !(xe.report.bundledReady || xe.report.downloadAvailable) || !!running"
          @click="intel?.component(true)"
        >
          安装组件
        </v-btn>
        <v-btn
          variant="text"
          :disabled="locked || !xe?.report"
          @click="intel?.showDetails()"
        >
          详情
        </v-btn>
      </div>
      <div
        v-if="route.nvidia && nv?.report?.requiresTrialConfirmation && !nv.blocked.length"
        class="trial-choice"
      >
        <v-checkbox
          :model-value="nv.allowUnverified"
          label="允许尝试未适配的模拟器"
          hide-details
          :disabled="locked"
          @update:model-value="nvidia?.allowTrial(!!$event)"
        />
      </div>
      <ul
        v-if="route.nvidia && nv?.blocked.length"
        class="component-problems"
      >
        <li
          v-for="item in nv.blocked"
          :key="item.id"
        >
          {{ item.detail }}
        </li>
      </ul>
      <p
        v-if="route.xess && xe?.report && !xe.report.targetReady"
        class="component-problems"
      >
        {{ xe.report.targetMessage }}
      </p>
      <details
        :open="maintenanceOpen"
        class="component-maintenance"
        @toggle="maintenanceOpen = ($event.target as HTMLDetailsElement).open"
      >
        <summary>组件维护</summary><p>更新或卸载前请关闭使用画面增强的模拟器。已保存的效果设置会保留。</p><div class="maintenance-actions">
          <v-btn
            v-if="gpu?.hasNvidia"
            variant="text"
            :disabled="locked || !!running || !nv?.installed"
            @click="nvidia?.operate('uninstall')"
          >
            卸载 NVIDIA 组件
          </v-btn><v-btn
            variant="text"
            :disabled="locked || !!running"
            @click="intel?.component(true)"
          >
            校验并安装 XeSS
          </v-btn><v-btn
            variant="text"
            :disabled="locked || !!running || !xe?.report?.installed"
            @click="intel?.component(false)"
          >
            卸载 XeSS
          </v-btn>
        </div>
      </details>
    </section>
    <v-alert
      v-if="error || nv?.error || xe?.error"
      type="error"
      variant="tonal"
      class="pipeline-notice"
      role="alert"
    >
      {{ error || nv?.error || xe?.error }}
    </v-alert>
    <p
      v-if="nv?.message || xe?.feedback"
      class="save-status"
      role="status"
    >
      {{ nv?.message || xe?.feedback }}
    </p>
    <div class="launch-bar">
      <div>
        <h3>{{ running ? '模拟器会话正在运行' : '以画面增强启动' }}</h3><p>{{ routeLabel }}</p><p v-if="!route.any">
          先开启至少一项效果。
        </p><p v-else-if="!ready">
          安装所需组件并通过目标检查后即可启动。
        </p><p v-else>
          启动后在模拟器内打开游戏。普通启动不会加载这些效果。
        </p>
      </div><v-btn
        color="primary"
        variant="flat"
        size="large"
        :prepend-icon="mdiPlay"
        :loading="busy"
        :disabled="!canLaunch"
        @click="launch"
      >
        以画面增强启动
      </v-btn>
    </div>
    <p class="quality-note">
      当前无法取得游戏真实深度与运动向量，快速运动仍可能出现鬼影、闪烁或 UI 变形。更换引擎后请在相同场景对比。
    </p>

    <section
      v-if="nv && !nv.live.connected && nvRunning"
      class="xess-session"
      aria-label="NVIDIA 本次运行"
    >
      <h3>本次运行：{{ nv.live.fg?.backend === 'xess' ? 'DLSS 与 XeSS' : 'NVIDIA' }}</h3><p :role="nv.live.launchState === 'failed' ? 'alert' : 'status'">
        {{ nv.live.launchMessage || (nvRunning ? '模拟器已启动，等待游戏画面和实际增强状态。' : '本次会话已结束。') }}
      </p>
    </section>
    <section
      v-if="xe?.live.connected || xe?.running"
      class="xess-session"
      aria-label="XeSS 本次运行"
    >
      <h3>本次运行：XeSS</h3><p
        v-if="!xe.live.connected"
        :role="xe.live.launchState === 'failed' ? 'alert' : 'status'"
      >
        {{ xe.live.launchMessage || '模拟器已启动，等待游戏画面和实际增强状态。' }}
      </p>
      <template v-else>
        <p>Native AA：{{ xe.live.runtime?.nativeAaActive ? '正在处理' : '未处理' }}；FG：{{ xe.live.runtime?.fgActive ? '已提交生成帧' : '未插帧' }}</p><p v-if="xe.live.runtime?.sourceFps !== undefined">
          源帧 {{ xe.live.runtime.sourceFps.toFixed(1) }} FPS，SDK 提交 {{ xe.live.runtime.sdkQueuedFps?.toFixed(1) }} FPS
        </p><p
          v-if="xe.live.runtime?.failed"
          role="alert"
        >
          处理失败：{{ xe.live.runtime.error }}
        </p><div class="maintenance-actions">
          <v-btn
            v-if="xe.live.availableNativeAa"
            variant="text"
            :disabled="locked"
            @click="intel?.control('nativeAa', !xe?.live.runtime?.nativeAaRequested)"
          >
            {{ xe.live.runtime?.nativeAaRequested ? '暂停' : '恢复' }}本次 AA
          </v-btn><v-btn
            v-if="xe.live.availableFg"
            variant="text"
            :disabled="locked || fgRestartPending"
            @click="intel?.control('fg', !xe?.live.runtime?.fgRequested)"
          >
            {{ xe.live.runtime?.fgRequested ? '暂停' : '恢复' }}本次 FG
          </v-btn>
        </div><p class="quality-note">
          SDK 提交数不等同于实测屏幕帧率。
        </p>
      </template>
    </section>
    <StreamlineFgPanel
      ref="nvidia"
      unified
      :gpu="safeGpu"
      :executable="executable"
      :api="api"
      :disabled="disabled"
      :selected-fg-backend="selection.fgEngine"
      @busy="nvBusy = $event"
    />
    <XessPanel
      ref="intel"
      unified
      :executable="executable"
      :api="api"
      :disabled="disabled"
      @busy="xeBusy = $event"
    />
    <GraphicsAdvancedDialog
      v-model="xessAdvanced"
      title="XeSS FG 高级配置"
      title-id="xess-fg-advanced-title"
      description="当前 XeSS FG 使用固定配置，暂未开放可调的内部光流参数。"
    >
      <dl class="xess-fg-settings">
        <div><dt>生成倍数</dt><dd>2×，包含原始帧</dd></div>
        <div><dt>运动向量</dt><dd>零运动向量</dd></div>
        <div><dt>深度</dt><dd>固定 0.5</dd></div>
        <div><dt>延迟与节奏</dt><dd>XeLL</dd></div>
      </dl>
      <p class="quality-note">
        DLSS FG 的边界保护、光流阈值和 Reflex 不适用于 XeSS FG。
      </p>
      <template v-if="route.nvidia">
        <p class="quality-note">
          前级 DLSS 重建／NR 使用的 NVIDIA 光流可以单独设置，不会改变 XeSS FG 的零运动向量与固定深度。
        </p><v-btn
          variant="text"
          :disabled="locked"
          @click="xessAdvanced = false; nvidia?.showMotionOptions()"
        >
          DLSS 重建／NR 光流设置
        </v-btn>
      </template>
      <p
        v-if="fgRestartPending"
        class="quality-note"
      >
        本次仍加载 {{ engineName(runningFgEngine) }}，退出模拟器并重新启动后应用所选引擎。
      </p>
    </GraphicsAdvancedDialog>
  </section>
</template>

<style scoped>
.enhancement { --workbench-blue: #356ac3; --workbench-teal: #287d76; margin-top: 30px; color: rgb(var(--v-theme-on-surface)); }
.pipeline-heading, .effect-heading, .effect-heading > div, .effect-options, .components-heading, .component-line, .launch-bar { display: flex; align-items: center; gap: 16px; }
.pipeline-heading, .effect-heading, .components-heading, .launch-bar { justify-content: space-between; }
.pipeline-heading h2 { font-size: 23px; font-weight: 650; letter-spacing: -.35px; }
.enhancement p { font-size: 14px; line-height: 1.75; max-width: 76ch; }
.pipeline-heading p, .effect-row > p, .component-line p, .components-heading > span, .quality-note { color: rgba(var(--v-theme-on-surface), .7); }
.pipeline-path { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; padding: 18px 0 22px; color: rgba(var(--v-theme-on-surface), .55); font-size: 14px; }
.pipeline-path button { color: rgb(var(--v-theme-primary)); padding: 5px 10px; background: rgba(var(--v-theme-primary), .08); border-radius: 6px; text-align: left; }
.pipeline-path .active { color: rgb(var(--v-theme-on-surface)); font-weight: 600; }
.pipeline-path button:focus-visible, summary:focus-visible { outline: 2px solid rgb(var(--v-theme-primary)); outline-offset: 4px; }
.sizing-drawer { padding: 20px 24px; margin-bottom: 20px; border-radius: 10px; background: rgba(var(--v-theme-primary), .035); }
.effect-workbench { border-radius: 12px; border: 1px solid rgba(var(--v-theme-on-surface), .15); background: rgb(var(--v-theme-surface)); overflow: hidden; }
.effect-row { padding: 22px 26px 24px; }
.effect-row + .effect-row { border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.effect-heading { margin-bottom: 16px; }
.effect-heading > div { gap: 12px; }
.effect-heading h3 { font-size: 18px; font-weight: 600; }
.effect-heading :deep(.v-switch) { flex: 0 0 auto; }
.effect-symbol { display: inline-grid; place-items: center; width: 38px; height: 38px; color: var(--workbench-blue); background: #edf2fc; border-radius: 8px; font-size: 14px; font-weight: 750; letter-spacing: -.4px; }
.effect-row:nth-child(2) .effect-symbol { color: var(--workbench-teal); background: #eaf4f1; }
.effect-options { margin-bottom: 14px; }
.effect-options :deep(.v-select) { max-width: 360px; min-width: 0; }
.nr-drawer { margin: 16px 0 22px; padding: 16px 22px; background: rgba(var(--v-theme-on-surface), .035); border-radius: 8px; }
summary { cursor: pointer; font-size: 14px; font-weight: 600; }
.nr-drawer summary > span { font-weight: 400; color: rgba(var(--v-theme-on-surface), .65); margin-left: 10px; }
.nr-drawer :deep(.nr-setting) { border: 0; padding: 24px 0 4px; }
.save-status { margin: 14px 0; color: rgba(var(--v-theme-on-surface), .7); }
.components { margin-top: 28px; }
.components-heading { padding-bottom: 12px; }
.components h3, .launch-bar h3, .xess-session h3 { font-size: 17px; font-weight: 650; }
.components-heading > span { font-size: 13px; }
.component-line { padding: 14px 0; border-top: 1px solid rgba(var(--v-theme-on-surface), .1); }
.component-line > div { flex: 1; }
.component-line strong { font-size: 14px; font-weight: 600; }
.component-line p { font-size: 13px; }
.component-maintenance { margin-top: 12px; }
.component-maintenance > p { margin-top: 12px; }
.maintenance-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.component-problems { padding: 12px 0 8px 20px; color: rgb(var(--v-theme-error)); font-size: 14px; }
.pipeline-notice { margin: 18px 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.launch-bar { margin-top: 26px; padding: 22px 24px; border-radius: 10px; background: rgba(var(--v-theme-primary), .065); }
.launch-bar > div { min-width: 0; }
.launch-bar p { overflow-wrap: anywhere; margin-top: 4px; }
.launch-bar > .v-btn { flex-shrink: 0; }
.quality-note { margin-top: 14px; font-size: 13px !important; }
.xess-session { padding: 24px; margin-top: 24px; border: 1px solid rgba(var(--v-theme-on-surface), .14); border-radius: 10px; }
.xess-fg-settings > div { display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: 16px; padding: 12px 0; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.xess-fg-settings dt { color: rgba(var(--v-theme-on-surface), .65); }
@media (max-width: 650px) { .pipeline-heading { align-items: flex-start; gap: 8px; } .pipeline-heading h2 { font-size: 21px; } .pipeline-heading > .v-btn { padding-inline: 8px; } .pipeline-path { gap: 8px; font-size: 12px; } .effect-row { padding: 18px; } .effect-heading { flex-wrap: wrap; gap: 4px; } .effect-heading h3 { font-size: 17px; } .effect-options { flex-wrap: wrap; gap: 8px; } .effect-options :deep(.v-select) { flex-basis: 100%; max-width: none; } .component-line { gap: 8px; flex-wrap: wrap; } .component-line > div { min-width: 50%; } .launch-bar { flex-direction: column; align-items: stretch; padding: 20px; } .sizing-drawer { padding: 18px; } }
</style>
