<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { mdiFolderOpenOutline, mdiRefresh, mdiMonitorShimmer } from '@mdi/js'
import { graphicsCommand, type GraphicsTarget } from '@/utils/graphics'
import GraphicsEnhancementPanel from '@/components/GraphicsEnhancementPanel.vue'
import { useGraphicsGpu } from '@/utils/graphicsGpu'

const { capabilities: gpu, error: gpuError, load: loadGpu } = useGraphicsGpu()
async function refreshGpu() {
  await loadGpu(true)
  await loadTargets()
}

const selectionKey = 'ns-emu-tools:graphics-selection:v1'
function readSelection(): { executable: string } {
  const fallback = { executable: '' }
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(selectionKey) ?? 'null')
    if (!saved || typeof saved !== 'object') return fallback
    const value = saved as Record<string, unknown>
    return {
      executable: typeof value.executable === 'string' ? value.executable.trim() : '',
    }
  } catch { return fallback }
}
const previousSelection = readSelection()
// Keep manually selected paths available even when automatic discovery misses them.
const targets = ref<GraphicsTarget[]>(previousSelection.executable ? [{ family: 'manual', executable: previousSelection.executable }] : [])
const executable = ref(previousSelection.executable)
const selectedTarget = computed({
  get: () => executable.value || null,
  set: (value: string | null) => { executable.value = value ?? '' },
})
const loading = ref(true)
watch(executable, selectedExecutable => {
  try {
    localStorage.setItem(selectionKey, JSON.stringify({ executable: selectedExecutable }))
  } catch (e) {
    console.warn('无法保存图形增强页面的上次选择', e)
  }
}, { flush: 'sync' })
const error = ref('')
const nameOf = (path: string) => path.split(/[\\/]/).pop()?.replace(/\.exe$/i, '') || '模拟器'
const displayPath = (path: string) => path
  .replace(/^\\\\\?\\UNC\\/i, '\\\\')
  .replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const items = computed(() => targets.value.map(t => ({ title: nameOf(t.executable), path: displayPath(t.executable), value: t.executable })))
const panelBusy = ref(false)
const locked = computed(() => loading.value || panelBusy.value)
const message = (e: unknown) => e instanceof Error ? e.message : String(e)

async function loadTargets() {
  loading.value = true
  error.value = ''
  try {
    const found = await graphicsCommand<GraphicsTarget[]>('list_graphics_component_targets')
    const selected = targets.value.find(t => t.executable === executable.value)
    if (selected && !found.some(t => t.executable === selected.executable)) found.push(selected)
    targets.value = found
    if (!executable.value && targets.value[0]) executable.value = targets.value[0].executable
  } catch (e) { error.value = message(e) }
  finally { loading.value = false }
}
async function browse() {
  try {
    const path = await open({ title: '选择模拟器主程序', multiple: false, directory: false, filters: [{ name: 'Windows 模拟器', extensions: ['exe'] }] })
    if (typeof path === 'string') {
      if (!targets.value.some(t => t.executable === path)) targets.value.push({ family: 'manual', executable: path })
      executable.value = path
    }
  } catch (e) { error.value = message(e) }
}
onMounted(async () => {
  await loadGpu()
  await loadTargets()
})
</script>

