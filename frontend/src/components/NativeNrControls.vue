<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { updateSetting } from '@/utils/tauri'
import { getNativeNrComponent, installNativeNrRuntime, uninstallNativeNrRuntime, liveStreamlineFg, type FgLive, type NativeNrComponent } from '@/utils/streamlineFg'
import { useProgressStore } from '@/stores/ProgressStore'
import { mdiTuneVariant } from '@mdi/js'
import GraphicsAdvancedDialog from './GraphicsAdvancedDialog.vue'
import NrPresetControls from './NrPresetControls.vue'
import { bypassNrAdditions, cloneNrOptions, graphicsAdvanced, nrLook, nrSpatialLook, nrTemporalLook, nrSecondPass, type NrOptions } from '@/utils/graphicsAdvanced'

const props = defineProps<{ executable: string; disabled: boolean; supported: boolean; live: FgLive; refreshKey?: string }>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const config = useConfigStore()
const progress = useProgressStore()
const component = ref<NativeNrComponent | null>(null)
const busy = ref(false)
const presetBusy = ref(false)
const working = computed(() => busy.value || presetBusy.value)
const advanced = ref(false)
const error = ref('')
const feedback = ref('自动保存；专用启动后可实时切换。')
const pending = ref(0)
let deadline = 0
let generation = 0
let inspectionPending = false
const enabled = computed(() => config.config.setting.other?.streamline_nr ?? false)
const savedIntensity = computed(() => config.config.setting.other?.streamline_nr_intensity ?? 100)
const intensity = ref(savedIntensity.value)
const savedConsolidated = computed(() => config.config.setting.other?.streamline_nr_consolidated ?? false)
const savedOptions = computed(() => graphicsAdvanced(config.config.setting.other?.streamline_advanced).nr)
const options = ref<NrOptions>(cloneNrOptions(savedOptions.value))
watch(savedOptions, value => { options.value = cloneNrOptions(value) })
const styles = [{ title: 'A（默认）', value: 'a' }, { title: 'B', value: 'b' }, { title: 'C', value: 'c' }]
const toneFields = [
  { key: 'globalTone', label: '整体色调强度', note: '调整整体色调重建。' },
  { key: 'localTone', label: '局部色调强度', note: '调整局部明暗和色调。' },
  { key: 'localStructure', label: '局部结构强度', note: '调整纹理和边缘的重建力度。' },
] as const
const lookFields = [
  { key: 'amount', label: '整体增强幅度' }, { key: 'brighten', label: '提亮变化' },
  { key: 'darken', label: '压暗变化' }, { key: 'color', label: '颜色变化' },
  { key: 'hue', label: '色相偏移' }, { key: 'shadows', label: '阴影变化' },
  { key: 'midtones', label: '中间调变化' }, { key: 'highlights', label: '高光变化' },
] as const
const lookCaps = [{ key: 'brightenCap', label: '提亮软上限' }, { key: 'darkenCap', label: '压暗软上限' }] as const
const protectionFields = [{ key: 'hue', label: '色相保护' }, { key: 'shadows', label: '暗部保护' }, { key: 'highlights', label: '高光保护' }, { key: 'overcorrection', label: '过度修正抑制' }] as const
const diagnosticViews = [
  { title: '关闭', value: 'off' }, { title: '原图', value: 'original' }, { title: '第一遍输出', value: 'first_pass' },
  { title: '最终模型输出', value: 'model_output' }, { title: '原始变化量', value: 'raw_delta' }, { title: '受控变化量', value: 'controlled_delta' },
  { title: '低频明度变化', value: 'low_frequency' }, { title: '高频明度变化', value: 'high_frequency' },
  { title: '保护保留比例', value: 'protection' }, { title: '历史有效性', value: 'history_validity' }, { title: '实际历史权重', value: 'history_weight' },
]
const spatialFields = [{ key: 'lighting', label: '大范围光照变化' }, { key: 'detail', label: '细节变化' }] as const
const temporalFields = [
  { key: 'timeMs', label: '平滑时间', min: 1, max: 500, unit: 'ms' },
  { key: 'strength', label: '历史混合上限', min: 0, max: 90, unit: '%' },
  { key: 'rejection', label: '输入差异拒绝阈值', min: 10, max: 400, unit: '档' },
] as const
const nrDefault = computed(() => savedIntensity.value === 100 && JSON.stringify(savedOptions.value) === JSON.stringify(graphicsAdvanced().nr))
watch(savedIntensity, value => { intensity.value = value })
watch(working, value => emit('busy', value), { flush: 'sync' })
const state = computed(() => {
  if (!props.live.connected) return '尚未连接游戏'
  if (pending.value) return props.live.nr?.controlsPending ? '设置已保存，等待新的游戏画面应用' : '正在应用'
  if (!props.live.nrLiveSupported) return '本次启动未准备 NR'
  if (!props.live.fresh) return '等待游戏画面'
  if (props.live.nr?.active) return props.live.nr.pipeline?.error ? 'NR 正在运行 · 第二遍失败，已降级为一遍' : `NR 正在运行 · ${props.live.nr.pipeline?.actualPasses ?? 1} 遍`
  const reason = props.live.nr?.reason ?? 'waiting'
  return ({ disabled: 'NR 已关闭', motion_unavailable: '暂停：等待有效硬件光流', window_transition: '暂停：窗口正在变化', resource_preparation_failed: 'NR 资源准备失败', waiting: '等待游戏画面' } as Record<string, string>)[reason] ?? `NR 未运行：${reason}`
})
watch(() => props.live, value => {
  if (!pending.value) return
  if (value.connected && value.fresh && (value.nr?.appliedRevision ?? 0) >= pending.value) {
    pending.value = 0
    feedback.value = value.nr?.pipeline?.error ? `第一遍正在使用，第二遍未生效：${value.nr.pipeline.error}` : '当前游戏已应用 NR 设置。'
  } else if (value.connected && value.fresh && value.nr?.controlsPending) {
    deadline = Date.now() + 10000
    feedback.value = '当前画面重复，正在复用 NR 输出；新画面到达后应用设置。'
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
  advanced.value = false
  intensity.value = savedIntensity.value
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
async function save(on: boolean, strength: number, tuning: NrOptions = savedOptions.value) {
  if (busy.value || props.disabled || (pending.value && on) || !config.config.setting.other) return
  if (!Number.isInteger(strength) || strength < 0 || strength > 200) return
  if (on === enabled.value && strength === savedIntensity.value && JSON.stringify(tuning) === JSON.stringify(savedOptions.value)) return
  const token = generation
  const executable = props.executable
  busy.value = true
  error.value = ''
  try {
    const setting = config.config.setting
    const patch = { streamline_nr: on, streamline_nr_intensity: strength, streamline_advanced: { ...graphicsAdvanced(setting.other.streamline_advanced), nr: cloneNrOptions(tuning) } }
    await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
    Object.assign(config.config.setting.other, patch)
    if (on && props.live.connected && JSON.stringify(tuning.secondPass) !== JSON.stringify(nrSecondPass()) && !props.live.nrTwoPassSupported) {
      feedback.value = '已保存；当前组件不支持两遍 NR，请更新组件后重新专用启动。'
      return
    }
    feedback.value = '已保存；下次专用启动时使用。'
    if (token !== generation || !executable || !props.live.connected) return
    if (!props.live.nrLiveSupported) { feedback.value = '已保存；安装 NR 组件后需重新专用启动，才能在游戏内启用 NR。'; return }
    if (on && !props.live.advancedSettingsSupported && (strength > 100 || JSON.stringify(tuning) !== JSON.stringify(graphicsAdvanced().nr))) {
      feedback.value = '已保存；当前组件不支持新增参数，请更新画面增强组件后重新专用启动。'
      return
    }
    if (on && !props.live.nrLookSupported && JSON.stringify(tuning.look) !== JSON.stringify(nrLook())) {
      feedback.value = '已保存；当前组件不支持 Look，请更新画面增强组件后重新专用启动。'
      return
    }
    if (on && !props.live.nrSpatialLookSupported && JSON.stringify(tuning.look.spatial) !== JSON.stringify(nrSpatialLook())) {
      feedback.value = '已保存；当前组件不支持空间 Look，请更新组件后重新专用启动。'
      return
    }
    if (on && !props.live.nrTemporalLookSupported && JSON.stringify(tuning.look.temporal) !== JSON.stringify(nrTemporalLook())) {
      feedback.value = '已保存；当前组件不支持时间 Look，请更新组件后重新专用启动。'
      return
    }
    if (on && tuning.look.scope !== 'final_pass' && !props.live.nrLookScopeSupported) {
      feedback.value = '已保存；当前组件不支持整链 Look，请更新组件并重新专用启动。'
      return
    }
    if (on && (tuning.look.algorithm !== 'log_delta' || tuning.look.diagnostic !== 'off' || Object.values(tuning.look.protection).some(value => value !== 0)) && !props.live.nrLookExperimentsSupported) {
      throw new Error('当前组件不支持 Look 颜色保护或诊断实验，请更新组件后重新专用启动')
    }
    if (on && tuning.look.temporal.mode === 'optical_flow_plus' && !props.live.nrPersistenceSupported) {
      throw new Error('当前组件不支持光流累积＋，请更新组件后重新专用启动')
    }
    if (on && (tuning.look.temporal.mode !== 'optical_flow' || tuning.look.temporal.sampling !== 'bilinear') && !props.live.nrTemporalModesSupported) {
      feedback.value = '已保存；当前组件不支持所选时间模式或历史采样，请更新组件并重新专用启动。'
      return
    }
    const result = await liveStreamlineFg(executable, undefined, undefined, undefined, undefined, on, (on ? strength : Math.min(strength, 100)) / 100, on && props.live.advancedSettingsSupported ? { nr: tuning } : undefined)
    if (token !== generation) return
    pending.value = result.sentNrRevision ?? 0
    deadline = Date.now() + 10000
    feedback.value = '已保存，正在应用到当前游戏…'
  } catch (e) { error.value = `NR 设置操作失败：${String(e)}` }
  finally { intensity.value = savedIntensity.value; options.value = cloneNrOptions(savedOptions.value); busy.value = false }
}
function saveOptions() { return save(enabled.value, savedIntensity.value, cloneNrOptions(options.value)) }
async function saveScheduling(consolidated: boolean) {
  if (working.value || props.disabled || !config.config.setting.other) return
  if (consolidated === savedConsolidated.value) return
  const token = generation
  busy.value = true
  error.value = ''
  try {
    const setting = config.config.setting
    const patch = { streamline_nr_consolidated: consolidated }
    await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
    Object.assign(config.config.setting.other, patch)
    if (token === generation) feedback.value = '调度设置已保存；关闭游戏并重新以画面增强启动后生效。'
  } catch (e) { if (token === generation) error.value = `调度设置保存失败：${String(e)}` }
  finally { busy.value = false }
}
function bypassAdditions() {
  return save(enabled.value, savedIntensity.value, bypassNrAdditions(savedOptions.value))
}
function retrySecond() {
  options.value.secondPass.retry = (options.value.secondPass.retry + 1) % 65536
  return saveOptions()
}
function resetSecond() { options.value.secondPass = nrSecondPass(); return saveOptions() }
function linkTone(key: typeof toneFields[number]['key'], linked: boolean | null) {
  options.value[key] = linked ? null : savedIntensity.value
  void saveOptions()
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
      <h3>重建源画面 · NR（DLSS 5）</h3><v-chip
        size="x-small"
        variant="outlined"
      >
        实验版
      </v-chip>
    </div>
    <p>直接处理游戏源画面，可与抗锯齿和帧生成配合使用。依赖 NVIDIA 硬件光流，会增加 GPU 开销。</p>
    <p>DLSS 5 需要 GeForce RTX 50 系列。{{ supported ? '请在模拟器中选择支持的 NVIDIA 显卡。' : '本机未检测到支持的显卡，NR 选项不可用。' }}</p>
    <div class="nr-controls">
      <v-switch
        :model-value="supported && enabled"
        label="启用 NR"
        color="primary"
        density="compact"
        inset
        hide-details
        :loading="working"
        :disabled="disabled || working || (!!pending && !enabled) || (!enabled && (!component?.installed || !component?.packageReady))"
        @update:model-value="value => value !== null && save(value, savedIntensity)"
      />
      <v-btn
        variant="text"
        size="small"
        :prepend-icon="mdiTuneVariant"
        aria-label="NR 高级配置"
        aria-haspopup="dialog"
        :disabled="disabled || working"
        @click="advanced = true"
      >
        高级配置
      </v-btn>
    </div>
    <p class="nr-summary">
      {{ savedOptions.style.toUpperCase() }} 风格 · 强度 {{ savedIntensity }}%{{ savedOptions.autoMask ? ' · 自动人物遮罩' : '' }}
    </p>
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
      NR 组件未安装。在“高级配置”中下载恢复后即可启用。
    </p>
    <p
      class="nr-state"
      role="status"
    >
      {{ state }}<span v-if="live.connected && live.fresh && live.nr?.active && live.nr.input"> · 输入 {{ live.nr.input.join(' × ') }} · 实际强度 {{ Math.round((live.nr.appliedIntensity ?? live.nr.intensity ?? 1) * 100) }}%{{ live.nr.modelRecomputed ? (live.nr.firstEvaluated ? ' · 模型链已重算' : ' · 第二遍已重算，复用第一遍') : live.nr.lookRecomputed ? ' · Look 已重新合成' : live.nr.outputReused ? ' · 复用上一张输出' : '' }}</span>
    </p>
    <p
      v-if="error && !advanced"
      class="nr-error"
      role="alert"
    >
      {{ error }}
    </p>
    <p
      v-if="live.connected && live.fresh && live.nr?.active && live.nr.pipeline?.inferenceExtent"
      class="nr-state"
    >
      当前推理 {{ live.nr.pipeline.inferenceExtent.join(' × ') }}{{ live.nr.pipeline.inferenceMaxEdge ? ` · 长边上限 ${live.nr.pipeline.inferenceMaxEdge} 像素` : live.nr.pipeline.inferenceScalePercent ? ` · 每边 ${live.nr.pipeline.inferenceScalePercent}%` : '' }} · {{ live.nr.pipeline.scheduling === 'consolidated_experimental' ? '集中提交' : '逐遍提交' }}
    </p>
    <GraphicsAdvancedDialog
      v-model="advanced"
      title="NR 高级配置"
      title-id="nr-advanced-title"
      description="设置自动保存。重建与 Look 参数可在专用启动后实时调整；输入缩放在主面板单独设置。"
      :busy="working"
    >
      <v-select
        v-model="options.style"
        :items="styles"
        label="重建风格"
        variant="outlined"
        :disabled="disabled || working || !!pending"
        hint="A / B / C 使用不同的重建风格，可在同一场景对比。"
        persistent-hint
        @update:model-value="saveOptions"
      />
      <div class="nr-strength-heading nr-overall-strength">
        <label id="nr-strength-label">重建强度</label><output class="nr-strength">{{ intensity }}%</output>
      </div>
      <v-slider
        v-model="intensity"
        min="0"
        max="200"
        step="1"
        color="primary"
        thumb-label
        hide-details
        aria-labelledby="nr-strength-label"
        :disabled="disabled || working || !!pending || !config.config.setting.other"
        @end="save(enabled, Math.round(intensity))"
        @keyup="save(enabled, Math.round(intensity))"
      />
      <div class="nr-strength-range">
        <span>0% · 最低强度</span><span>200% · 增强强度</span>
      </div>
      <p class="nr-dialog-note">
        设置 NR 的处理强度。调低数值可减弱重建效果，GPU 开销不会按比例降低；要停用处理，请关闭主面板的 NR 开关。
      </p>
      <div
        v-for="field in toneFields"
        :key="field.key"
        class="nr-parameter"
      >
        <div class="nr-strength-heading">
          <label :id="`nr-${field.key}-label`">{{ field.label }}</label>
          <output>{{ options[field.key] ?? savedIntensity }}%</output>
        </div>
        <v-slider
          :model-value="options[field.key] ?? savedIntensity"
          :min="0"
          :max="200"
          :step="1"
          color="primary"
          thumb-label
          hide-details
          :aria-labelledby="`nr-${field.key}-label`"
          :disabled="disabled || working || !!pending || options[field.key] === null"
          @update:model-value="value => options[field.key] = value"
          @end="saveOptions"
          @keyup="saveOptions"
        />
        <v-switch
          :model-value="options[field.key] === null"
          :label="`${field.label}跟随重建强度`"
          color="primary"
          density="compact"
          hide-details
          :disabled="disabled || working || !!pending"
          @update:model-value="value => linkTone(field.key, value)"
        />
        <p class="nr-dialog-note">
          {{ field.note }}关闭“跟随”后可独立调整，100% 为标准强度。
        </p>
      </div>
      <div class="nr-parameter">
        <div class="nr-strength-heading">
          <label id="nr-skin-label">皮肤结构强度</label><output>{{ options.skinStructure }}%</output>
        </div>
        <v-slider
          v-model="options.skinStructure"
          :min="0"
          :max="200"
          :step="1"
          color="primary"
          thumb-label
          hide-details
          aria-labelledby="nr-skin-label"
          :disabled="disabled || working || !!pending"
          @end="saveOptions"
          @keyup="saveOptions"
        />
        <v-switch
          v-model="options.autoMask"
          label="自动人物遮罩"
          color="primary"
          density="comfortable"
          hide-details
          inset
          :disabled="disabled || working || !!pending"
          @update:model-value="saveOptions"
        />
        <p class="nr-dialog-note">
          由 NR 模型识别人物区域，可配合皮肤结构强度对比细节。皮肤结构默认 0%，自动遮罩默认关闭。
        </p>
      </div>
      <p
        v-if="live.connected && !live.advancedSettingsSupported"
        class="nr-dialog-note"
      >
        当前组件不支持新增参数。设置可先保存，更新画面增强组件并重新专用启动后使用。
      </p>
      <p class="nr-dialog-note">
        NR 会话始终启用 NVIDIA 硬件光流，用于估算画面运动。
      </p>
      <div class="nr-parameter">
        <h3>第二遍 NR</h3>
        <p class="nr-dialog-note">
          再次处理第一遍输出，会增加 GPU 开销。默认关闭；失败时保留第一遍。Look 只处理最终成功一遍的变化。
        </p>
        <v-switch
          v-model="options.secondPass.enabled"
          label="启用第二遍 NR"
          color="primary"
          hide-details
          :disabled="disabled || working || !!pending"
          @update:model-value="saveOptions"
        />
        <v-switch
          v-if="options.secondPass.enabled"
          v-model="options.secondPass.inherit"
          label="第二遍继承第一遍参数"
          color="primary"
          hide-details
          :disabled="disabled || working || !!pending"
          @update:model-value="saveOptions"
        />
        <template v-if="options.secondPass.enabled && !options.secondPass.inherit">
          <v-select
            v-model="options.secondPass.style"
            :items="styles"
            label="第二遍重建风格"
            variant="outlined"
            :disabled="disabled || working || !!pending"
            @update:model-value="saveOptions"
          />
          <div class="nr-strength-heading">
            <label id="nr-second-strength">第二遍强度</label><output>{{ options.secondPass.intensity }}%</output>
          </div>
          <v-slider
            v-model="options.secondPass.intensity"
            :min="0"
            :max="200"
            :step="1"
            aria-labelledby="nr-second-strength"
            hide-details
            :disabled="disabled || working || !!pending"
            @end="saveOptions"
            @keyup="saveOptions"
          />
          <div
            v-for="field in toneFields"
            :key="`second-${field.key}`"
            class="nr-parameter"
          >
            <div class="nr-strength-heading">
              <label :id="`nr-second-${field.key}`">第二遍{{ field.label }}</label><output>{{ options.secondPass[field.key] ?? options.secondPass.intensity }}%</output>
            </div>
            <v-slider
              :model-value="options.secondPass[field.key] ?? options.secondPass.intensity"
              :min="0"
              :max="200"
              :step="1"
              :aria-labelledby="`nr-second-${field.key}`"
              hide-details
              :disabled="disabled || working || !!pending || options.secondPass[field.key] === null"
              @update:model-value="value => options.secondPass[field.key] = value"
              @end="saveOptions"
              @keyup="saveOptions"
            />
            <v-switch
              :model-value="options.secondPass[field.key] === null"
              :label="`${field.label}跟随第二遍强度`"
              hide-details
              :disabled="disabled || working || !!pending"
              @update:model-value="value => { options.secondPass[field.key] = value ? null : options.secondPass.intensity; saveOptions() }"
            />
          </div>
          <div class="nr-strength-heading">
            <label id="nr-second-skin">第二遍皮肤结构强度</label><output>{{ options.secondPass.skinStructure }}%</output>
          </div>
          <v-slider
            v-model="options.secondPass.skinStructure"
            :min="0"
            :max="200"
            :step="1"
            aria-labelledby="nr-second-skin"
            hide-details
            :disabled="disabled || working || !!pending"
            @end="saveOptions"
            @keyup="saveOptions"
          />
          <v-switch
            v-model="options.secondPass.autoMask"
            label="第二遍自动人物遮罩"
            hide-details
            :disabled="disabled || working || !!pending"
            @update:model-value="saveOptions"
          />
        </template>
        <p
          v-if="live.nr?.pipeline?.error"
          class="nr-dialog-note"
          role="status"
        >
          第二遍未生效：{{ live.nr.pipeline.error }}
        </p>
        <v-btn
          v-if="options.secondPass.enabled && live.connected && live.nrTwoPassSupported && live.nr?.pipeline?.error"
          variant="text"
          :disabled="disabled || working || !!pending"
          @click="retrySecond"
        >
          重试第二遍
        </v-btn>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!pending"
          @click="resetSecond"
        >
          恢复第二遍默认配置
        </v-btn>
      </div>
      <div class="nr-parameter">
        <h3>Look · 控制模型变化</h3>
        <details>
          <summary>进阶颜色保护与诊断</summary>
          <v-select
            v-model="options.look.algorithm"
            label="颜色算法"
            :items="[{ title: 'Log 变化量（默认）', value: 'log_delta' }, { title: 'Oklab 色度分离（实验）', value: 'oklab' }]"
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @update:model-value="saveOptions"
          />
          <div
            v-for="field in protectionFields"
            :key="field.key"
            class="nr-parameter"
          >
            <div class="nr-strength-heading">
              <label :id="`nr-protection-${field.key}`">{{ field.label }}</label><output>{{ options.look.protection[field.key] }}%</output>
            </div>
            <v-slider
              v-model="options.look.protection[field.key]"
              :min="0"
              :max="100"
              :step="1"
              hide-details
              :aria-labelledby="`nr-protection-${field.key}`"
              :disabled="disabled || working || !!pending || !options.look.enabled"
              @end="saveOptions"
              @keyup="saveOptions"
            />
          </div>
          <v-select
            v-model="options.look.diagnostic"
            label="诊断视图"
            :items="diagnosticViews"
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @update:model-value="saveOptions"
          />
          <p class="nr-dialog-note">
            暗部/高光按范围基底的线性明度判断，不识别场景物体。过度修正抑制随变化的 stop 幅度增加。
            Oklab 只改变色度分离，提亮/压暗和软上限仍使用 stops；中性和旁路保留原模型结果。
            变化量视图以灰色表示 0，±4 stops 映射至黑/白。保护视图 RGB 分别为色相、明度区域和过度修正的保留比例。
            历史权重显示逐像素实际值，不是全局上限。诊断只改变显示，下游历史在切换时重置；测性能时关闭诊断和读回。
          </p>
        </details>
        <p class="nr-dialog-note">
          在 NR 输出后调整所选范围的变化量。100% 保留对应变化，0% 抑制对应变化。
          最后一遍范围的整体幅度 0% 保留第一遍 RGB；整链范围的 0% 返回原图 RGB。alpha 沿用最终模型输出。
          所有增益为 100%、软上限为 0 时直接保留 NR 输出。当前支持 SDR。
        </p>
        <v-switch
          v-model="options.look.enabled"
          label="启用 Look 调整"
          color="primary"
          hide-details
          inset
          :disabled="disabled || working || !!pending"
          @update:model-value="saveOptions"
        />
        <v-select
          v-model="options.look.scope"
          label="Look 作用范围"
          :items="[{ title: '最后一遍（兼容旧配置）', value: 'final_pass' }, { title: '整条 NR 链', value: 'chain_total' }]"
          :disabled="disabled || working || !!pending"
          @update:model-value="saveOptions"
        />
        <div
          v-for="field in lookFields"
          :key="field.key"
          class="nr-parameter"
        >
          <div class="nr-strength-heading">
            <label :id="`nr-look-${field.key}`">{{ field.label }}</label>
            <output>{{ options.look[field.key] }}%</output>
          </div>
          <v-slider
            v-model="options.look[field.key]"
            :min="0"
            :max="200"
            :step="1"
            color="primary"
            hide-details
            :aria-labelledby="`nr-look-${field.key}`"
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @end="saveOptions"
            @keyup="saveOptions"
          />
        </div>
        <div
          v-for="field in lookCaps"
          :key="field.key"
          class="nr-parameter"
        >
          <div class="nr-strength-heading">
            <label :id="`nr-look-${field.key}`">{{ field.label }}</label>
            <output>{{ options.look[field.key] === 0 ? '不限制' : `${(options.look[field.key] / 100).toFixed(2)} 档` }}</output>
          </div>
          <v-slider
            v-model="options.look[field.key]"
            :min="0"
            :max="1600"
            :step="25"
            color="primary"
            hide-details
            :aria-labelledby="`nr-look-${field.key}`"
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @end="saveOptions"
            @keyup="saveOptions"
          />
        </div>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!pending"
          @click="options.look = nrLook(); saveOptions()"
        >
          恢复 Look 中性配置
        </v-btn>
        <div class="nr-parameter">
          <h3>时间 Look</h3>
          <p class="nr-dialog-note">
            平滑模型变化量以减轻闪烁，会增加显存和 GPU 开销。使用光流估算运动，并在输入不一致时拒绝历史；可能产生拖影，默认关闭。
          </p>
          <v-switch
            v-model="options.look.temporal.enabled"
            label="启用时间平滑"
            color="primary"
            hide-details
            inset
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @update:model-value="saveOptions"
          />
          <v-select
            v-model="options.look.temporal.mode"
            label="时间累积模式"
            :items="[{ title: '光流累积（原有路径）', value: 'optical_flow' }, { title: '静态累积（固定镜头）', value: 'static' }, { title: '光流累积＋（实验）', value: 'optical_flow_plus' }]"
            :disabled="disabled || working || !!pending || !options.look.temporal.enabled"
            @update:model-value="saveOptions"
          />
          <v-select
            v-model="options.look.temporal.sampling"
            label="历史颜色验证"
            :items="[{ title: '双线性后验证（原有路径）', value: 'bilinear' }, { title: '逐采样点验证（实验）', value: 'per_tap' }]"
            :disabled="disabled || working || !!pending || !options.look.temporal.enabled"
            @update:model-value="saveOptions"
          />
          <p class="nr-dialog-note">
            静态模式不读取运动，只累积原图局部颜色与边缘稳定的区域，不用于解决运动闪烁。
            光流失效时保留原有旁路；时间处理始终用未经 NR 修改的原图验证历史。
            光流累积＋分开保留修正的存在支持与幅度，实验出现/消退时间为 60/180 ms，平滑时间控制条件幅度；可能改变时间均值。
          </p>
          <div
            v-for="field in temporalFields"
            :key="field.key"
            class="nr-parameter"
          >
            <div class="nr-strength-heading">
              <label :id="`nr-temporal-${field.key}`">{{ field.label }}</label><output>{{ field.key === 'rejection' ? (options.look.temporal.rejection / 100).toFixed(2) : options.look.temporal[field.key] }} {{ field.unit }}</output>
            </div>
            <v-slider
              v-model="options.look.temporal[field.key]"
              :min="field.min"
              :max="field.max"
              :step="1"
              color="primary"
              hide-details
              :aria-labelledby="`nr-temporal-${field.key}`"
              :disabled="disabled || working || !!pending || !options.look.enabled || !options.look.temporal.enabled"
              @end="saveOptions"
              @keyup="saveOptions"
            />
          </div>
          <p class="nr-dialog-note">
            平滑时间按相邻源画面的实际观察间隔计算。拒绝阈值调低更容易丢弃历史；暂停恢复、窗口变化和较长间隔会重新开始历史。
          </p>
          <v-btn
            variant="text"
            :disabled="disabled || working || !!pending"
            @click="options.look.temporal = nrTemporalLook(); saveOptions()"
          >
            恢复时间平滑默认配置
          </v-btn>
          <p
            v-if="live.connected && !live.nrTemporalLookSupported"
            class="nr-dialog-note"
          >
            当前会话不支持时间 Look。更新组件并重新专用启动后使用。
          </p>
          <p
            v-if="live.nr?.look?.temporal?.error"
            class="nr-error"
            role="alert"
          >
            时间平滑未生效，保留空间或基础 Look：{{ live.nr.look.temporal.error }}。调整窗口尺寸或重新启动游戏后重试。
          </p>
        </div>
        <div class="nr-parameter">
          <h3>空间 Look</h3>
          <p class="nr-dialog-note">
            将模型明暗变化分成大范围光照和细节，分别调整。空间处理会增加 GPU 开销，默认关闭。
            光晕抑制仅尝试减弱亮侧轮廓的局部压暗变化，建议从低强度开始对比真实阴影和文字。
          </p>
          <v-switch
            v-model="options.look.spatial.enabled"
            label="启用空间 Look"
            color="primary"
            hide-details
            inset
            :disabled="disabled || working || !!pending || !options.look.enabled"
            @update:model-value="saveOptions"
          />
          <div
            v-for="field in spatialFields"
            :key="field.key"
            class="nr-parameter"
          >
            <div class="nr-strength-heading">
              <label :id="`nr-spatial-${field.key}`">{{ field.label }}</label>
              <output>{{ options.look.spatial[field.key] }}%</output>
            </div>
            <v-slider
              v-model="options.look.spatial[field.key]"
              :min="0"
              :max="200"
              :step="1"
              color="primary"
              hide-details
              :aria-labelledby="`nr-spatial-${field.key}`"
              :disabled="disabled || working || !!pending || !options.look.enabled || !options.look.spatial.enabled"
              @end="saveOptions"
              @keyup="saveOptions"
            />
          </div>
          <div class="nr-parameter">
            <div class="nr-strength-heading">
              <label id="nr-spatial-radius">分离半径</label>
              <output>{{ options.look.spatial.radius }} 像素</output>
            </div>
            <v-slider
              v-model="options.look.spatial.radius"
              :min="1"
              :max="32"
              :step="1"
              color="primary"
              hide-details
              aria-labelledby="nr-spatial-radius"
              :disabled="disabled || working || !!pending || !options.look.enabled || !options.look.spatial.enabled"
              @end="saveOptions"
              @keyup="saveOptions"
            />
            <p class="nr-dialog-note">
              半径按 NR 输入像素解释。增大可分离更大范围的光照，细小轮廓建议使用较小半径。
            </p>
          </div>
          <div class="nr-parameter">
            <div class="nr-strength-heading">
              <label id="nr-spatial-halo">光晕抑制</label>
              <output>{{ options.look.spatial.halo }}%</output>
            </div>
            <v-slider
              v-model="options.look.spatial.halo"
              :min="0"
              :max="100"
              :step="1"
              color="primary"
              hide-details
              aria-labelledby="nr-spatial-halo"
              :disabled="disabled || working || !!pending || !options.look.enabled || !options.look.spatial.enabled"
              @end="saveOptions"
              @keyup="saveOptions"
            />
          </div>
          <v-btn
            variant="text"
            :disabled="disabled || working || !!pending"
            @click="options.look.spatial = nrSpatialLook(); saveOptions()"
          >
            恢复空间 Look 默认配置
          </v-btn>
          <p
            v-if="live.connected && !live.nrSpatialLookSupported"
            class="nr-dialog-note"
          >
            当前会话不支持空间 Look。更新组件并重新专用启动后使用。
          </p>
          <p
            v-if="live.nr?.look?.spatial?.error"
            class="nr-error"
            role="alert"
          >
            空间处理未生效，保留基础 Look：{{ live.nr.look.spatial.error }}。调整窗口尺寸或重新启动游戏后重试。
          </p>
        </div>
        <p
          v-if="live.connected && !live.nrLookSupported"
          class="nr-dialog-note"
        >
          当前会话不支持 Look。设置可先保存，更新画面增强组件并重新专用启动后使用。
        </p>
        <p
          v-if="live.nr?.look?.error"
          class="nr-error"
          role="alert"
        >
          Look 未生效，当前保留 NR 输出：{{ live.nr.look.error }}。调整窗口尺寸或重新启动游戏后重试。
        </p>
      </div>
      <p
        class="nr-feedback"
        role="status"
      >
        {{ feedback }}
      </p>
      <p
        v-if="error"
        class="nr-error"
        role="alert"
      >
        {{ error }}
      </p>
      <details class="nr-maintenance">
        <summary>提交方式实验 · {{ savedConsolidated ? '集中提交' : '逐遍提交' }}</summary>
        <p class="nr-dialog-note">
          需要关闭游戏并重新以画面增强启动后生效。默认逐遍提交；实验需要新版画面增强组件。
        </p>
        <v-switch
          :model-value="savedConsolidated"
          label="集中提交多遍 NR"
          color="primary"
          inset
          hide-details
          :disabled="disabled || working"
          @update:model-value="value => value !== null && saveScheduling(value)"
        />
        <p class="nr-dialog-note">
          减少两遍之间的 CPU 等待；仍保留独立模型实例。实际收益取决于场景和 GPU。
        </p>
        <v-btn
          size="small"
          variant="text"
          :disabled="disabled || working || !savedConsolidated"
          @click="saveScheduling(false)"
        >
          恢复逐遍提交
        </v-btn>
      </details>
      <NrPresetControls
        :executable="executable"
        :blocked="disabled || working || !!pending"
        :live="live"
        :enabled="enabled"
        :intensity="savedIntensity"
        :options="savedOptions"
        @busy="presetBusy = $event"
        @apply="preset => save(preset.settings.enabled, preset.settings.intensity, preset.settings.options)"
      />
      <details class="nr-maintenance">
        <summary>NR 组件维护{{ component?.installed ? ' · 已安装' : ' · 未就绪' }}</summary>
        <div class="nr-actions">
          <v-btn
            size="small"
            variant="outlined"
            :loading="working"
            :disabled="disabled || working || !component?.supported || !component?.packageReady"
            @click="installRuntime"
          >
            {{ component?.installed ? '重新安装 NR 组件' : '下载恢复 NR 组件' }}
          </v-btn>
          <v-btn
            v-if="component?.installed"
            size="small"
            variant="text"
            :disabled="disabled || working"
            @click="removeRuntime"
          >
            卸载 NR 运行库
          </v-btn>
          <v-btn
            size="small"
            variant="text"
            :disabled="disabled || working || !executable"
            @click="inspect"
          >
            重新检查
          </v-btn>
        </div>
        <p class="nr-component">
          {{ component?.message ?? (working ? '正在检查 NR 组件…' : error ? 'NR 组件检查失败，请重新检查。' : executable ? '等待自动检查 NR 组件…' : '选择主程序后自动检查 NR 组件。') }}
        </p>
        <p
          v-if="component?.installed"
          class="nr-component"
        >
          运行库 {{ component.runtimeVersion }} · {{ component.architecture }} · SHA-256 {{ component.runtimeSha256?.slice(0, 12) }}
        </p>
      </details>
      <template #actions>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!pending || (!savedOptions.secondPass.enabled && !savedOptions.look.enabled)"
          @click="bypassAdditions"
        >
          一遍 NR · 绕过 Look
        </v-btn>
        <v-btn
          variant="text"
          :disabled="disabled || working || !!pending || nrDefault"
          @click="save(enabled, 100, graphicsAdvanced().nr)"
        >
          恢复重建与 Look 默认配置
        </v-btn>
      </template>
    </GraphicsAdvancedDialog>
  </div>
</template>

<style scoped>
.nr-setting { padding: 20px 0; }
.nr-heading { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 8px; }
.nr-heading h3 { font-size: 15px; font-weight: 600; }
.nr-controls { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.nr-controls .v-switch { flex: 1 1 180px; }
.nr-controls .v-btn { flex-shrink: 0; }
.nr-summary { font-variant-numeric: tabular-nums; }
.nr-setting p { font-size: .85rem; line-height: 1.6; margin-bottom: 8px; }
.nr-feedback, .nr-component { color: rgba(var(--v-theme-on-surface), .72); overflow-wrap: anywhere; }
.nr-strength { min-width: 42px; font-variant-numeric: tabular-nums; font-size: .85rem; }
.nr-strength-heading, .nr-strength-range { display: flex; justify-content: space-between; gap: 12px; }
.nr-strength-heading { margin-bottom: 12px; font-weight: 600; }
.nr-overall-strength { margin-top: 24px; }
.nr-parameter { margin-top: 22px; padding-top: 20px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.nr-strength-range, .nr-dialog-note { font-size: 12px; color: rgba(var(--v-theme-on-surface), .72); }
.nr-dialog-note { line-height: 1.7; margin: 14px 0; }
.nr-maintenance { margin-top: 20px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); padding-top: 10px; }
.nr-actions { display: flex; gap: 8px; flex-wrap: wrap; margin: 12px 0 8px; }
.nr-state { font-weight: 500; }
.nr-maintenance summary { cursor: pointer; font-size: 12px; padding: 8px 0; color: rgba(var(--v-theme-on-surface), .72); }
.nr-maintenance summary:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 3px; }
.nr-error { color: rgb(var(--v-theme-error)); }
</style>
