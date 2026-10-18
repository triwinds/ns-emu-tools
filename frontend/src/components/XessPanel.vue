<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { mdiRefresh, mdiPlay, mdiFolderOpenOutline, mdiLayersOutline, mdiClipboardTextSearchOutline, mdiCheckCircleOutline } from '@mdi/js'
import { graphicsCommand, type GraphicsApi } from '@/utils/graphics'
import { useProgressStore } from '@/stores/ProgressStore'

interface InputSizing { scalePercent: number; maxEdge: number }
interface Settings { nativeAa: boolean; fg: boolean; inputSizing: InputSizing }
interface ComponentStatus {
  supported: boolean; version: string; bundledReady: boolean; downloadAvailable: boolean; installed: boolean; message: string
  targetReady: boolean; targetHash: string | null; targetMessage: string; settings: Settings
}
interface Live {
  connected: boolean; launchState: string; launchMessage?: string; availableNativeAa?: boolean; availableFg?: boolean; inputScalingLiveSupported?: boolean; sentInputScaleRevision?: number
  runtime?: { nativeAaRequested: boolean; nativeAaActive: boolean; fgRequested: boolean; fgActive: boolean; failed: boolean; sourceFps?: number; sdkQueuedFps?: number; error?: unknown; inputScale?: { inputExtent?: number[]; originalExtent?: number[]; outputExtent?: number[]; active?: boolean; error?: unknown; appliedRevision?: number } }
}
const props = defineProps<{ executable: string; api: GraphicsApi; disabled?: boolean; unified?: boolean }>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const report = ref<ComponentStatus | null>(null)
const progress = useProgressStore()
const settings = ref<Settings>({ nativeAa: true, fg: false, inputSizing: { scalePercent: 100, maxEdge: 0 } })
const live = ref<Live>({ connected: false, launchState: 'idle' })
const busy = ref(false)
const error = ref('')
const feedback = ref('')
const pollError = ref('')
const game = ref('')
const details = ref(false)
const inputFeedback = ref('自动保存；连接通过 XeSS 启动的游戏后实时应用。')
const inputPending = ref(0)
watch(live, value => {
  if (!inputPending.value) return
  if (!value.connected) {
    inputPending.value = 0
    inputFeedback.value = '已保存；游戏连接已断开，下次以 XeSS 启动时使用。'
  } else if ((value.runtime?.inputScale?.appliedRevision ?? 0) >= inputPending.value) {
    inputPending.value = 0
    inputFeedback.value = value.runtime?.inputScale?.error ? '已保存；当前画面未能应用缩放，请查看错误。' : '输入尺寸已在当前游戏中生效。'
  }
})
const locked = computed(() => busy.value || props.disabled)
const running = computed(() => ['launching', 'started'].includes(live.value.launchState))
const aaActive = computed(() => live.value.connected && live.value.runtime?.nativeAaActive)
const fgActive = computed(() => live.value.connected && live.value.runtime?.fgActive)
const setupStep = computed(() => !report.value?.targetReady ? 1 : !report.value.installed ? 2 : 3)
const nextStep = computed(() => {
  if (!report.value) return '正在检查组件与启动条件…'
  if (!report.value.supported) return report.value.message
  if (!report.value.targetReady) return report.value.targetMessage
  if (report.value.installed) return '组件已安装。选择下方效果，再通过此面板启动模拟器。'
  return report.value.message
})
const cleanPath = (value: string) => value.replace(/^\\\\\?\\UNC\\/i, '\\\\').replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const message = (e: unknown) => e instanceof Error ? e.message : String(e)
let alive = true
let generation = 0
let polling = false
async function poll() {
  if (!props.executable || polling || !report.value?.targetReady) return
  polling = true
  const selected = props.executable
  try {
    const value = await graphicsCommand<Live>('live_xess', { executable: selected })
    if (alive && selected === props.executable) { live.value = value; pollError.value = '' }
  } catch (e) { if (alive && selected === props.executable) pollError.value = message(e) }
  finally { polling = false }
}
async function refresh() {
  const request = ++generation
  const value = await graphicsCommand<ComponentStatus>('get_xess_component', { executable: props.executable || null, graphicsApi: props.api })
  if (!alive || request !== generation) return
  report.value = value; settings.value = value.settings
  await poll()
}
async function action(work: () => Promise<void>) {
  if (locked.value) return
  busy.value = true; emit('busy', true); error.value = ''; feedback.value = ''
  try { await work() } catch (e) { if (alive) error.value = message(e) }
  finally { if (alive) { busy.value = false; emit('busy', false) } }
}
async function update(patch: Partial<Settings>) {
  await action(async () => {
    settings.value = await graphicsCommand<Settings>('save_xess_settings', { settings: { ...settings.value, ...patch } })
    feedback.value = '已保存，应用于下一次 XeSS 启动。'
  })
}
async function saveInput(percent: number, maxEdge: number) {
  await action(async () => {
    const inputSizing = { scalePercent: percent, maxEdge }
    settings.value = await graphicsCommand<Settings>('save_xess_settings', { settings: { ...settings.value, inputSizing } })
    if (!props.executable) { inputFeedback.value = '已保存；下次以 XeSS 启动游戏时使用。'; return }
    const status = await graphicsCommand<Live>('live_xess', { executable: props.executable })
    if (!status.connected) inputFeedback.value = '已保存；下次以 XeSS 启动游戏时使用。'
    else if (!status.inputScalingLiveSupported) inputFeedback.value = '已保存；请使用新版 XeSS 组件重新启动一次，之后可实时调整。'
    else {
      live.value = await graphicsCommand<Live>('live_xess', { executable: props.executable, inputSizing })
      inputPending.value = live.value.sentInputScaleRevision ?? 0
      inputFeedback.value = inputPending.value ? '已保存，等待下一帧应用新尺寸；无需重启。' : '已保存，尚未收到应用请求确认。'
    }
  })
}
async function component(install: boolean) {
  await action(async () => {
    const value = await graphicsCommand<{ message: string }>(install ? 'install_xess_component' : 'uninstall_xess_component')
    if (install) progress.closeDialog()
    feedback.value = value.message; await refresh()
  })
}
async function chooseGame() {
  const path = await open({ title: '选择要通过 XeSS 启动的游戏', multiple: false, directory: false, filters: [{ name: 'Switch 游戏', extensions: ['xci', 'nsp', 'nro'] }] })
  if (typeof path === 'string') game.value = path
}
async function launch() {
  await action(async () => {
    const value = await graphicsCommand<{ message: string }>('launch_xess', {
      executable: props.executable, graphicsApi: props.api, expectedSha256: report.value?.targetHash, game: game.value || null,
    })
    feedback.value = value.message
    live.value = { connected: false, launchState: 'launching' }
    await poll()
  })
}
async function control(kind: 'nativeAa' | 'fg', enabled: boolean) {
  await action(async () => {
    live.value = await graphicsCommand<Live>('live_xess', { executable: props.executable, [kind]: enabled })
    feedback.value = `已请求${enabled ? '恢复' : '暂停'}本次 ${kind === 'fg' ? 'FG' : 'Native AA'}，以运行状态确认为准。`
  })
}
watch(() => [props.executable, props.api, props.disabled], () => {
  ++generation
  inputPending.value = 0
  report.value = null; live.value = { connected: false, launchState: 'idle' }
  if (props.disabled) return
  void refresh().catch(e => { if (alive) error.value = message(e) })
}, { immediate: true })
const timer = setInterval(() => { if (!busy.value) void poll() }, 2000)
onBeforeUnmount(() => { alive = false; ++generation; clearInterval(timer); emit('busy', false) })
defineExpose({
  state: computed(() => ({ report: report.value, settings: settings.value, live: live.value,
    busy: busy.value, running: running.value, error: error.value || pollError.value, feedback: feedback.value })),
  refresh, component, poll, control,
  showDetails: () => { details.value = true },
  async configure(next: Settings) {
    settings.value = await graphicsCommand<Settings>('save_xess_settings', { settings: next })
  },
  async start() {
    const value = await graphicsCommand<{ message: string }>('launch_xess', {
      executable: props.executable, graphicsApi: props.api, expectedSha256: report.value?.targetHash, game: null,
    })
    feedback.value = value.message
    live.value = { connected: false, launchState: 'launching' }
    await poll()
  },
})
</script>