<template>
  <main class="graphics-page">
    <header class="page-heading">
      <div>
        <h1>图形增强</h1>
        <p>为模拟器组合重建与帧生成，自由选择 DLSS 或 XeSS。</p>
        <p class="text-warning mt-2">
          使用前，请先完整备份模拟器目录，并备份配置与存档。
        </p>
      </div>
      <v-chip
        size="small"
        variant="outlined"
      >
        实验性功能
      </v-chip>
    </header>

    <section
      class="target-panel"
      aria-labelledby="target-title"
    >
      <div class="target-heading">
        <h2 id="target-title">
          目标模拟器
        </h2>
        <v-btn
          :prepend-icon="mdiRefresh"
          variant="text"
          size="small"
          :loading="loading"
          :disabled="locked"
          @click="loadTargets"
        >
          重新扫描
        </v-btn>
      </div>
      <div class="target-controls">
        <v-select
          v-model="selectedTarget"
          :items="items"
          aria-label="目标模拟器"
          placeholder="选择已发现的模拟器"
          persistent-placeholder
          variant="outlined"
          class="target-select"
          hide-details
          :disabled="locked"
          no-data-text="未发现模拟器，请选择 EXE"
        >
          <template #prepend-inner>
            <v-icon
              :icon="mdiMonitorShimmer"
              size="24"
              color="primary"
            />
          </template>
          <template #selection="{ item }">
            <div class="target-selection">
              <span class="target-name">{{ item.title }}</span>
              <span
                class="target-path"
                :title="item.raw.path"
              >{{ item.raw.path }}</span>
            </div>
          </template>
          <template #item="{ props: itemProps, item }">
            <v-list-item
              v-bind="itemProps"
              :subtitle="item.raw.path"
              class="target-option"
            />
          </template>
        </v-select>
        <v-btn
          :prepend-icon="mdiFolderOpenOutline"
          variant="outlined"
          class="target-browse"
          :disabled="locked"
          @click="browse"
        >
          选择 EXE
        </v-btn>
      </div>
      <p
        v-if="!executable"
        class="empty-help"
      >
        未找到模拟器？点击“选择 EXE”定位模拟器主程序。
      </p>
      <p class="target-help">
        仅支持 Vulkan，请先在模拟器的图形设置中启用。
      </p>
    </section>

    <v-alert
      v-if="error"
      type="error"
      variant="tonal"
      class="feedback"
      role="alert"
    >
      {{ error }}
    </v-alert>
    <GraphicsEnhancementPanel
      :gpu="gpu"
      :executable="executable"
      api="vulkan"
      :disabled="loading"
      @busy="panelBusy = $event"
    />
    <v-alert
      v-if="gpuError"
      type="info"
      variant="tonal"
    >
      {{ gpuError }}
      <v-btn
        v-if="gpu || gpuError"
        variant="text"
        @click="refreshGpu"
      >
        重新检测
      </v-btn>
    </v-alert>
  </main>
</template>

<style scoped>
.graphics-page { max-width: 1080px; margin: 0 auto; padding: 28px 32px; font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; }
.page-heading, .target-heading, .target-controls { display: flex; align-items: center; gap: 16px; }
.page-heading { justify-content: space-between; margin-bottom: 22px; }
h1 { font-size: 30px; font-weight: 650; letter-spacing: -.6px; }
h2 { font-size: 18px; font-weight: 600; }
p { line-height: 1.7; }
.page-heading p { color: rgba(var(--v-theme-on-background), .7); font-size: 14px; }
.target-panel { background: rgb(var(--v-theme-surface)); border-radius: 12px; padding: 18px 24px 16px; border: 1px solid rgba(var(--v-theme-on-surface), .12); }
.target-heading { justify-content: space-between; margin-bottom: 14px; gap: 12px; }
.target-heading .v-btn { flex: none; color: rgba(var(--v-theme-on-surface), .75); letter-spacing: 0; }
.target-controls { gap: 12px; }
.target-select { flex: 1 1 0; min-width: 0; }
.target-select :deep(.v-field) { border-radius: 8px; }
.target-select :deep(.v-field__input) { min-height: 72px; padding-top: 12px; padding-bottom: 12px; flex-wrap: nowrap; }
.target-select :deep(.v-field__prepend-inner) { align-items: center; padding: 0 8px 0 2px; }
.target-select :deep(.v-select__selection) { width: 100%; min-width: 0; margin-inline-end: 0; }
.target-selection { display: flex; flex-direction: column; gap: 3px; min-width: 0; width: 100%; }
.target-name { font-size: 16px; font-weight: 600; line-height: 24px; }
.target-path { font-size: 13px; line-height: 20px; color: rgba(var(--v-theme-on-surface), .65); }
.target-name, .target-path { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.target-option :deep(.v-list-item-subtitle) { overflow-wrap: anywhere; -webkit-line-clamp: 2; }
.target-browse { height: 44px; flex: none; padding-inline: 18px; border-color: rgba(var(--v-theme-on-surface), .25); border-radius: 8px; letter-spacing: 0; }
.target-help { margin-top: 14px; padding-top: 12px; border-top: 1px solid rgba(var(--v-theme-on-surface), .08); font-size: 13px; color: rgba(var(--v-theme-on-surface), .65); }
.empty-help { font-size: 13px; margin-top: 12px; color: rgba(var(--v-theme-on-surface), .75); }
.target-panel :deep(.v-btn:focus-visible), .target-select :deep(.v-field:has(input:focus-visible)) { outline: 2px solid rgb(var(--v-theme-primary)); outline-offset: 3px; }
.feedback { margin-top: 20px; white-space: pre-wrap; overflow-wrap: anywhere; }
@media (max-width: 650px) { .graphics-page { padding: 24px 16px; } .target-panel { padding: 16px; } .target-controls { flex-wrap: wrap; gap: 12px; } .target-select { flex-basis: 100%; } .target-browse { width: 100%; } .page-heading { gap: 8px; } h1 { font-size: 26px; } }
</style>
