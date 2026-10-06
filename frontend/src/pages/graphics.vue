<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { mdiFolderOpenOutline, mdiRefresh, mdiMonitorShimmer } from '@mdi/js'
import { graphicsCommand, type GraphicsApi, type GraphicsTarget } from '@/utils/graphics'
import StreamlineFgPanel from '@/components/StreamlineFgPanel.vue'
import { useGraphicsGpu } from '@/utils/graphicsGpu'

const { capabilities: gpu, error: gpuError, load: loadGpu } = useGraphicsGpu()
async function refreshGpu() {
  await loadGpu(true)
  if (gpu.value?.hasNvidia) await loadTargets()
}

const selectionKey = 'ns-emu-tools:graphics-selection:v1'
function readSelection(): { executable: string; api: GraphicsApi } {
  const fallback = { executable: '', api: 'vulkan' as GraphicsApi }
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(selectionKey) ?? 'null')
    if (!saved || typeof saved !== 'object') return fallback
    const value = saved as Record<string, unknown>
    return {
      executable: typeof value.executable === 'string' ? value.executable.trim() : '',
      api: value.api === 'openGl' ? 'openGl' : 'vulkan',
    }
  } catch { return fallback }
}
const previousSelection = readSelection()
// Keep manually selected paths available even when automatic discovery misses them.
const targets = ref<GraphicsTarget[]>(previousSelection.executable ? [{ family: 'manual', executable: previousSelection.executable }] : [])
const executable = ref(previousSelection.executable)
const api = ref<GraphicsApi>(previousSelection.api)
const loading = ref(true)
watch([executable, api], ([selectedExecutable, selectedApi]) => {
  try {
    localStorage.setItem(selectionKey, JSON.stringify({ executable: selectedExecutable, api: selectedApi }))
  } catch (e) {
    console.warn('无法保存图形增强页面的上次选择', e)
  }
}, { flush: 'sync' })
const error = ref('')
const nameOf = (path: string) => path.split(/[\\/]/).pop()?.replace(/\.exe$/i, '') || '模拟器'
const displayPath = (path: string) => path
  .replace(/^\\\\\?\\UNC\\/i, '\\\\')
  .replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const items = computed(() => targets.value.map(t => ({ title: `${nameOf(t.executable)} — ${displayPath(t.executable)}`, value: t.executable })))
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
  if (gpu.value?.hasNvidia) await loadTargets()
  else loading.value = false
})
</script>

<template>
  <main class="graphics-page">
    <header class="page-heading">
      <div>
        <h1>图形增强</h1>
        <p>为你的模拟器管理原生神经渲染、超分辨率与帧生成。</p>
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
      v-if="gpu?.hasNvidia"
      class="target-panel"
      aria-labelledby="target-title"
    >
      <div class="target-heading">
        <v-icon
          :icon="mdiMonitorShimmer"
          size="32"
          color="primary"
        /><div>
          <h2 id="target-title">
            {{ executable ? nameOf(executable) : '选择模拟器' }}
          </h2><p>第 1 步：核对模拟器主程序，再确认图形接口。</p>
        </div>
      </div>
      <div class="target-controls">
        <v-select
          v-model="executable"
          :items="items"
          label="目标模拟器"
          variant="outlined"
          hide-details
          :disabled="locked"
          no-data-text="未发现模拟器，请选择 EXE"
        />
        <v-btn
          :prepend-icon="mdiFolderOpenOutline"
          variant="tonal"
          :disabled="locked"
          @click="browse"
        >
          选择 EXE
        </v-btn>
        <v-btn
          :icon="mdiRefresh"
          variant="text"
          aria-label="重新扫描模拟器"
          :disabled="locked"
          @click="loadTargets"
        />
      </div>
      <p
        v-if="executable"
        class="target-path"
      >
        {{ displayPath(executable) }}
      </p>
      <p
        v-else
        class="empty-help"
      >
        读取当前配置与历史目录。没有找到？点击“选择 EXE”定位模拟器主程序。
      </p>
      <div class="api-row">
        <v-select
          v-model="api"
          :items="[{title: 'Vulkan', value: 'vulkan'}, {title: 'OpenGL', value: 'openGl'}]"
          label="图形接口"
          variant="outlined"
          density="compact"
          hide-details
          :disabled="locked"
        />
        <p>画面增强需要 Vulkan。请在模拟器设置中选择相同的图形后端。</p>
      </div>
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
    <StreamlineFgPanel
      v-if="gpu?.hasNvidia"
      :gpu="gpu"
      :executable="executable"
      :api="api"
      :disabled="loading"
      @busy="panelBusy = $event"
    />
    <v-alert
      v-else
      type="info"
      variant="tonal"
    >
      {{ gpuError || (gpu ? '未检测到 NVIDIA 显卡，图形增强菜单不可用。' : '正在检测本机显卡…') }}
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
.graphics-page { max-width: 1080px; margin: 0 auto; padding: 36px 32px; font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; }
.page-heading, .target-heading, .target-controls, .api-row { display: flex; align-items: center; gap: 16px; }
.page-heading { justify-content: space-between; margin-bottom: 28px; }
h1 { font-size: 30px; font-weight: 650; letter-spacing: -.6px; }
h2 { font-size: 22px; font-weight: 600; }
p { line-height: 1.7; }
.page-heading p, .target-heading p, .api-row p { color: rgba(var(--v-theme-on-background), .7); font-size: 14px; }
.target-panel { background: rgb(var(--v-theme-surface)); border-radius: 16px; padding: 24px; border-inline-start: 4px solid rgb(var(--v-theme-primary)); }
.target-heading { margin-bottom: 22px; }
.target-controls > .v-select { min-width: 0; }
.target-path { overflow-wrap: anywhere; font-size: 13px; margin: 12px 0; }
.empty-help { font-size: 14px; margin-top: 16px; }
.api-row { margin-top: 22px; flex-wrap: wrap; }
.api-row > .v-select { max-width: 180px; min-width: 150px; }
.feedback { margin-top: 20px; white-space: pre-wrap; overflow-wrap: anywhere; }
@media (max-width: 650px) { .graphics-page { padding: 24px 16px; } .target-panel { padding: 18px; } .target-controls { flex-wrap: wrap; gap: 8px; } .target-controls > .v-select { flex-basis: 100%; } .page-heading { gap: 8px; } h1 { font-size: 26px; } }
</style>