<template>
  <section
    class="xess-panel"
    :class="{ 'xess-controller': unified }"
    :aria-labelledby="unified ? undefined : 'xess-title'"
    :aria-busy="busy"
  >
    <div v-if="!unified" class="xess-intro">
      <div class="xess-title-line">
        <v-icon
          :icon="mdiLayersOutline"
          color="secondary"
          size="24"
        />
        <h2 id="xess-title">
          XeSS 画面增强
        </h2>
        <v-chip
          size="x-small"
          variant="outlined"
        >
          实验版
        </v-chip>
      </div>
      <p class="xess-description">
        先安装，再选择效果，最后从这里启动模拟器。
      </p>
      <p>面向 AMD、Intel 等显卡，支持 Native AA 与 2× 帧生成。请在模拟器中选择要使用的显卡，实际支持范围以运行状态为准。</p>
      <v-alert
        class="mt-3"
        type="warning"
        variant="tonal"
        density="compact"
      >
        当前实现无法获取游戏真实的深度和运动向量信息。Native AA 不注入 jitter，并逐帧重置历史；FG 使用零运动向量和固定深度，可能产生鬼影、闪烁、UI 变形等问题，运动画质尚未验收。
      </v-alert>
      <ol
        class="xess-steps"
        aria-label="XeSS 画面增强设置流程"
      >
        <li :class="{ complete: !!report?.targetReady, current: setupStep === 1 }">
          <span class="xess-step-number">1</span>
          <div><strong>核对模拟器</strong><span>{{ !executable ? '在上方选择主程序' : report?.targetReady ? '已检查所选主程序' : '等待条件检查' }}</span></div>
        </li>
        <li :class="{ complete: report?.installed, current: setupStep === 2 }">
          <span class="xess-step-number">2</span>
          <div><strong>安装全部组件</strong><span>{{ report?.installed ? '组件已就绪' : '安装与校验独立组件' }}</span></div>
        </li>
        <li :class="{ complete: live.connected, current: setupStep === 3 && !live.connected }">
          <span class="xess-step-number">3</span>
          <div><strong>选择效果并启动</strong><span>{{ live.connected ? '已连接游戏' : '通过专用入口生效' }}</span></div>
        </li>
      </ol>
    </div>

    <div v-if="!unified" class="xess-body">
      <div class="xess-setup">
        <div class="xess-status-line">
          <h3>{{ report?.installed ? '组件已就绪' : '安装 XeSS 组件' }}</h3>
          <span
            class="xess-status"
            role="status"
          >{{ !report ? '检查中' : report.installed ? '已安装' : '未安装' }}</span>
        </div>
        <p v-if="!executable">
          在上方选择模拟器主程序后，自动检查安装条件。
        </p>
        <p
          v-else
          class="xess-path"
        >
          {{ cleanPath(executable) }}
        </p>
        <p
          v-if="report?.targetReady"
          class="xess-match"
        >
          <v-icon
            :icon="mdiCheckCircleOutline"
            size="16"
            color="success"
          /> Vulkan x64 目标检查通过
        </p>
        <p
          class="xess-next-step"
          role="status"
        >
          {{ nextStep }}
        </p>
        <v-alert
          v-if="error || pollError"
          type="error"
          variant="tonal"
          class="mt-4"
          role="alert"
        >
          {{ error || pollError }}
        </v-alert>
        <p
          v-if="feedback"
          role="status"
        >
          {{ feedback }}
        </p>
        <div class="xess-install-actions">
          <v-btn
            v-if="!report?.installed"
            color="primary"
            variant="flat"
            size="large"
            :loading="busy"
            :disabled="locked || !report?.supported || !(report?.bundledReady || report?.downloadAvailable) || running"
            @click="component(true)"
          >
            {{ report?.bundledReady ? '安装 XeSS 组件' : '下载并安装 XeSS 组件' }}
          </v-btn>
          <v-btn
            class="xess-utility-button"
            variant="outlined"
            height="44"
            :prepend-icon="mdiRefresh"
            :disabled="locked"
            @click="action(refresh)"
          >
            重新检查
          </v-btn>
          <v-btn
            class="xess-utility-button"
            variant="outlined"
            height="44"
            :prepend-icon="mdiClipboardTextSearchOutline"
            :disabled="locked || !report"
            @click="details = true"
          >
            检测与安装详情
          </v-btn>
        </div>

        <div class="xess-launch">
          <div>
            <h3>以 XeSS 启动</h3>
            <p>选择游戏直接启动，或先打开模拟器，再从模拟器内进入游戏。普通启动不会加载这些效果。</p>
            <div class="xess-game-actions">
              <v-btn
                variant="text"
                size="small"
                :prepend-icon="mdiFolderOpenOutline"
                :disabled="locked || running"
                @click="action(chooseGame)"
              >
                选择游戏（可选）
              </v-btn>
              <v-btn
                v-if="game"
                variant="text"
                size="small"
                :disabled="locked || running"
                @click="game = ''"
              >
                清除选择
              </v-btn>
            </div>
            <p
              v-if="game"
              class="xess-path"
            >
              {{ cleanPath(game) }}
            </p>
          </div>
          <v-btn
            color="primary"
            variant="flat"
            size="large"
            :prepend-icon="mdiPlay"
            :loading="busy"
            :disabled="locked || running || !report?.installed || !report?.targetReady || (!settings.nativeAa && !settings.fg)"
            @click="launch"
          >
            以 XeSS 启动
          </v-btn>
        </div>

        <div class="xess-effects">
          <InputSizingControls
            :scale-percent="settings.inputSizing.scalePercent"
            :max-edge="settings.inputSizing.maxEdge"
            :disabled="locked || !report?.supported"
            description="先缩小游戏画面输入，再交给 Native AA 处理。关闭 Native AA 时也可使用；FG 接收恢复到窗口尺寸的画面。"
            :feedback="inputFeedback"
            :error="error"
            :runtime="live.connected ? live.runtime?.inputScale : undefined"
            @save="saveInput"
          />
          <div class="xess-effects-heading">
            <h3>选择要启用的效果</h3>
            <p>两项效果可独立或同时使用。设置自动保存，应用于下一次 XeSS 启动，实际效果以游戏运行状态为准。</p>
          </div>
          <div class="xess-effect">
            <h3>抗锯齿 · Native AA</h3>
            <p id="xess-aa-description">
              以相同输入、输出尺寸处理抗锯齿，可与 FG 配合使用，会增加 GPU 开销。
            </p>
            <v-switch
              :model-value="settings.nativeAa"
              label="启用 Native AA"
              color="primary"
              density="comfortable"
              hide-details
              :disabled="locked || !report?.supported"
              aria-describedby="xess-aa-description"
              @update:model-value="update({ nativeAa: Boolean($event) })"
            />
          </div>
          <div class="xess-effect">
            <h3>提升流畅度 · FG 帧生成</h3>
            <p id="xess-fg-description">
              在原游戏窗口显示 2× 帧生成，可与 Native AA 配合使用。
            </p>
            <v-switch
              :model-value="settings.fg"
              label="启用 FG 帧生成"
              color="primary"
              density="comfortable"
              hide-details
              :disabled="locked || !report?.supported"
              aria-describedby="xess-fg-description"
              @update:model-value="update({ fg: Boolean($event) })"
            />
          </div>
          <p class="xess-note">
            XeSS 的组件、设置和启动入口独立于 DLSS、NVIDIA 光流。当前不提供 XeSS SR。
          </p>
        </div>

        <section
          v-if="running || live.connected"
          class="xess-runtime"
          aria-label="XeSS 实际运行状态"
        >
          <h3>本次运行</h3>
          <p v-if="!live.connected">
            等待游戏画面或新的运行状态，尚未确认 XeSS 正在处理。
          </p>
          <template v-else>
            <v-alert
              v-if="live.runtime?.failed"
              type="warning"
              variant="tonal"
              class="mb-3"
            >
              本次画面处理报告错误，请退出模拟器后重新检查组件。{{ live.runtime.error }}
            </v-alert>
            <div class="xess-runtime-row">
              <v-chip
                :color="aaActive ? 'success' : undefined"
                variant="tonal"
              >
                Native AA：{{ aaActive ? '正在处理' : '未处理' }}
              </v-chip>
              <v-chip
                :color="fgActive ? 'success' : undefined"
                variant="tonal"
              >
                FG：{{ fgActive ? '已提交生成帧' : '未插帧' }}
              </v-chip>
              <v-btn
                v-if="live.availableNativeAa"
                size="small"
                variant="text"
                :disabled="locked"
                @click="control('nativeAa', !live.runtime?.nativeAaRequested)"
              >
                {{ live.runtime?.nativeAaRequested ? '暂停本次 AA' : '恢复本次 AA' }}
              </v-btn>
              <v-btn
                v-if="live.availableFg"
                size="small"
                variant="text"
                :disabled="locked"
                @click="control('fg', !live.runtime?.fgRequested)"
              >
                {{ live.runtime?.fgRequested ? '暂停本次 FG' : '恢复本次 FG' }}
              </v-btn>
            </div>
            <p
              v-if="live.runtime?.sourceFps !== undefined"
              class="mt-3"
            >
              源帧 {{ live.runtime.sourceFps.toFixed(1) }} FPS · {{ live.availableFg ? 'SDK 提交' : '呈现' }} {{ live.runtime.sdkQueuedFps?.toFixed(1) }} FPS
            </p>
            <p
              v-if="live.availableFg"
              class="xess-note"
            >
              SDK 提交数不等同于实测屏幕帧率。关闭 FG 后，当前会话仍保留显示桥接。
            </p>
          </template>
        </section>
        <details
          v-if="report?.installed"
          class="xess-maintenance"
        >
          <summary>组件维护</summary>
          <p>更新或卸载前请先关闭通过 XeSS 启动的模拟器。卸载保留已保存的效果设置与运行记录。</p>
          <v-btn
            variant="text"
            :disabled="locked || running"
            @click="component(true)"
          >
            校验并安装 XeSS 组件
          </v-btn>
          <v-btn
            variant="text"
            :disabled="locked || running"
            @click="component(false)"
          >
            卸载 XeSS 组件
          </v-btn>
        </details>
      </div>
    </div>

    <v-dialog
      v-model="details"
      max-width="680"
      scrollable
    >
      <v-card title="XeSS 检测与安装详情">
        <v-card-text class="xess-details">
          <dl v-if="report">
            <div><dt>组件状态</dt><dd>{{ report.message }}</dd></div>
            <div><dt>组件版本</dt><dd>{{ report.version }}</dd></div>
            <div><dt>配套文件</dt><dd>{{ report.bundledReady ? '已通过校验' : '未就绪' }}</dd></div>
            <div><dt>目标程序</dt><dd>{{ executable ? cleanPath(executable) : '尚未选择' }}</dd></div>
            <div><dt>启动条件</dt><dd>{{ report.targetMessage }}</dd></div>
          </dl>
        </v-card-text>
        <v-card-actions>
          <v-spacer /><v-btn
            variant="text"
            @click="details = false"
          >
            关闭
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </section>
</template>

