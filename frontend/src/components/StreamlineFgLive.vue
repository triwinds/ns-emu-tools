<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { liveStreamlineFg, type FgLive, type FgSample } from '@/utils/streamlineFg'
const props = defineProps<{ executable: string }>()
const live = ref<FgLive>({ connected: false })
const error = ref('')
const pending = ref(0)
const desired = ref(false)
const sending = ref(false)
const selected = ref<number | null>(null)
const plot = ref<SVGSVGElement | null>(null)
const plotWidth = ref(760)
let resize: ResizeObserver | undefined
onMounted(() => {
  resize = new ResizeObserver(entries => { plotWidth.value = Math.max(240, entries[0].contentRect.width) })
  if (plot.value) resize.observe(plot.value)
})
let generation = 0
let timer: ReturnType<typeof setTimeout> | undefined
let disposed = false
let deadline = 0
const samples = computed(() => live.value.samples ?? [])
const current = computed(() => samples.value[samples.value.length - 1])
const inspected = computed(() => selected.value === null ? current.value : samples.value[selected.value])
const number = (value: number | null | undefined) => value == null ? '—' : value.toFixed(1)
const state = computed(() => {
  if (!live.value.connected) return '未连接'
  if (pending.value || sending.value) return '正在应用设置'
  if (!live.value.requested) return 'FG 已关闭'
  if (!live.value.fresh) return '等待游戏画面'
  if (live.value.active) return 'FG 正在运行'
  const reasons: Record<string, string> = { background: '已暂停：游戏不在前台', warmup: '正在预热', window_operation: '已停用：窗口变化触发保护，请重新启动游戏', sdk_status: '已暂停：SDK 状态异常', below_minimum_extent: '窗口尺寸过小', user_disabled: 'FG 已关闭', frame_budget: '诊断帧数已用完' }
  return reasons[live.value.reason ?? ''] ?? '等待运行条件满足'
})
const ceiling = computed(() => Math.max(60, Math.ceil(Math.max(...samples.value.flatMap(s => [s.appFps ?? 0, s.presentFps ?? 0])) / 30) * 30))
const x = (s: FgSample) => 42 + (s.time - ((current.value?.time ?? s.time) - 60000)) / 60000 * (plotWidth.value - 60)
const y = (n: number) => 160 - Math.min(n / ceiling.value, 1) * 140
function curve(field: 'appFps' | 'presentFps') {
  let pen = false
  return samples.value.map(s => {
    const n = s[field]
    if (n === null) { pen = false; return '' }
    const segment = `${pen ? 'L' : 'M'}${x(s).toFixed(1)},${y(n).toFixed(1)}`
    pen = true
    return segment
  }).join(' ')
}
async function poll(token: number) {
  if (disposed || token !== generation || !props.executable) return
  try {
    const result = await liveStreamlineFg(props.executable)
    if (disposed || token !== generation) return
    live.value = result
    if (pending.value && (result.appliedRevision ?? 0) >= pending.value) { pending.value = 0; error.value = '' }
    if (pending.value && Date.now() > deadline) { pending.value = 0; error.value = '设置尚未确认。请回到游戏恢复画面后检查开关状态。' }
  } catch (e) {
    if (token === generation) { live.value = { connected: false }; error.value = String(e) }
  } finally {
    if (!disposed && token === generation) timer = setTimeout(() => poll(token), 1000)
  }
}
async function toggle(value: boolean | null) {
  if (!live.value.connected || sending.value || pending.value) return
  const token = generation
  sending.value = true
  desired.value = !!value
  error.value = ''
  try {
    const result = await liveStreamlineFg(props.executable, !!value)
    if (token !== generation) return
    pending.value = result.sentRevision ?? 0
    deadline = Date.now() + 8000
  } catch (e) { if (token === generation) error.value = String(e) }
  finally { if (token === generation) sending.value = false }
}
watch(() => props.executable, () => {
  clearTimeout(timer)
  generation++
  live.value = { connected: false }
  pending.value = 0
  sending.value = false
  selected.value = null
  error.value = ''
  void poll(generation)
}, { immediate: true })
onBeforeUnmount(() => { disposed = true; generation++; clearTimeout(timer); resize?.disconnect() })
</script>

