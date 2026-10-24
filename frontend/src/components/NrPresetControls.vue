<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { save as saveDialog } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import { useConfigStore } from '@/stores/ConfigStore'
import { updateSetting } from '@/utils/tauri'
import type { NrOptions } from '@/utils/graphicsAdvanced'
import type { FgLive } from '@/utils/streamlineFg'
import { normalizeNrPreset, nrPresetEnvironment, presetNotices, validateNrPreset, type NrPreset, type NrPresetEmulator, type NrPresetEnvironment } from '@/utils/nrPresets'

const props = defineProps<{ executable: string; blocked: boolean; live: FgLive; enabled: boolean; intensity: number; options: NrOptions }>()
const emit = defineEmits<{ apply: [preset: NrPreset]; busy: [value: boolean] }>()
const config = useConfigStore()
const busy = ref(false)
const error = ref('')
const status = ref('')
const name = ref('')
const emulator = ref<NrPresetEmulator>('other')
const game = ref('')
const displayMode = ref('')
const selected = ref<number | null>(null)
const preview = ref<NrPreset | null>(null)
const environment = ref<NrPresetEnvironment | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)
let generation = 0
const library = computed(() => config.config.setting.other?.streamline_nr_presets ?? [])
const items = computed(() => library.value.map((preset, index) => ({ value: index, title: `${preset.name} · ${preset.emulator} · ${preset.game || '通用'} · ${preset.displayMode || '未标记显示模式'}` })))
const notes = computed(() => preview.value ? presetNotices(preview.value, environment.value, props.live) : [])
const emulators = ['eden', 'citron', 'yuzu', 'ryujinx', 'other']
watch(busy, value => emit('busy', value), { flush: 'sync' })
watch(() => props.executable, value => {
  generation++
  preview.value = null
  selected.value = null
  environment.value = null
  error.value = ''
  status.value = ''
  const filename = value.split(/[\\/]/).pop()?.toLowerCase() ?? ''
  emulator.value = emulators.find(item => filename.startsWith(item)) as NrPresetEmulator ?? 'other'
}, { immediate: true })
onBeforeUnmount(() => { generation++ })

async function run(action: (token: number) => Promise<void>) {
  if (busy.value || props.blocked) return
  busy.value = true
  error.value = ''
  const token = generation
  try { await action(token) }
  catch (e) { if (token === generation) error.value = String(e) }
  finally { busy.value = false }
}
async function persist(presets: NrPreset[]) {
  const setting = config.config.setting
  await updateSetting({ ...setting, other: { ...setting.other, streamline_nr_presets: presets } })
  config.config.setting.other.streamline_nr_presets = presets
}
function saveCurrent() {
  return run(async token => {
    if (library.value.length >= 64) throw new Error('最多保存 64 个预设，请先删除不再使用的预设。')
    const recorded = await nrPresetEnvironment(props.executable)
    if (token !== generation) return
    const result = await validateNrPreset(JSON.stringify({ schemaVersion: 4, name: name.value.trim(), emulator: emulator.value, game: game.value.trim(), displayMode: displayMode.value.trim(), settings: { enabled: props.enabled, intensity: props.intensity, options: props.options }, environment: recorded }))
    if (token !== generation) return
    const preset = normalizeNrPreset(result.preset)
    await persist([...library.value, preset])
    if (token !== generation) return
    environment.value = recorded
    selected.value = library.value.length - 1
    preview.value = preset
    status.value = '当前 NR 配置已保存到预设库。'
  })
}
function selectPreset(index: number | null) {
  selected.value = index
  preview.value = index !== null && library.value[index] ? normalizeNrPreset(library.value[index]) : null
  status.value = ''
  if (preview.value) void run(async token => {
    const recorded = await nrPresetEnvironment(props.executable)
    if (token === generation) environment.value = recorded
  })
}
function importFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  void run(async token => {
    if (file.size > 64 * 1024) throw new Error('NR 预设文件不能超过 64 KiB。')
    const result = await validateNrPreset(await file.text())
    const recorded = await nrPresetEnvironment(props.executable)
    if (token !== generation) return
    preview.value = normalizeNrPreset(result.preset)
    selected.value = null
    environment.value = recorded
    status.value = result.migratedFrom ? `已从 v${result.migratedFrom} 迁移到 v2；显式参数已保留。导入尚未应用。` : '校验通过，导入尚未保存或应用。'
  })
}
function saveImported() {
  return run(async token => {
    if (!preview.value || library.value.length >= 64) throw new Error('最多保存 64 个预设。')
    const result = await validateNrPreset(JSON.stringify(preview.value))
    if (token !== generation) return
    await persist([...library.value, normalizeNrPreset(result.preset)])
    if (token !== generation) return
    selected.value = library.value.length - 1
    status.value = '导入预设已保存，尚未应用。'
  })
}
function exportPreset() {
  return run(async token => {
    if (!preview.value) return
    const result = await validateNrPreset(JSON.stringify(preview.value))
    const path = await saveDialog({ title: '导出 NR 预设', defaultPath: 'nr-preset.json', filters: [{ name: 'NR 预设 JSON', extensions: ['json'] }] })
    if (!path || token !== generation) return
    await writeFile(path, new TextEncoder().encode(JSON.stringify(result.preset, null, 2)))
    if (token === generation) status.value = 'NR 预设已导出。'
  })
}
function removePreset() {
  return run(async token => {
    if (selected.value === null) return
    const index = selected.value
    await persist(library.value.filter((_, i) => i !== index))
    if (token !== generation) return
    selected.value = null
    preview.value = null
    status.value = '已删除此预设，当前 NR 设置保持不变。'
  })
}
</script>

