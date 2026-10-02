<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { updateSetting } from '@/utils/tauri'
import { getNativeNrComponent, installNativeNrRuntime, uninstallNativeNrRuntime, liveStreamlineFg, type FgLive, type NativeNrComponent } from '@/utils/streamlineFg'
import { useProgressStore } from '@/stores/ProgressStore'

const props = defineProps<{ executable: string; disabled: boolean; live: FgLive; refreshKey?: string }>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const config = useConfigStore()
const progress = useProgressStore()
const component = ref<NativeNrComponent | null>(null)
const busy = ref(false)
const error = ref('')
const feedback = ref('自动保存；专用启动后可实时切换。')
const pending = ref(0)
let deadline = 0
let generation = 0
let inspectionPending = false
const enabled = computed(() => config.config.setting.other?.streamline_nr ?? false)
const savedIntensity = computed(() => config.config.setting.other?.streamline_nr_intensity ?? 100)
const intensity = ref(savedIntensity.value)
watch(savedIntensity, value => { intensity.value = value })
watch(busy, value => emit('busy', value), { flush: 'sync' })
const state = computed(() => {
  if (!props.live.connected) return '尚未连接游戏'
  if (pending.value) return '正在应用'
  if (!props.live.nrLiveSupported) return '本次启动未准备 NR'
  if (!props.live.fresh) return '等待游戏画面'
  if (props.live.nr?.active) return 'NR 正在运行'
  const reason = props.live.nr?.reason ?? 'waiting'
  return ({ disabled: 'NR 已关闭', motion_unavailable: '暂停：等待有效硬件光流', window_transition: '暂停：窗口正在变化', resource_preparation_failed: 'NR 资源准备失败', waiting: '等待游戏画面' } as Record<string, string>)[reason] ?? `NR 未运行：${reason}`
})
watch(() => props.live, value => {
  if (!pending.value) return
  if (value.connected && value.fresh && (value.nr?.appliedRevision ?? 0) >= pending.value) {
    pending.value = 0
    feedback.value = '当前游戏已应用 NR 设置。'
  } else if (Date.now() > deadline) {
    pending.value = 0
    feedback.value = '已保存，尚未收到生效确认；回到游戏后查看实际状态。'
  }
})
watch(() => [props.executable, props.refreshKey], () => {
  generation++
  inspectionPending = true
  component.value = null
  error.value = ''
  pending.value = 0
  feedback.value = '自动保存；专用启动后可实时切换。'
}, { immediate: true, flush: 'sync' })
async function inspect() {
  if (busy.value || props.disabled || !props.executable) return
  const token = generation
  busy.value = true
  error.value = ''
  try {
    const result = await getNativeNrComponent()
    if (token === generation) component.value = result
  }
  catch (e) { if (token === generation) error.value = String(e) }
  finally { busy.value = false }
}
watch(() => [props.executable, props.refreshKey, props.disabled, busy.value], () => {
  if (!inspectionPending || !props.executable || props.disabled || busy.value) return
  inspectionPending = false
  void inspect()
}, { immediate: true, flush: 'post' })
onBeforeUnmount(() => { generation++ })
async function save(on: boolean, strength: number) {
  if (busy.value || props.disabled || !config.config.setting.other) return
  if (!Number.isInteger(strength) || strength < 0 || strength > 100) return
  const token = generation
  const executable = props.executable
  busy.value = true
  error.value = ''
  try {
    const setting = config.config.setting
    const patch = { streamline_nr: on, streamline_nr_intensity: strength }
    await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
    Object.assign(config.config.setting.other, patch)
    feedback.value = '已保存；下次专用启动时使用。'
    if (token !== generation || !executable || !props.live.connected) return
    if (!props.live.nrLiveSupported) { feedback.value = '已保存；安装 NR 组件后需重新专用启动，才能在游戏内启用 NR。'; return }
    const result = await liveStreamlineFg(executable, undefined, undefined, undefined, undefined, on, strength / 100)
    if (token !== generation) return
    pending.value = result.sentNrRevision ?? 0
    deadline = Date.now() + 10000
    feedback.value = '已保存，正在应用到当前游戏…'
  } catch (e) { error.value = `NR 设置操作失败：${String(e)}` }
  finally { busy.value = false }
}
async function installRuntime() {
  if (busy.value || props.disabled) return
  busy.value = true
  error.value = ''
  try {
    component.value = await installNativeNrRuntime()
    feedback.value = 'NR 组件已下载并校验安装。当前游戏需重新专用启动后才能使用新组件。'
  } catch (e) { error.value = String(e) }
  finally { progress.closeDialog(); busy.value = false }
}
async function removeRuntime() {
  if (busy.value || props.disabled) return
  busy.value = true
  error.value = ''
  try {
    component.value = await uninstallNativeNrRuntime()
    const setting = config.config.setting
    await updateSetting({ ...setting, other: { ...setting.other, streamline_nr: false } })
    config.config.setting.other.streamline_nr = false
    if (props.executable && props.live.connected && props.live.nrLiveSupported) {
      const result = await liveStreamlineFg(props.executable, undefined, undefined, undefined, undefined, false)
      pending.value = result.sentNrRevision ?? 0
      deadline = Date.now() + 10000
    }
    feedback.value = 'NR 组件已卸载，NR 已请求关闭；当前会话的私有副本保留到游戏退出。'
  } catch (e) { error.value = String(e) }
  finally { busy.value = false }
}
</script>

