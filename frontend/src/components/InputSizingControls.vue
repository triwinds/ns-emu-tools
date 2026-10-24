<script setup lang="ts">
import { ref, watch, useId } from 'vue'
interface RuntimeSizing { inputExtent?: number[]; originalExtent?: number[]; outputExtent?: number[]; active?: boolean; error?: unknown }
const props = defineProps<{ scalePercent: number; maxEdge: number; disabled?: boolean; description: string; feedback: string; error?: string; runtime?: RuntimeSizing }>()
const emit = defineEmits<{ save: [percent: number, maxEdge: number] }>()
const uid = useId()
const scale = ref(props.scalePercent)
const cap = ref(props.maxEdge || 1920)
watch(() => props.scalePercent, value => { scale.value = value })
watch(() => props.maxEdge, value => { cap.value = value || 1920 })
const modes = [{ title: '按输入比例', value: 'percentage' }, { title: '固定长边上限', value: 'max_edge' }]
const validCap = (value: number) => Number.isInteger(value) && value >= 320 && value <= 8192
const capRules = [(value: string | number) => validCap(Number(value)) || '请输入 320～8192 的整数像素值']
function save(percent: number, maxEdge: number) {
  if (!props.disabled && Number.isInteger(percent) && percent >= 50 && percent <= 100 && (maxEdge === 0 || validCap(maxEdge))) emit('save', percent, maxEdge)
}
function changeMode(mode: string | null) {
  if (mode === 'percentage') save(props.scalePercent, 0)
  else if (mode === 'max_edge') save(props.scalePercent, Number(cap.value))
}
function saveCap() { save(props.scalePercent, Number(cap.value)) }
</script>

<template>
  <section
    class="input-scale"
    :aria-labelledby="`${uid}-title`"
  >
    <h3 :id="`${uid}-title`">
      输入尺寸缩放
    </h3>
    <p>{{ description }}</p>
    <v-select
      :model-value="maxEdge ? 'max_edge' : 'percentage'"
      :items="modes"
      label="尺寸方式"
      variant="outlined"
      density="compact"
      :disabled="disabled"
      @update:model-value="changeMode"
    />
    <v-text-field
      v-if="maxEdge"
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
      :disabled="disabled"
      hint="超过上限时等比缩小；较小输入不放大。例如 2560 × 1325、上限 1920，输入为 1920 × 993。"
      persistent-hint
      @blur="saveCap"
      @keyup.enter="saveCap"
    />
    <template v-else>
      <div class="input-scale-label">
        <label :id="`${uid}-percent`">每边比例</label><output>{{ scale }}%</output>
      </div>
      <v-slider
        v-model="scale"
        :min="50"
        :max="100"
        :step="1"
        color="primary"
        thumb-label
        hide-details
        :aria-labelledby="`${uid}-percent`"
        :disabled="disabled"
        @end="save(Math.round(Number(scale)), 0)"
        @keyup="save(Math.round(Number(scale)), 0)"
      />
    </template>
    <p>仅支持 SDR，缩小后的输入至少为 320 × 180。输出保持窗口尺寸；文字和细线也会缩放。100% 且不设上限时保持原输入。</p>
    <p role="status">
      {{ feedback }}
    </p>
    <p
      v-if="runtime?.inputExtent"
      role="status"
    >
      当前输入 {{ runtime.originalExtent?.join(' × ') }} → {{ runtime.inputExtent.join(' × ') }} · 输出 {{ runtime.outputExtent?.join(' × ') }}{{ runtime.active ? '' : ' · 未缩放' }}
    </p>
    <p
      v-if="error || runtime?.error"
      class="input-error"
      role="alert"
    >
      {{ error || runtime?.error }}
    </p>
    <v-btn
      variant="text"
      size="small"
      :disabled="disabled"
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