<style scoped>
.xess-panel.xess-controller { display: contents; }
.xess-panel { margin-top: 28px; overflow: hidden; border: 1px solid rgba(var(--v-theme-on-background), .16); border-radius: 16px; background: rgb(var(--v-theme-surface)); }
.xess-intro { padding: 24px 26px 0; }
.xess-title-line, .xess-status-line, .xess-install-actions, .xess-launch, .xess-game-actions, .xess-runtime-row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.xess-title-line { gap: 10px; }
.xess-title-line h2 { font-size: 23px; font-weight: 650; letter-spacing: -.4px; }
.xess-description { margin-top: 10px; line-height: 1.7; font-size: 15px; }
.xess-body { padding: 24px 26px; }
.xess-setup { min-width: 0; }
.xess-status-line { justify-content: space-between; }
.xess-status-line h3 { font-size: 15px; font-weight: 600; }
.xess-status { font-size: 12px; color: rgba(var(--v-theme-on-surface), .7); }
.xess-setup > p { font-size: 13px; line-height: 1.75; margin-top: 10px; }
.xess-path { overflow-wrap: anywhere; word-break: break-word; }
.xess-next-step { max-width: 68ch; }
.xess-steps { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); list-style: none; padding: 0; margin-top: 24px; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.xess-steps li { display: flex; gap: 12px; align-items: center; padding: 16px 0; border-bottom: 3px solid transparent; color: rgba(var(--v-theme-on-surface), .65); }
.xess-steps li.current { border-bottom-color: rgb(var(--v-theme-secondary)); color: rgb(var(--v-theme-on-surface)); }
.xess-step-number { display: grid; place-items: center; width: 30px; height: 30px; flex-shrink: 0; border: 1px solid rgba(var(--v-theme-on-surface), .3); border-radius: 50%; font-weight: 600; }
.xess-steps .current .xess-step-number { background: rgb(var(--v-theme-secondary)); border-color: transparent; color: rgb(var(--v-theme-on-secondary)); }
.xess-steps .complete .xess-step-number { border-color: rgb(var(--v-theme-success)); color: rgb(var(--v-theme-success)); }
.xess-steps strong, .xess-steps li div > span { display: block; }
.xess-steps strong { font-size: 14px; font-weight: 600; }
.xess-steps li div > span { font-size: 12px; margin-top: 3px; }
.xess-install-actions { margin-top: 18px; }
.xess-install-actions .xess-utility-button { padding: 0 16px; border: 1px solid rgba(var(--v-theme-on-surface), .28); border-radius: 8px; background: rgba(var(--v-theme-on-surface), .07); color: rgb(var(--v-theme-on-surface)); font-size: 14px; font-weight: 600; letter-spacing: 0; text-transform: none; transition: background-color .16s ease, border-color .16s ease; }
.xess-utility-button :deep(.v-icon) { color: rgb(var(--v-theme-secondary)); font-size: 20px; }
.xess-install-actions .xess-utility-button:hover:not(:disabled) { background: rgba(var(--v-theme-secondary), .12); border-color: rgba(var(--v-theme-secondary), .65); }
.xess-install-actions .xess-utility-button:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 3px; }
.xess-launch { justify-content: space-between; gap: 24px; flex-wrap: nowrap; padding: 20px; margin-top: 24px; border-radius: 10px; background: rgba(var(--v-theme-primary), .1); border: 1px solid rgba(var(--v-theme-primary), .3); }
.xess-launch > div { min-width: 0; }
.xess-launch > .v-btn { flex-shrink: 0; }
.xess-effects-heading h3, .xess-launch h3, .xess-runtime h3 { font-size: 17px; font-weight: 600; }
.xess-effects-heading p, .xess-launch p, .xess-maintenance p, .xess-runtime p, .xess-note { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.xess-game-actions { margin-top: 10px; gap: 4px; }
.xess-game-actions .v-btn { margin-inline-start: -8px; }
.xess-effects { margin-top: 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .16); padding-top: 22px; }
.xess-effect { padding: 20px 0; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.xess-effects-heading + .xess-effect { margin-top: 20px; }
.xess-effect h3 { font-size: 15px; font-weight: 600; margin-bottom: 8px; }
.xess-effect p { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.xess-effects-heading { margin-top: 22px; }
.xess-runtime { margin-top: 24px; padding-top: 20px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.xess-runtime h3 { margin-bottom: 12px; }
.xess-runtime-row { margin-top: 12px; }
.xess-maintenance { margin-top: 18px; }
.xess-maintenance summary { cursor: pointer; padding: 10px 0; font-size: 13px; }
.xess-maintenance summary:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 4px; border-radius: 3px; }
.xess-details { font-size: 14px; line-height: 1.7; }
.xess-details dl > div { display: grid; grid-template-columns: 90px minmax(0, 1fr); gap: 12px; padding: 12px 0; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.xess-details dt { color: rgba(var(--v-theme-on-surface), .7); }
.xess-details dd { overflow-wrap: anywhere; }
@media (max-width: 650px) { .xess-steps { grid-template-columns: 1fr; }.xess-steps li { padding: 10px 0; gap: 10px; }.xess-launch { flex-direction: column; align-items: stretch; gap: 16px; } }
@media (max-width: 450px) { .xess-intro { padding: 20px 18px 0; }.xess-body { padding: 20px 18px; }.xess-title-line h2 { font-size: 21px; }.xess-install-actions { align-items: stretch; flex-direction: column; }.xess-details dl > div { grid-template-columns: 1fr; gap: 3px; } }
@media (prefers-reduced-motion: reduce) { .xess-install-actions .xess-utility-button { transition: none; } }
</style>
