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
watch(busy, value => emit('busy', value), { flush: 'sync' })
const validCap = (value: number) => Number.isInteger(value) && value >= 320 && value <= 8192
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
  finally { busy.value = false }
}
</script>

<template>
  <InputSizingControls
    :scale-percent="savedScale"
    :max-edge="savedCap"
    :disabled="disabled || busy"
    description="先缩小游戏画面输入，再交给 NR／SR 处理。关闭 NR 时也可使用；FG 接收最终窗口尺寸的画面。"
    :feedback="feedback"
    :error="error"
    :runtime="live.connected && live.fresh ? live.inputScale : undefined"
    @save="save"
  />
</template>