<template>
  <details class="nr-presets">
    <summary>NR 预设 · {{ library.length }} / 64</summary>
    <p>保存当前 NR 参数，按模拟器、游戏和显示模式标记。应用会更新全局 NR 设置；不会自动切换游戏或显示模式。</p>
    <v-text-field
      v-model="name"
      label="新预设名称"
      :maxlength="160"
      :disabled="blocked || busy"
    />
    <v-select
      v-model="emulator"
      label="模拟器"
      :items="emulators"
      :disabled="blocked || busy"
    />
    <v-text-field
      v-model="game"
      label="游戏名称 / Title ID（可选）"
      :maxlength="160"
      :disabled="blocked || busy"
    />
    <v-text-field
      v-model="displayMode"
      label="显示模式 / 分辨率（可选，如 SDR 1440p）"
      :maxlength="160"
      :disabled="blocked || busy"
    />
    <div class="preset-actions">
      <v-btn
        size="small"
        variant="outlined"
        :disabled="blocked || busy || !name.trim() || library.length >= 64"
        @click="saveCurrent"
      >
        保存当前配置
      </v-btn>
      <v-btn
        size="small"
        variant="text"
        :disabled="blocked || busy"
        @click="fileInput?.click()"
      >
        导入 JSON
      </v-btn>
      <input
        ref="fileInput"
        type="file"
        accept=".json,application/json"
        hidden
        aria-label="导入 NR 预设"
        @change="importFile"
      >
    </div>
    <v-select
      :model-value="selected"
      label="预设库"
      :items="items"
      :disabled="blocked || busy"
      clearable
      @update:model-value="selectPreset"
    />
    <div
      v-if="preview"
      class="preset-preview"
    >
      <strong>{{ preview.name }} · {{ preview.emulator }}</strong>
      <p>{{ preview.game || '通用' }} · {{ preview.displayMode || '未标记显示模式' }}<br>NR {{ preview.settings.enabled ? '开启' : '关闭' }} · 强度 {{ preview.settings.intensity }}% · {{ preview.settings.options.secondPass.enabled ? 2 : 1 }} 遍 · Look {{ preview.settings.options.look.enabled ? '开启' : '绕过' }} · 时间平滑 {{ preview.settings.options.look.temporal.enabled ? '开启' : '关闭' }}</p>
      <p>保存时已安装组件：{{ preview.environment.componentVersion || '未知' }}<br>NR 模型：{{ preview.environment.modelVersion || '未知' }} · {{ preview.environment.modelSha256?.slice(0, 12) || '无哈希' }}<br>工具：{{ preview.environment.toolboxVersion || '未知' }}</p>
      <p>记录描述保存时的安装环境；当前游戏可能仍在使用旧副本。SDR Look 的实际状态以游戏连接反馈为准。</p>
      <details>
        <summary>查看完整参数与版本记录</summary>
        <pre>{{ JSON.stringify(preview, null, 2) }}</pre>
      </details>
      <p
        v-for="note in notes"
        :key="note"
      >
        {{ note }}
      </p>
      <div class="preset-actions">
        <v-btn
          size="small"
          color="primary"
          :disabled="blocked || busy"
          @click="emit('apply', normalizeNrPreset(preview))"
        >
          应用到 NR
        </v-btn>
        <v-btn
          v-if="selected === null"
          size="small"
          variant="outlined"
          :disabled="blocked || busy || library.length >= 64"
          @click="saveImported"
        >
          保存到预设库
        </v-btn>
        <v-btn
          size="small"
          variant="text"
          :disabled="blocked || busy"
          @click="exportPreset"
        >
          导出 JSON
        </v-btn>
        <v-btn
          v-if="selected !== null"
          size="small"
          variant="text"
          :disabled="blocked || busy"
          @click="removePreset"
        >
          删除此预设
        </v-btn>
      </div>
    </div>
    <p
      v-if="status"
      role="status"
    >
      {{ status }}
    </p>
    <p
      v-if="error"
      role="alert"
      class="preset-error"
    >
      {{ error }}
    </p>
  </details>
</template>

<style scoped>
.nr-presets { margin-top: 24px; padding-top: 12px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
summary { cursor: pointer; padding: 8px 0; font-weight: 600; }
p { margin: 12px 0; font-size: 12px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); overflow-wrap: anywhere; }
.preset-actions { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 16px; }
.preset-preview { padding: 12px; border: 1px solid rgba(var(--v-theme-on-surface), .12); border-radius: 8px; }
.preset-error { color: rgb(var(--v-theme-error)); }
pre { max-height: 260px; overflow: auto; padding: 12px; font-size: 11px; white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