<template>
  <section
    class="fg-live"
    aria-labelledby="fg-live-title"
  >
    <div class="live-heading">
      <div>
        <h3 id="fg-live-title">
          运行控制
        </h3><p role="status">
          {{ state }}
        </p>
      </div>
      <v-switch
        :model-value="pending || sending ? desired : !!live.requested"
        :disabled="!live.connected || !!pending || sending"
        label="FG 帧生成"
        color="secondary"
        hide-details
        density="compact"
        @update:model-value="toggle"
      />
    </div>
    <p
      v-if="error"
      class="live-error"
      role="alert"
    >
      {{ error }}
    </p>
    <p
      v-if="!live.connected"
      class="live-note"
    >
      通过工具箱的“以 FG 启动”入口打开模拟器后，可在这里实时切换。
    </p>
    <div class="live-sr">
      <span>SR 超分辨率</span><span>尚未接入</span>
    </div>
    <figure class="rate-chart">
      <figcaption>
        <strong>帧率记录</strong>
        <span class="rate-readout app-rate">游戏提交 {{ number(live.connected && live.fresh ? inspected?.appFps : null) }} FPS</span>
        <span class="rate-readout present-rate">呈现完成 {{ number(live.connected && live.fresh ? inspected?.presentFps : null) }} FPS</span>
      </figcaption>
      <svg
        ref="plot"
        :viewBox="`0 0 ${plotWidth} 194`"
        role="img"
        aria-label="最近 60 秒游戏提交帧率与原生呈现完成速率曲线，按每秒实际次数统计"
        @mouseleave="selected = null"
      >
        <g
          v-for="tick in [0, ceiling / 2, ceiling]"
          :key="tick"
          class="chart-grid"
        >
          <line
            x1="42"
            :x2="plotWidth - 18"
            :y1="y(tick)"
            :y2="y(tick)"
          />
          <text
            x="32"
            :y="y(tick) + 4"
            text-anchor="end"
          >{{ tick }}</text>
        </g>
        <path
          :d="curve('presentFps')"
          class="present-line"
        />
        <path
          :d="curve('appFps')"
          class="app-line"
        />
        <g
          v-for="(s, index) in samples"
          :key="s.time"
        >
          <rect
            :x="x(s) - 6"
            y="12"
            width="12"
            height="152"
            fill="transparent"
            @mouseenter="selected = index"
          ><title>{{ new Date(s.time).toLocaleTimeString() }}：游戏提交 {{ number(s.appFps) }} FPS，呈现完成 {{ number(s.presentFps) }} FPS</title></rect>
        </g>
        <line
          v-if="selected !== null && inspected"
          :x1="x(inspected)"
          :x2="x(inspected)"
          y1="20"
          y2="160"
          class="chart-cursor"
        />
        <text
          x="42"
          y="187"
        >60 秒前</text><text
          :x="plotWidth - 18"
          y="187"
          text-anchor="end"
        >{{ live.connected ? '现在' : '最近记录' }}</text>
        <text
          v-if="!samples.some(s => s.appFps !== null)"
          :x="plotWidth / 2"
          y="92"
          text-anchor="middle"
        >等待游戏产生帧率数据</text>
      </svg>
      <p class="live-note">
        每秒采样，保留最近 60 秒。呈现完成包含生成帧，按原生呈现完成次数统计；不代表屏幕实际扫描帧率。数据中断时留空。
      </p>
    </figure>
  </section>
</template>

<style scoped>
.fg-live { padding: 22px 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.live-heading { display: flex; align-items: center; justify-content: space-between; gap: 20px; }
.live-heading h3 { font-size: 16px; font-weight: 600; }.live-heading p { font-size: 13px; margin-top: 6px; }
.live-heading :deep(.v-switch) { flex: 0 0 auto; }
.live-sr { display: flex; gap: 18px; font-size: 12px; opacity: .65; margin: 12px 0 20px; }
.rate-chart { margin: 0; }.rate-chart figcaption { display: flex; flex-wrap: wrap; gap: 10px 24px; align-items: baseline; font-size: 13px; }
.rate-readout { font-variant-numeric: tabular-nums; }.app-rate { color: #64b5f6; }.present-rate { color: #4db6ac; }
.rate-chart svg { width: 100%; display: block; margin-top: 12px; overflow: visible; }
.rate-chart text { fill: currentColor; font-size: 11px; opacity: .7; }.chart-grid line { stroke: currentColor; opacity: .12; }
.app-line, .present-line { fill: none; stroke-width: 2; vector-effect: non-scaling-stroke; }.app-line { stroke: #64b5f6; }.present-line { stroke: #4db6ac; stroke-dasharray: 5 3; }
.chart-cursor { stroke: currentColor; opacity: .35; }.live-note { font-size: 12px; line-height: 1.7; opacity: .7; margin-top: 12px; }.live-error { color: rgb(var(--v-theme-error)); font-size: 13px; overflow-wrap: anywhere; }
@media (max-width: 450px) { .fg-live { padding: 20px 18px; }.live-heading { align-items: flex-start; flex-direction: column; gap: 4px; }.rate-chart figcaption { flex-direction: column; gap: 6px; } }
</style>
