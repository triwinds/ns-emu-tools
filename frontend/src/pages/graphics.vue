<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { onBeforeRouteLeave } from 'vue-router'
import { open } from '@tauri-apps/plugin-dialog'
import { mdiFolderOpenOutline, mdiRefresh, mdiMonitorShimmer, mdiCheckCircleOutline } from '@mdi/js'
import { graphicsCommand, type GraphicsApi, type GraphicsTarget, type Detection, type InstallPreview, type Operation, type ComponentState } from '@/utils/graphics'
import StreamlineFgPanel from '@/components/StreamlineFgPanel.vue'
import { useProgressStore } from '@/stores/ProgressStore'

const targets = ref<GraphicsTarget[]>([])
const executable = ref('')
const api = ref<GraphicsApi>('vulkan')
const report = ref<Detection | null>(null)
const busy = ref(false)
const loading = ref(false)
const error = ref('')
const notice = ref('')
const preview = ref<InstallPreview | null>(null)
const component = ref<'reshade' | 'feeder'>('reshade')
const externalConsent = ref(false)
const vulkanConsent = ref(false)
const dialog = ref(false)
const removal = ref<'reshade' | 'feeder' | null>(null)
const progress = useProgressStore()
let revision = 0
const nameOf = (path: string) => path.split(/[\\/]/).pop()?.replace(/\.exe$/i, '') || '模拟器'
const displayPath = (path: string) => path
  .replace(/^\\\\\?\\UNC\\/i, '\\\\')
  .replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const items = computed(() => targets.value.map(t => ({ title: `${nameOf(t.executable)} — ${displayPath(t.executable)}`, value: t.executable })))
const locked = computed(() => busy.value || loading.value || dialog.value || !!removal.value)
const canInstall = computed(() => !!report.value?.installationAvailable && !locked.value)
const feederPresent = computed(() => report.value && !['notInstalled', 'unsupported'].includes(report.value.feederState))
const states: Record<ComponentState, string> = { notInstalled: '未安装', installed: '已安装', incomplete: '需要恢复', modified: '文件已变更', external: '外部安装', unknown: '未知', unsupported: '不支持', error: '检测失败' }
const stateLabel = (state?: ComponentState) => state ? states[state] : '等待检测'
const managed = (state?: ComponentState) => !!state && ['installed', 'modified', 'incomplete'].includes(state)
const ready = computed(() => preview.value?.planId && !preview.value.blockers.length && (!preview.value.requiresExternalOverwriteConfirmation || externalConsent.value) && (!preview.value.requiresVulkanScopeConfirmation || vulkanConsent.value))
const message = (e: unknown) => e instanceof Error ? e.message : String(e)

async function detect() {
  const token = ++revision
  report.value = null
  preview.value = null
  error.value = ''
  if (!executable.value) return
  loading.value = true
  try {
    const result = await graphicsCommand<Detection>('detect_graphics_components', { executable: executable.value, graphicsApi: api.value })
    if (token === revision) report.value = result
  } catch (e) { if (token === revision) error.value = message(e) }
  finally { if (token === revision) loading.value = false }
}
watch([executable, api], () => { notice.value = ''; void detect() })
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
async function prepare(kind: 'reshade' | 'feeder') {
  busy.value = true
  error.value = ''; notice.value = ''; preview.value = null
  component.value = kind
  externalConsent.value = false; vulkanConsent.value = false
  try {
    preview.value = await graphicsCommand<InstallPreview>(kind === 'reshade' ? 'prepare_official_graphics_component_install' : 'prepare_feeder_install', { executable: executable.value, graphicsApi: api.value })
    dialog.value = true
  } catch (e) { error.value = message(e) }
  finally { progress.closeDialog(); busy.value = false }
}
async function execute(command: string, args: Record<string, unknown>) {
  busy.value = true; error.value = ''; notice.value = ''
  let failure = ''
  try {
    const result = await graphicsCommand<Operation>(command, args)
    notice.value = result.message + (result.preservedFiles?.length ? `；已保留修改过的文件：${result.preservedFiles.join('、')}` : '')
  } catch (e) { failure = message(e) }
  finally {
    progress.closeDialog()
    await detect()
    if (failure) error.value = failure
    busy.value = false
  }
}
async function install() {
  if (!ready.value || !preview.value) return
  const planId = preview.value.planId
  dialog.value = false
  await execute(component.value === 'reshade' ? 'install_graphics_components' : 'install_feeder', { planId, confirmExternalOverwrite: externalConsent.value, confirmVulkanScope: vulkanConsent.value })
}
async function remove() {
  const kind = removal.value
  removal.value = null
  if (kind) await execute(kind === 'reshade' ? 'uninstall_graphics_components' : 'uninstall_feeder', { executable: executable.value, graphicsApi: api.value })
}
async function repair(kind: 'reshade' | 'feeder') {
  await execute(kind === 'reshade' ? 'repair_graphics_components' : 'repair_feeder', { executable: executable.value, graphicsApi: api.value })
}
onBeforeRouteLeave(() => !busy.value)
onMounted(loadTargets)
</script>