<template>
  <div class="nr-setting">
    <div class="nr-heading">
      <h3>重建源画面 · 原生 NR</h3><v-chip
        size="x-small"
        variant="outlined"
      >
        实验版
      </v-chip>
    </div>
    <p>直接处理游戏源画面，可与抗锯齿和帧生成配合使用。依赖 NVIDIA 硬件光流，会增加 GPU 开销。</p>
    <v-switch
      :model-value="enabled"
      label="启用原生 NR"
      color="primary"
      density="compact"
      inset
      hide-details
      :loading="busy"
      :disabled="disabled || busy || !!pending || (!enabled && (!component?.installed || !component?.packageReady))"
      @update:model-value="value => value !== null && save(value, savedIntensity)"
    />
    <v-slider
      v-if="enabled"
      v-model="intensity"
      label="NR 强度"
      min="0"
      max="100"
      step="1"
      thumb-label
      hide-details
      :disabled="disabled || busy || !!pending || !enabled"
      @end="save(enabled, Math.round(intensity))"
      @keyup="save(enabled, Math.round(intensity))"
    >
      <template #append>
        <span class="nr-strength">{{ intensity }}%</span>
      </template>
    </v-slider>
    <p
      class="nr-feedback"
      role="status"
    >
      {{ feedback }}
    </p>
    <p
      v-if="component && !component.installed"
      class="nr-component"
    >
      NR 组件未安装。展开下方“NR 组件维护”，下载恢复后即可启用。
    </p>
    <details class="nr-maintenance">
      <summary>NR 组件维护{{ component?.installed ? ' · 已安装' : ' · 未就绪' }}</summary>
      <div class="nr-actions">
        <v-btn
          size="small"
          variant="outlined"
          :loading="busy"
          :disabled="disabled || busy || !component?.supported || !component?.packageReady"
          @click="installRuntime"
        >
          {{ component?.installed ? '重新安装 NR 组件' : '下载恢复 NR 组件' }}
        </v-btn>
        <v-btn
          v-if="component?.installed"
          size="small"
          variant="text"
          :disabled="disabled || busy"
          @click="removeRuntime"
        >
          卸载 NR 运行库
        </v-btn>
        <v-btn
          size="small"
          variant="text"
          :disabled="disabled || busy || !executable"
          @click="inspect"
        >
          重新检查
        </v-btn>
      </div>
      <p class="nr-component">
        {{ component?.message ?? (busy ? '正在检查原生 NR 组件…' : error ? '原生 NR 组件检查失败，请重新检查。' : executable ? '等待自动检查原生 NR 组件…' : '选择主程序后自动检查原生 NR 组件。') }}
      </p>
      <p
        v-if="component?.installed"
        class="nr-component"
      >
        运行库 {{ component.runtimeVersion }} · {{ component.architecture }} · SHA-256 {{ component.runtimeSha256?.slice(0, 12) }}
      </p>
    </details>
    <p
      class="nr-state"
      role="status"
    >
      {{ state }}<span v-if="live.connected && live.fresh && live.nr?.active && live.nr.input"> · 输入 {{ live.nr.input.join(' × ') }} · 实际强度 {{ Math.round((live.nr.intensity ?? 1) * 100) }}%</span>
    </p>
    <p
      v-if="error"
      class="nr-error"
      role="alert"
    >
      {{ error }}
    </p>
  </div>
</template>

<style scoped>
.nr-setting { padding: 20px 0; }
.nr-heading { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
.nr-heading h3 { font-size: 15px; font-weight: 600; }
.nr-setting p { font-size: .85rem; line-height: 1.6; margin-bottom: 8px; }
.nr-feedback, .nr-component { color: rgba(var(--v-theme-on-surface), .72); overflow-wrap: anywhere; }
.nr-strength { min-width: 42px; font-variant-numeric: tabular-nums; font-size: .85rem; }
.nr-actions { display: flex; gap: 8px; flex-wrap: wrap; margin: 12px 0 8px; }
.nr-state { font-weight: 500; }
.nr-maintenance summary { cursor: pointer; font-size: 12px; padding: 8px 0; color: rgba(var(--v-theme-on-surface), .72); }
.nr-maintenance summary:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 3px; }
.nr-error { color: rgb(var(--v-theme-error)); }
</style>
