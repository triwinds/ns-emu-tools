<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { updateSetting } from '@/utils/tauri'
import { liveStreamlineFg, type FgLive } from '@/utils/streamlineFg'

const props = defineProps<{ executable: string; disabled: boolean; live: FgLive }>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const config = useConfigStore()
const busy = ref(false)
const error = ref('')
const defaultFeedback = '自动保存；连接以画面增强启动的游戏后实时应用。'
const feedback = ref(defaultFeedback)
const pending = ref(0)
let generation = 0
watch(() => props.executable, () => {
  generation++
  pending.value = 0
  feedback.value = defaultFeedback
  error.value = ''
})
onBeforeUnmount(() => { generation++ })
watch(() => props.live, live => {
  if (!pending.value) return
  if (!live.connected) {
    pending.value = 0
    feedback.value = '已保存；游戏连接已断开，下次以画面增强启动时使用。'
  } else if (live.fresh && (live.inputScale?.appliedRevision ?? 0) >= pending.value) {
    pending.value = 0
    feedback.value = live.inputScale?.error ? '已保存；当前画面未能应用缩放，请查看错误。' : '输入尺寸已在当前游戏中生效。'
  } else if (!live.fresh) {
    feedback.value = '已保存，等待游戏恢复画面后应用；无需重启。'
  }
})
const savedScale = computed(() => config.config.setting.other?.streamline_input_scale ?? 100)
const savedCap = computed(() => config.config.setting.other?.streamline_input_max_edge ?? 0)
const scale = ref(savedScale.value)
const cap = ref(savedCap.value || 1920)
watch(savedScale, value => { scale.value = value })
watch(savedCap, value => { cap.value = value || 1920 })
watch(busy, value => emit('busy', value), { flush: 'sync' })
const modes = [{ title: '按输入比例', value: 'percentage' }, { title: '固定长边上限', value: 'max_edge' }]
const validCap = (value: number) => Number.isInteger(value) && value >= 320 && value <= 8192
const capRules = [(value: string | number) => validCap(Number(value)) || '请输入 320～8192 的整数像素值']
async function save(percent: number, maxEdge: number) {
  if (props.disabled || busy.value || !config.config.setting.other) return
  if (!Number.isInteger(percent) || percent < 50 || percent > 100 || (maxEdge !== 0 && !validCap(maxEdge))) return
  const requestGeneration = generation
  const executable = props.executable
  busy.value = true
  error.value = ''
  try {
    const setting = config.config.setting
    const patch = { streamline_input_scale: percent, streamline_input_max_edge: maxEdge }
    if (percent !== savedScale.value || maxEdge !== savedCap.value) {
      await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
      Object.assign(config.config.setting.other, patch)
    }
    if (requestGeneration !== generation) return
    const status = executable ? await liveStreamlineFg(executable) : { connected: false } as FgLive
    if (requestGeneration !== generation) return
    if (!status.connected) {
      pending.value = 0
      feedback.value = '已保存；下次以画面增强启动游戏时使用。'
    } else if (!status.inputScalingLiveSupported) {
      pending.value = 0
      feedback.value = '已保存；请更新画面增强组件并重新启动一次，之后可实时调整。'
    } else {
      const result = await liveStreamlineFg(executable, undefined, undefined, undefined, undefined, undefined, undefined, undefined, { scalePercent: percent, maxEdge })
      if (requestGeneration !== generation) return
      pending.value = result.sentInputScaleRevision ?? 0
      feedback.value = pending.value ? '已保存，等待下一帧应用新尺寸；无需重启。' : '已保存，但尚未收到应用请求确认。'
    }
  } catch (e) {
    if (requestGeneration === generation) {
      pending.value = 0
      error.value = `输入缩放保存或实时应用失败：${String(e)}`
    }
  }
  finally { scale.value = savedScale.value; cap.value = savedCap.value || 1920; busy.value = false }
}
function changeMode(mode: string | null) {
  if (mode === 'percentage') return save(savedScale.value, 0)
  if (mode === 'max_edge') return save(savedScale.value, Number(cap.value))
}
function saveCap() {
  const value = Number(cap.value)
  if (validCap(value)) return save(savedScale.value, value)
}
</script>

<template>
  <section
    class="input-scale"
    aria-labelledby="input-scale-title"
  >
    <h3 id="input-scale-title">
      输入尺寸缩放
    </h3>
    <p>先缩小游戏画面输入，再交给 NR／SR 处理。关闭 NR 时也可使用；FG 接收最终窗口尺寸的画面。</p>
    <v-select
      :model-value="savedCap ? 'max_edge' : 'percentage'"
      :items="modes"
      label="尺寸方式"
      variant="outlined"
      density="compact"
      :disabled="disabled || busy"
      @update:model-value="changeMode"
    />
    <v-text-field
      v-if="savedCap"
      v-model.number="cap"
      label="输入长边上限"
      type="number"
      min="320"
      max="8192"
      step="1"
      suffix="像素"
      variant="outlined"
      density="compact"
      :rules="capRules"
      :disabled="disabled || busy"
      hint="超过上限时等比缩小；较小输入不放大。例如 2560 × 1325、上限 1920，输入为 1920 × 993。"
      persistent-hint
      @blur="saveCap"
      @keyup.enter="saveCap"
    />
    <template v-else>
      <div class="input-scale-label">
        <label id="input-scale-percent">每边比例</label><output>{{ scale }}%</output>
      </div>
      <v-slider
        v-model="scale"
        :min="50"
        :max="100"
        :step="1"
        color="primary"
        thumb-label
        hide-details
        aria-labelledby="input-scale-percent"
        :disabled="disabled || busy"
        @end="save(Math.round(scale), 0)"
        @keyup="save(Math.round(scale), 0)"
      />
    </template>
    <p>仅支持 SDR，缩小后的输入至少为 320 × 180。输出保持窗口尺寸；文字和细线也会缩放。100% 且不设上限时保持原输入。</p>
    <p role="status">
      {{ feedback }}
    </p>
    <p
      v-if="live.connected && live.fresh && live.inputScale?.inputExtent"
      role="status"
    >
      当前输入 {{ live.inputScale.originalExtent?.join(' × ') }} → {{ live.inputScale.inputExtent.join(' × ') }} · 输出 {{ live.inputScale.outputExtent?.join(' × ') }}{{ live.inputScale.active ? '' : ' · 未缩放' }}
    </p>
    <p
      v-if="error || (live.connected && live.fresh && live.inputScale?.error)"
      class="input-error"
      role="alert"
    >
      {{ error || live.inputScale?.error }}
    </p>
    <v-btn
      variant="text"
      size="small"
      :disabled="disabled || busy"
      @click="save(100, 0)"
    >
      恢复原始输入尺寸
    </v-btn>
  </section>
</template>

<style scoped>
.input-scale { padding: 20px 0; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.input-scale h3 { font-size: 15px; font-weight: 600; margin-bottom: 8px; }
.input-scale p { font-size: .85rem; line-height: 1.6; margin: 8px 0 12px; color: rgba(var(--v-theme-on-surface), .72); }
.input-scale-label { display: flex; justify-content: space-between; gap: 12px; margin-bottom: 12px; font-weight: 600; }
.input-scale .input-error { color: rgb(var(--v-theme-error)); }
</style>