<template>
  <main class="graphics-page">
    <header class="page-heading">
      <div>
        <h1>图形增强</h1>
        <p>为你的模拟器管理帧生成、ReShade 与 DLSS5。</p>
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
        <v-icon
          :icon="mdiMonitorShimmer"
          size="32"
          color="primary"
        /><div>
          <h2 id="target-title">
            {{ executable ? nameOf(executable) : '选择模拟器' }}
          </h2><p>组件将安装到此模拟器，请核对路径。</p>
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
        <p>与模拟器设置中的图形后端保持一致。</p>
        <v-btn
          variant="text"
          :loading="loading"
          :disabled="!executable || busy || dialog || !!removal"
          @click="detect"
        >
          重新检测
        </v-btn>
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
    <v-alert
      v-if="notice"
      type="success"
      variant="tonal"
      class="feedback"
      role="status"
    >
      {{ notice }}
    </v-alert>
    <StreamlineFgPanel :executable="executable" :api="api" :disabled="locked" />

    <p class="operation-hint">
      安装或卸载前，请保存游戏并退出模拟器。安装状态仅代表文件状态。
    </p>

    <section
      class="component-row"
      aria-labelledby="reshade-title"
    >
      <div class="component-copy">
        <div class="component-title">
          <h2 id="reshade-title">
            ReShade
          </h2><v-chip
            size="small"
            :color="report?.reshadeState === 'installed' ? 'success' : undefined"
          >
            {{ stateLabel(report?.reshadeState) }}
          </v-chip>
        </div><p>调整色彩、锐化与后处理效果。</p><span class="source-note">官方 Add-on 版本 · DLSS5 的运行基础</span>
      </div>
      <div class="component-actions">
        <v-btn
          color="primary"
          :disabled="!canInstall"
          @click="prepare('reshade')"
        >
          {{ managed(report?.reshadeState) ? '更新 / 重装' : '安装 ReShade' }}
        </v-btn><v-btn
          v-if="report?.reshadeState === 'incomplete'"
          variant="text"
          :disabled="locked"
          @click="repair('reshade')"
        >
          恢复未完成操作
        </v-btn><v-btn
          variant="text"
          :disabled="locked || !managed(report?.reshadeState) || !!feederPresent"
          @click="removal = 'reshade'"
        >
          卸载 ReShade
        </v-btn><small v-if="feederPresent">请先卸载 DLSS5</small>
      </div>
    </section>

    <section
      class="component-row feeder-row"
      aria-labelledby="feeder-title"
    >
      <div class="component-copy">
        <div class="component-title">
          <h2 id="feeder-title">
            DLSS5
          </h2><v-chip
            size="small"
            :color="report?.feederState === 'installed' ? 'success' : undefined"
          >
            {{ stateLabel(report?.feederState) }}
          </v-chip>
        </div><p>通过 Feeder 与 RenoDX 接入神经渲染。</p><span class="source-note">RHI 镜像组件 · 卸载时保留 ReShade</span><p class="compatibility-note">
          兼容性尚未验证通过。Ryujinx 实测存在深度数据缺失、窗口调整后崩溃的问题。
        </p>
      </div>
      <div class="component-actions">
        <v-btn
          color="primary"
          variant="tonal"
          :disabled="!canInstall || report?.reshadeState !== 'installed'"
          @click="prepare('feeder')"
        >
          {{ managed(report?.feederState) ? '重装 DLSS5' : '安装 DLSS5' }}
        </v-btn><small v-if="report && report.reshadeState !== 'installed'">请先安装或恢复 ReShade</small><v-btn
          v-if="report?.feederState === 'incomplete'"
          variant="text"
          :disabled="locked"
          @click="repair('feeder')"
        >
          恢复未完成操作
        </v-btn><v-btn
          variant="text"
          :disabled="locked || !managed(report?.feederState)"
          @click="removal = 'feeder'"
        >
          卸载 DLSS5
        </v-btn>
      </div>
    </section>

    <details
      v-if="report"
      class="diagnostics"
    >
      <summary>检测详情 · {{ report.architecture }}</summary><p v-if="!report.diagnostics.length">
        检测完成，无额外提示。
      </p><ul v-else>
        <li
          v-for="line in report.diagnostics"
          :key="line"
        >
          {{ line }}
        </li>
      </ul>
    </details>
    <footer>
      <v-icon
        :icon="mdiCheckCircleOutline"
        size="18"
      />安装前预检，变更前备份；修改过的配置按后端恢复规则保留。
    </footer>

    <v-dialog
      v-model="dialog"
      max-width="680"
      :persistent="busy"
    >
      <v-card
        v-if="preview"
        class="preview-card"
      >
        <v-card-title>安装预览 · {{ component === 'reshade' ? 'ReShade' : 'DLSS5' }}</v-card-title>
        <v-card-text class="preview-body">
          <p class="target-path">
            {{ displayPath(executable) }}
          </p><p>{{ api === 'vulkan' ? 'Vulkan' : 'OpenGL' }} · {{ preview.version || preview.bundle }}</p>
          <v-alert
            v-for="blocker in preview.blockers"
            :key="blocker"
            type="error"
            variant="tonal"
            class="my-3"
          >
            {{ blocker }}
          </v-alert>
          <ul class="preview-notes">
            <li
              v-for="line in preview.diagnostics"
              :key="line"
            >
              {{ line }}
            </li>
          </ul>
          <details>
            <summary>文件与下载来源</summary><p
              v-if="preview.destination"
              class="target-path"
            >
              {{ displayPath(preview.destination) }}
            </p><ul>
              <li
                v-for="file in preview.files"
                :key="file"
              >
                {{ file }}
              </li>
            </ul><p
              v-if="preview.sourceUrl"
              class="target-path"
            >
              {{ preview.sourceUrl }}
            </p><p
              v-for="source in preview.sources"
              :key="source.name"
              class="target-path"
            >
              {{ source.name }}：{{ source.url }}
            </p>
          </details>
          <v-checkbox
            v-if="preview.requiresExternalOverwriteConfirmation"
            v-model="externalConsent"
            label="备份并替换检测到的外部安装"
            hide-details
          />
          <template v-if="preview.requiresVulkanScopeConfirmation">
            <v-alert
              type="warning"
              variant="tonal"
              class="mt-4"
            >
              Vulkan 使用当前 Windows 用户的共享图层，其他包含 ReShade.ini 的程序也可能加载它。
            </v-alert><p
              v-for="target in preview.affectedTargets"
              :key="target"
              class="target-path"
            >
              {{ displayPath(target) }}
            </p><v-checkbox
              v-model="vulkanConsent"
              label="我了解并同意共享 Vulkan 图层的影响范围"
              hide-details
            />
          </template>
        </v-card-text><v-card-actions>
          <v-spacer /><v-btn @click="dialog = false">
            取消
          </v-btn><v-btn
            color="primary"
            variant="flat"
            :disabled="!ready || busy"
            @click="install"
          >
            确认安装
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
    <v-dialog
      :model-value="!!removal"
      max-width="480"
      @update:model-value="value => { if (!value) removal = null }"
    >
      <v-card>
        <v-card-title>卸载 {{ removal === 'feeder' ? 'DLSS5' : 'ReShade' }}</v-card-title><v-card-text>
          <p class="target-path">
            {{ displayPath(executable) }}
          </p><p>{{ removal === 'feeder' ? '移除管理的 DLSS5 组件并恢复备份，保留 ReShade。' : '移除管理的 ReShade 安装并恢复备份。' }}修改过的文件将按恢复规则处理。</p>
        </v-card-text><v-card-actions>
          <v-spacer /><v-btn @click="removal = null">
            取消
          </v-btn><v-btn
            color="error"
            @click="remove"
          >
            确认卸载
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </main>
</template>

<style scoped>
.graphics-page { max-width: 1080px; margin: 0 auto; padding: 36px 32px; font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; }
.page-heading, .target-heading, .component-title, .target-controls, .api-row, footer { display: flex; align-items: center; gap: 16px; }
.page-heading { justify-content: space-between; margin-bottom: 28px; }
h1 { font-size: 30px; font-weight: 650; letter-spacing: -.6px; }
h2 { font-size: 22px; font-weight: 600; }
p { line-height: 1.7; }
.page-heading p, .target-heading p, .source-note, .api-row p, .operation-hint, footer { color: rgba(var(--v-theme-on-background), .7); font-size: 14px; }
.target-panel { background: rgb(var(--v-theme-surface)); border-radius: 16px; padding: 24px; border-inline-start: 4px solid rgb(var(--v-theme-primary)); }
.target-heading { margin-bottom: 22px; }
.target-controls > .v-select { min-width: 0; }
.target-path { overflow-wrap: anywhere; font-size: 13px; margin: 12px 0; }
.empty-help { font-size: 14px; margin-top: 16px; }
.api-row { margin-top: 22px; flex-wrap: wrap; }
.api-row > .v-select { max-width: 180px; min-width: 150px; }
.api-row > .v-btn { margin-left: auto; }
.operation-hint { margin: 24px 0 4px; }
.component-row { display: grid; grid-template-columns: minmax(0, 1fr) 180px; gap: 32px; padding: 28px 0; border-bottom: 1px solid rgba(var(--v-theme-on-background), .15); }
.component-title { margin-bottom: 10px; }
.component-copy > p { margin-bottom: 6px; }
.component-actions { display: flex; flex-direction: column; gap: 8px; justify-content: center; }
.component-actions small { text-align: center; opacity: .75; }
.compatibility-note { max-width: 62ch; font-size: 13px; margin-top: 14px; padding-left: 12px; border-left: 3px solid rgb(var(--v-theme-warning)); }
.diagnostics { margin: 24px 0; font-size: 14px; }
summary { cursor: pointer; padding: 8px 0; }
summary:focus-visible { outline: 2px solid rgb(var(--v-theme-primary)); outline-offset: 4px; }
ul { padding-left: 22px; line-height: 1.8; overflow-wrap: anywhere; }
footer { font-size: 12px; margin-top: 24px; align-items: flex-start; }
.feedback { margin-top: 20px; white-space: pre-wrap; overflow-wrap: anywhere; }
.preview-body { max-height: 65vh; overflow-y: auto; }
.preview-notes { margin: 16px 0; }
@media (max-width: 650px) { .graphics-page { padding: 24px 16px; } .target-panel { padding: 18px; } .target-controls { flex-wrap: wrap; gap: 8px; } .target-controls > .v-select { flex-basis: 100%; } .component-row { grid-template-columns: 1fr; gap: 18px; } .component-actions { align-items: stretch; } .api-row > .v-btn { margin-left: 0; } .page-heading { gap: 8px; } h1 { font-size: 26px; } }
</style>
