<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import StreamlineFgLive from './StreamlineFgLive.vue'
import NativeNrControls from './NativeNrControls.vue'
import { useConfigStore } from '@/stores/ConfigStore'
import { useProgressStore } from '@/stores/ProgressStore'
import { updateSetting } from '@/utils/tauri'
import { mdiCheckCircleOutline, mdiAlertCircleOutline, mdiClockOutline, mdiLayersOutline } from '@mdi/js'
import type { GraphicsApi } from '@/utils/graphics'
import { detectStreamlineFg, operateStreamlineFg, liveStreamlineFg, type FgLive, type FgCheck, type FgPreflight } from '@/utils/streamlineFg'

const emit = defineEmits<{ busy: [value: boolean] }>()
const activeAction = ref<'install' | 'launch' | 'uninstall' | null>(null)
const props = defineProps<{ executable: string; api: GraphicsApi; disabled: boolean }>()
const configStore = useConfigStore()
const progress = useProgressStore()
const savingNvof = ref(false)
const savingNr = ref(false)
const currentLive = ref<FgLive>({ connected: false })
const nvofError = ref('')
const srFeedback = ref('自动保存；连接专用启动的游戏后实时应用。')
const srPending = ref(0)
let srDeadline = 0
function receiveLive(value: FgLive) {
  currentLive.value = value
  if (!srPending.value) return
  if (value.connected && value.fresh && (value.sr?.appliedRevision ?? 0) >= srPending.value) {
    srPending.value = 0
    srFeedback.value = value.sr?.active ? 'SR 已生效。' : `SR ${value.sr?.reason ?? '未运行'}`
  } else if (Date.now() > srDeadline) {
    srPending.value = 0
    srFeedback.value = '已保存，但尚未收到生效确认。请回到游戏恢复画面后查看运行状态。'
  }
}
const nvofEnabled = computed(() => configStore.config.setting.other?.streamline_nvof ?? true)
const fgEnabled = computed(() => configStore.config.setting.other?.streamline_fg ?? true)
const srEnabled = computed(() => configStore.config.setting.other?.streamline_sr ?? false)
const srMode = computed(() => configStore.config.setting.other?.streamline_sr_mode ?? 'quality')
const srPreset = computed(() => configStore.config.setting.other?.streamline_sr_preset ?? 'default')
const srPresets = [
  { title: '自动（运行库默认）', value: 'default' },
  { title: 'K · 画质优先', value: 'k' },
  { title: 'J · 减少拖影，可能增加闪烁', value: 'j' },
  { title: 'M · Performance 模式默认模型', value: 'm' },
  { title: 'L · Ultra Performance 模式默认模型', value: 'l' },
]
async function setSrPreset(value: string) {
  if (srPresets.some(p => p.value === value)) {
    await saveGraphics({ streamline_sr_preset: value as NonNullable<GraphicsSettings['streamline_sr_preset']> })
  }
}
const savedScale = computed(() => configStore.config.setting.other?.streamline_sr_scale ?? ({ quality: 150, balanced: 172, performance: 200, dlaa: 100 }[srMode.value]))
const sliderScale = (value: number) => Math.min(2, Math.max(1, value / 100))
const srScale = ref(sliderScale(savedScale.value))
watch(savedScale, value => { srScale.value = sliderScale(value) })
type GraphicsSettings = Partial<Pick<typeof configStore.config.setting.other, 'streamline_nvof' | 'streamline_fg' | 'streamline_sr' | 'streamline_sr_mode' | 'streamline_sr_scale' | 'streamline_sr_preset'>>
async function setFg(value: boolean | null) {
  if (value === null) return
  await saveGraphics({ streamline_fg: value })
}
async function setNvof(value: boolean | null) {
  if (value !== null) await saveGraphics({ streamline_nvof: value })
}
async function setSr(value: boolean | null) {
  if (value !== null) await saveGraphics({ streamline_sr: value, streamline_sr_scale: Math.round(srScale.value * 100) })
}
async function setSrScale() {
  const scale = Math.round(srScale.value * 100)
  if (!Number.isFinite(scale) || scale < 100 || scale > 200 || scale === savedScale.value) return
  await saveGraphics({ streamline_sr_scale: scale })
  srScale.value = sliderScale(savedScale.value)
}
async function saveGraphics(patch: GraphicsSettings) {
  if (savingNvof.value || savingNr.value || !configStore.config.setting.other) return
  const executable = props.executable
  savingNvof.value = true
  nvofError.value = ''
  try {
    const setting = configStore.config.setting
    await updateSetting({ ...setting, other: { ...setting.other, ...patch } })
    Object.assign(configStore.config.setting.other, patch)
    if ('streamline_sr' in patch || 'streamline_sr_mode' in patch || 'streamline_sr_scale' in patch || 'streamline_sr_preset' in patch) {
      if (executable !== props.executable) return
      srFeedback.value = '已保存，正在连接游戏…'
      try {
        const status = executable ? await liveStreamlineFg(executable) : { connected: false }
        if (executable !== props.executable) return
        if (!status.connected) {
          srFeedback.value = '已保存；尚未连接游戏，下次专用启动时使用。'
        } else {
          const result = await liveStreamlineFg(executable, undefined, srEnabled.value ? srMode.value : 'off', savedScale.value, srPreset.value)
          if (executable !== props.executable) return
          srPending.value = result.sentSrRevision ?? 0
          srDeadline = Date.now() + 10000
          srFeedback.value = '已保存，正在应用到当前游戏…'
        }
      } catch (e) {
        srFeedback.value = `已保存，实时应用失败：${e instanceof Error ? e.message : String(e)}`
      }
    }
  } catch (e) {
    nvofError.value = `图形设置保存失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    savingNvof.value = false
  }
}
const report = ref<FgPreflight | null>(null)
const loading = ref(false)
const error = ref('')
const message = ref('')
const session = ref('')
const details = ref(false)
const allowUnverified = ref(false)
let revision = 0
let inspectionPending = false
const installed = computed(() => report.value?.installationState === 'installed')
const working = computed(() => loading.value || savingNvof.value || savingNr.value)
watch(working, value => emit('busy', value), { flush: 'sync' })
const installLabel = computed(() => activeAction.value === 'install' ? '正在下载并安装…' : '下载并安装全部组件')
const nextStep = computed(() => !props.executable ? '先在上方选择模拟器主程序。' : loading.value ? (activeAction.value ? '操作进行中，下载进度与取消操作见进度窗口。' : '正在检查模拟器与组件…') : !report.value ? '检查未完成，请重新检查后继续。' : blocked.value.length ? '请先解决下方列出的安装条件。' : report.value.requiresTrialConfirmation && !allowUnverified.value ? '此版本尚未验证，请先阅读并确认兼容性提示。' : installed.value ? '组件已安装。选择下方效果，再通过此面板启动模拟器。' : report.value.installationState === 'damaged' ? '组件不完整。先移除损坏安装，再下载并安装全部组件。' : !report.value.packageAvailable ? '组件包暂不可用，请查看检测详情并重新检查。' : '一次安装 NR、SR / DLAA 和帧生成所需组件，无需手动查找 DLL。')
const blocked = computed(() => report.value?.checks.filter(c => c.status === 'blocked') ?? [])
const targetMatches = computed(() => report.value?.compatibility === 'verified')
const statusLabel = computed(() => loading.value ? activeAction.value === 'install' ? '正在安装' : activeAction.value === 'launch' ? '正在启动' : activeAction.value === 'uninstall' ? '正在卸载' : '正在检查' : error.value ? '操作失败' : !report.value ? '等待检查' : blocked.value.length ? '不满足启用条件' : report.value.requiresTrialConfirmation && !allowUnverified.value ? '兼容性未验证' : report.value.installationState === 'installed' ? '已安装' : report.value.installationState === 'damaged' ? '安装需检查' : report.value.packageAvailable ? '可以安装' : '缺少组件包')
const canUse = computed(() => !!report.value && !blocked.value.length && (!report.value.requiresTrialConfirmation || allowUnverified.value))
const checkIcon = (status: FgCheck['status']) => ({ passed: mdiCheckCircleOutline, blocked: mdiAlertCircleOutline, pending: mdiClockOutline })[status]
const checkLabel = (status: FgCheck['status']) => ({ passed: '通过', blocked: '不满足', pending: '待确认' })[status]
const checkColor = (status: FgCheck['status']) => ({ passed: 'success', blocked: 'error', pending: 'warning' })[status]
const cleanPath = (value: string) => value.replace(/^\\\\\?\\UNC\\/i, '\\\\').replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const checkedTime = computed(() => report.value ? new Date(report.value.checkedAt).toLocaleString('zh-CN', { hour12: false }) : '')

// Clear old evidence immediately, including when an earlier request is still running.
watch(() => [props.executable, props.api], () => {
  revision++
  inspectionPending = true
  srPending.value = 0
  currentLive.value = { connected: false }
  srFeedback.value = '自动保存；连接专用启动的游戏后实时应用。'
  allowUnverified.value = false
  report.value = null
  error.value = ''
  message.value = ''
  session.value = ''
  loading.value = false
  activeAction.value = null
  details.value = false
}, { immediate: true, flush: 'sync' })

async function inspect() {
  if (!props.executable || props.disabled || loading.value) return
  const token = ++revision
  allowUnverified.value = false
  report.value = null
  error.value = ''
  loading.value = true
  try {
    const result = await detectStreamlineFg(props.executable, props.api)
    if (token === revision) report.value = result
  } catch (e) {
    if (token === revision) error.value = e instanceof Error ? e.message : String(e)
  } finally {
    if (token === revision) loading.value = false
  }
}
// Wait for the parent page's detection/operation lock to clear before checking.
watch(() => [props.executable, props.api, props.disabled], () => {
  if (!inspectionPending || !props.executable || props.disabled) return
  inspectionPending = false
  void inspect()
}, { immediate: true, flush: 'post' })
onBeforeUnmount(() => { revision++; emit('busy', false) })
async function operate(action: 'install' | 'launch' | 'uninstall') {
  if (!report.value || loading.value || savingNvof.value || savingNr.value || props.disabled) return
  const token = ++revision
  const executable = props.executable
  const api = props.api
  activeAction.value = action
  loading.value = true
  error.value = ''
  message.value = ''
  try {
    const result = await operateStreamlineFg(action, executable, api, allowUnverified.value, report.value.targetSha256 ?? '')
    if (token !== revision) return
    message.value = result.message
    session.value = result.session ?? ''
    const refreshed = await detectStreamlineFg(executable, api)
    if (token === revision) report.value = refreshed
  } catch (e) {
    if (token === revision) error.value = e instanceof Error ? e.message : String(e)
  } finally {
    progress.closeDialog()
    if (token === revision) { loading.value = false; activeAction.value = null }
  }
}
</script>

<template>
  <section
    class="fg-panel"
    aria-labelledby="fg-title"
    :aria-busy="loading"
  >
    <div class="fg-intro">
      <div class="fg-title-line">
        <v-icon
          :icon="mdiLayersOutline"
          color="secondary"
          size="24"
        />
        <h2 id="fg-title">
          DLSS 画面增强
        </h2>
        <v-chip
          size="x-small"
          variant="outlined"
        >
          实验版
        </v-chip>
      </div>
      <p class="fg-description">
        先安装，再选择效果，最后从这里启动模拟器。
      </p>
      <ol
        class="fg-steps"
        aria-label="画面增强设置流程"
      >
        <li :class="{ complete: !!report && !blocked.length, current: !report || !!blocked.length }">
          <span class="fg-step-number">1</span><div><strong>核对模拟器</strong><span>{{ !executable ? '在上方选择主程序' : report && !blocked.length ? '已检查所选主程序' : '等待条件检查' }}</span></div>
        </li>
        <li :class="{ complete: installed, current: !!report && !blocked.length && !installed }">
          <span class="fg-step-number">2</span><div><strong>安装全部组件</strong><span>{{ installed ? '组件已就绪' : '自动下载与校验' }}</span></div>
        </li>
        <li :class="{ complete: currentLive.connected, current: installed && !currentLive.connected }">
          <span class="fg-step-number">3</span><div><strong>选择效果并启动</strong><span>{{ currentLive.connected ? '已连接游戏' : '通过专用入口生效' }}</span></div>
        </li>
      </ol>
    </div>

    <div class="fg-body">
      <div class="fg-setup">
        <div class="fg-status-line">
          <h3>{{ installed ? '组件已就绪' : '安装画面增强组件' }}</h3><span
            class="fg-status"
            role="status"
          >{{ statusLabel }}</span>
        </div>
        <p v-if="!executable">
          在上方选择模拟器主程序后，自动检查安装条件。
        </p>
        <p
          v-else
          class="fg-path"
        >
          {{ cleanPath(executable) }}
        </p>
        <p
          v-if="report && targetMatches"
          class="fg-match"
        >
          <v-icon
            :icon="mdiCheckCircleOutline"
            size="16"
            color="success"
          /> 已匹配 {{ report.targetVersion }}
        </p>
        <div
          v-if="report?.requiresTrialConfirmation && !blocked.length"
          class="fg-trial"
        >
          <p>此构建兼容性未验证。选择尝试不会跳过运行时能力检查。</p>
          <v-checkbox
            v-model="allowUnverified"
            label="允许尝试此未验证构建"
            density="compact"
            hide-details
            :disabled="disabled || loading"
          />
          <p v-if="allowUnverified">
            已选择尝试；本次选择仅适用于当前检测结果。
          </p>
        </div>
        <p
          v-if="error"
          class="fg-error"
          role="alert"
        >
          {{ error }}
        </p>
        <ul
          v-if="blocked.length"
          class="fg-blockers"
        >
          <li
            v-for="item in blocked"
            :key="item.id"
          >
            {{ item.detail }}
          </li>
        </ul>
        <p
          class="fg-next-step"
          role="status"
        >
          {{ nextStep }}
        </p>
        <div class="fg-install-actions">
          <v-btn
            v-if="!installed"
            color="primary"
            variant="flat"
            size="large"
            :loading="!!activeAction && activeAction !== 'launch'"
            :disabled="disabled || working || !report || (report.installationState !== 'damaged' && (!canUse || !report.packageAvailable))"
            @click="operate(report?.installationState === 'damaged' ? 'uninstall' : 'install')"
          >
            {{ report?.installationState === 'damaged' ? '移除损坏组件' : installLabel }}
          </v-btn>
          <v-btn
            variant="text"
            :disabled="!executable || disabled || working"
            @click="inspect"
          >
            重新检查
          </v-btn>
          <v-btn
            variant="text"
            :disabled="working || disabled"
            @click="details = true"
          >
            检测与安装详情
          </v-btn>
        </div>
        <p
          v-if="!installed"
          class="fg-install-hint"
        >
          安装后可分别启用画面重建、抗锯齿和帧生成。下载可在进度窗口取消。
        </p>
        <p
          v-if="message"
          role="status"
        >
          {{ message }}
        </p>
        <p
          v-if="session"
          class="fg-path"
        >
          本次运行记录：{{ cleanPath(session) }}
        </p>
        <div
          v-if="installed"
          class="fg-launch"
        >
          <div><h3>以画面增强启动</h3><p>启动模拟器后，在模拟器内打开游戏。普通启动不会加载这些效果。</p></div>
          <v-btn
            color="primary"
            variant="flat"
            size="large"
            :loading="activeAction === 'launch'"
            :disabled="!canUse || disabled || working"
            @click="operate('launch')"
          >
            以画面增强启动
          </v-btn>
        </div>
        <div
          v-show="installed"
          class="fg-effects"
        >
          <div class="fg-effects-heading">
            <h3>选择要启用的效果</h3><p>三个效果可独立使用。设置自动保存，实际效果以游戏运行状态为准。</p>
          </div>
          <NativeNrControls
            :executable="executable"
            :refresh-key="report?.checkedAt"
            :disabled="disabled || loading || savingNvof"
            :live="currentLive"
            @busy="savingNr = $event"
          />
          <div class="fg-sr-setting">
            <v-switch
              :model-value="fgEnabled"
              label="提升流畅度 · FG 帧生成（2×）"
              color="primary"
              density="compact"
              hide-details
              inset
              :disabled="disabled || loading || savingNvof || savingNr"
              @update:model-value="setFg"
            />
            <p>插入生成帧，让画面更流畅。下次从此面板启动时生效；游戏运行中可在运行控制里切换。</p>
            <v-switch
              :model-value="srEnabled"
              label="改善锯齿 · SR / DLAA"
              color="primary"
              density="compact"
              hide-details
              inset
              :loading="savingNvof"
              :disabled="disabled || loading || savingNvof || savingNr || !configStore.config.setting.other"
              aria-describedby="fg-sr-description"
              @update:model-value="setSr"
            />
            <p id="fg-sr-description">
              重建画面以改善锯齿，文字和界面也会参与处理。开启后可调整倍率与模型，会增加 GPU 开销。
            </p>
            <div
              v-if="srEnabled"
              class="fg-sr-options"
            >
              <v-select
                :model-value="srPreset"
                :items="srPresets"
                label="SR / DLAA 模型预设"
                variant="outlined"
                density="compact"
                :disabled="disabled || loading || savingNvof || savingNr || !!srPending"
                hint="自动保存并实时应用。不同模型会改变画质和耗时，M / L 不代表一定更快。"
                persistent-hint
                @update:model-value="setSrPreset"
              />
              <div class="fg-sr-scale-label">
                <span id="fg-sr-scale-label">放大倍率</span><output>{{ srScale.toFixed(2) }}×</output>
              </div>
              <v-slider
                v-model="srScale"
                :min="1.0"
                :max="2.0"
                :step="0.05"
                color="primary"
                thumb-label
                hide-details
                aria-labelledby="fg-sr-scale-label"
                :disabled="disabled || loading || savingNvof || savingNr"
                @end="setSrScale"
                @keyup="setSrScale"
              />
              <div class="fg-sr-scale-label">
                <span>1.0× 等尺寸抗锯齿</span><span>2.0×</span>
              </div>
              <p>1.0× 使用 DLAA 等尺寸抗锯齿；高于 1.0× 先重建到更高分辨率，再缩回窗口以改善锯齿。2.0× 表示输入宽高各两倍，处理像素数为四倍，会增加 GPU 开销。</p>
              <p>优先处理模拟器缩放前的画面；无法识别时自动使用窗口画面。不会改变模拟器内部渲染分辨率，不保证提升帧率。</p>
              <p>建议保留 NVIDIA 光流辅助；不可用时逐帧重置重建历史。实际输入尺寸和执行状态见下方运行控制。</p>
            </div>
            <p class="fg-motion-timing">
              {{ srFeedback }} 调整倍率或模型可能短暂停顿，实际运行状态见下方。
            </p>
          </div>
          <details class="fg-motion-setting">
            <summary>高级设置 · NVIDIA 光流辅助</summary>
            <v-switch
              :model-value="nvofEnabled"
              label="NVIDIA 光流辅助"
              color="primary"
              density="compact"
              hide-details
              inset
              :loading="savingNvof"
              :disabled="disabled || loading || savingNvof || savingNr || !configStore.config.setting.other"
              aria-describedby="fg-motion-description fg-motion-timing"
              @update:model-value="setNvof"
            />
            <p id="fg-motion-description">
              利用 NVIDIA 硬件估算画面运动，辅助 SR 和帧生成。会增加处理开销，可关闭对比效果；准备 NR 的会话始终使用硬件光流。
            </p>
            <p
              id="fg-motion-timing"
              class="fg-motion-timing"
            >
              自动保存 · 下次以画面增强启动时生效，当前游戏会话不变。
            </p>
          </details>
          <p
            v-if="nvofError"
            class="fg-error"
            role="alert"
          >
            {{ nvofError }}
          </p>
        </div>
        <details
          v-if="installed"
          class="fg-maintenance"
        >
          <summary>组件维护</summary>
          <p>卸载后需重新安装才能通过画面增强入口启动。</p>
          <v-btn
            variant="text"
            :disabled="disabled || working"
            @click="operate('uninstall')"
          >
            卸载全部画面增强组件
          </v-btn>
        </details>
      </div>
    </div>
    <StreamlineFgLive
      v-show="currentLive.connected"
      :executable="executable"
      @status="receiveLive"
    />
    <p
      v-if="installed && !currentLive.connected"
      class="fg-awaiting"
      role="status"
    >
      游戏尚未连接。通过“以画面增强启动”打开模拟器并运行游戏后，这里会显示实时状态与帧率。
    </p>
    <div class="fg-footer">
      <span>支持 Ryujinx / Ryubing</span>
      <span>需要 Vulkan 图形接口</span>
    </div>

    <v-dialog
      v-model="details"
      max-width="760"
      aria-labelledby="fg-plan-title"
      scrollable
    >
      <v-card class="fg-dialog">
        <v-card-title id="fg-plan-title">
          检测与安装详情
        </v-card-title>
        <v-card-text class="fg-dialog-body">
          <p class="fg-dialog-lead">
            自动下载并部署 NR、SR / DLAA、FG 与配套图层，通过工具箱的专用入口启动游戏。
          </p>
          <div class="fg-plan-target">
            <span>目标模拟器</span><strong>{{ executable ? cleanPath(executable) : '尚未选择，请先返回选择模拟器' }}</strong>
          </div>
          <h3>安装条件</h3>
          <p
            v-if="!report"
            class="fg-muted"
          >
            {{ loading ? '正在核对所选主程序…' : error ? '自动检查失败，请关闭此窗口后点击“重新检查”。' : executable ? '等待自动检查所选主程序…' : '选择模拟器主程序后自动检查。' }}
          </p>
          <ul
            v-else
            class="fg-checks"
          >
            <li
              v-for="item in report.checks"
              :key="item.id"
            >
              <v-icon
                :icon="checkIcon(item.status)"
                :color="checkColor(item.status)"
                size="20"
              />
              <div><strong>{{ item.label }}<span class="fg-check-label">{{ checkLabel(item.status) }}</span></strong><p>{{ item.detail }}</p></div>
            </li>
          </ul>
          <h3>部署内容</h3>
          <dl class="fg-deployment">
            <div><dt>组件</dt><dd>NR 调用桥、画面增强图层、Streamline、DLSS SR / FG 与 Reflex</dd></div>
            <div>
              <dt>安装位置</dt><dd class="fg-path">
                {{ report ? cleanPath(report.plannedDestination) : '工具箱配置目录 / graphics / streamline-fg' }}<small>组件部署到独立版本目录，运行记录单独保存。</small>
              </dd>
            </div>
            <div><dt>生效方式</dt><dd>点击“以画面增强启动”，按三个独立开关运行。普通启动不加载此图层。</dd></div>
          </dl>
          <p class="fg-muted">
            不替换模拟器主程序，不修改存档或全局 Vulkan 注册。
          </p>
          <div class="fg-release-note">
            <v-icon
              :icon="mdiClockOutline"
              size="20"
            /><div><strong>{{ report?.packageAvailable ? '自动下载画面增强组件' : '组件包未就绪' }}</strong><p>{{ report?.packageMessage ?? '选择主程序后自动检查组件包。' }}</p></div>
          </div>
          <details
            v-if="report"
            class="fg-evidence"
          >
            <summary>查看检测记录</summary><p>检查时间：{{ checkedTime }}</p><p class="fg-path">
              主程序 SHA-256：{{ report.targetSha256 }}
            </p><p>本次只检查安装条件，未检测游戏内的实时帧生成状态。</p>
          </details>
        </v-card-text>
        <v-card-actions class="fg-dialog-actions">
          <v-btn
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
.fg-panel { margin-top: 28px; overflow: hidden; border: 1px solid rgba(var(--v-theme-on-background), .16); border-radius: 16px; background: rgb(var(--v-theme-surface)); }
.fg-intro { padding: 24px 26px 0; }
.fg-title-line, .fg-tags, .fg-status-line, .fg-actions, .fg-footer { display: flex; align-items: center; }
.fg-title-line { gap: 10px; flex-wrap: wrap; }
.fg-title-line h2 { font-size: 23px; font-weight: 650; letter-spacing: -.4px; }
.fg-description { margin-top: 10px; line-height: 1.7; font-size: 15px; }
.fg-tags { gap: 18px; margin-top: 12px; font-size: 12px; color: rgba(var(--v-theme-on-surface), .68); }
.fg-body { padding: 24px 26px; }
.fg-setup { min-width: 0; }
.fg-status-line { gap: 12px; justify-content: space-between; flex-wrap: wrap; }
.fg-status-line h3 { font-size: 15px; font-weight: 600; }
.fg-status { font-size: 12px; color: rgba(var(--v-theme-on-surface), .7); }
.fg-setup > p, .fg-blockers { font-size: 13px; line-height: 1.75; margin-top: 10px; }
.fg-path { overflow-wrap: anywhere; word-break: break-word; }
.fg-package-note { color: rgba(var(--v-theme-on-surface), .7); }
.fg-blockers { padding-left: 18px; }
.fg-trial { margin-top: 12px; font-size: 13px; line-height: 1.7; padding: 12px; background: rgba(var(--v-theme-warning), .09); border-radius: 8px; }
.fg-error { color: rgb(var(--v-theme-error)); overflow-wrap: anywhere; }
.fg-sr-setting { margin-top: 0; padding: 18px 0; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-sr-setting p { font-size: 12px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.fg-sr-scale-label { display: flex; justify-content: space-between; gap: 12px; font-size: 12px; font-variant-numeric: tabular-nums; }
.fg-sr-scale-label output { font-weight: 600; }
.fg-sr-options { display: grid; gap: 10px; margin: 16px 0 12px; }
.fg-motion-setting { margin-top: 0; padding-top: 12px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-motion-setting p { font-size: 12px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); }
.fg-motion-setting .fg-motion-timing { margin-top: 6px; }
.fg-motion-setting .fg-error { margin-top: 6px; color: rgb(var(--v-theme-error)); }
.fg-actions { gap: 6px; margin-top: 18px; flex-wrap: wrap; }
.fg-footer { border-top: 1px solid rgba(var(--v-theme-on-surface), .1); gap: 24px; padding: 12px 26px; font-size: 12px; color: rgba(var(--v-theme-on-surface), .65); flex-wrap: wrap; }
.fg-dialog { font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; }
.fg-dialog-body { font-size: 14px; line-height: 1.7; }
.fg-dialog-lead { margin-bottom: 20px; }
.fg-plan-target { padding: 14px 16px; background: rgba(var(--v-theme-on-surface), .05); border-radius: 8px; }
.fg-plan-target span, .fg-plan-target strong { display: block; overflow-wrap: anywhere; }
.fg-plan-target span { color: rgba(var(--v-theme-on-surface), .65); font-size: 12px; margin-bottom: 4px; }
.fg-dialog-body h3 { font-size: 16px; margin: 24px 0 12px; }
.fg-checks { list-style: none; padding: 0; }
.fg-checks li { display: flex; align-items: flex-start; gap: 12px; padding: 10px 0; }
.fg-checks .v-icon { margin-top: 3px; flex-shrink: 0; }
.fg-checks strong { font-size: 14px; font-weight: 600; }
.fg-checks p, .fg-muted { color: rgba(var(--v-theme-on-surface), .7); font-size: 13px; }
.fg-check-label { font-weight: 400; margin-left: 12px; font-size: 12px; }
.fg-deployment { margin-bottom: 16px; }
.fg-deployment > div { display: grid; grid-template-columns: 80px minmax(0, 1fr); gap: 12px; padding: 9px 0; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .1); }
.fg-deployment dt { color: rgba(var(--v-theme-on-surface), .65); }
.fg-deployment small { display: block; margin-top: 4px; color: rgba(var(--v-theme-on-surface), .65); }
.fg-release-note { display: flex; gap: 12px; align-items: flex-start; background: rgba(var(--v-theme-warning), .09); border-radius: 8px; padding: 16px; margin-top: 22px; }
.fg-release-note .v-icon { margin-top: 3px; flex-shrink: 0; }
.fg-release-note p { margin-top: 4px; font-size: 13px; }
.fg-evidence { margin-top: 18px; font-size: 12px; }
.fg-evidence summary { cursor: pointer; padding: 8px 0; }
.fg-evidence summary:focus-visible { outline: 2px solid rgb(var(--v-theme-primary)); outline-offset: 3px; }
.fg-dialog-actions { padding: 16px 24px; border-top: 1px solid rgba(var(--v-theme-on-surface), .1); }
.fg-steps { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); list-style: none; padding: 0; margin-top: 24px; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-steps li { display: flex; gap: 12px; align-items: center; padding: 16px 0; border-bottom: 3px solid transparent; color: rgba(var(--v-theme-on-surface), .65); }
.fg-steps li.current { border-bottom-color: rgb(var(--v-theme-secondary)); color: rgb(var(--v-theme-on-surface)); }
.fg-step-number { display: grid; place-items: center; width: 30px; height: 30px; flex-shrink: 0; border: 1px solid rgba(var(--v-theme-on-surface), .3); border-radius: 50%; font-weight: 600; }
.fg-steps .current .fg-step-number { background: rgb(var(--v-theme-secondary)); border-color: transparent; color: rgb(var(--v-theme-on-secondary)); }
.fg-steps .complete .fg-step-number { border-color: rgb(var(--v-theme-success)); color: rgb(var(--v-theme-success)); }
.fg-steps strong, .fg-steps li div > span { display: block; }
.fg-steps strong { font-size: 14px; font-weight: 600; }
.fg-steps li div > span { font-size: 12px; margin-top: 3px; }
.fg-install-actions { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 18px; }
.fg-next-step { max-width: 68ch; }
.fg-install-hint { color: rgba(var(--v-theme-on-surface), .7); }
.fg-awaiting { padding: 16px 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); }
.fg-effects { margin-top: 26px; border-top: 1px solid rgba(var(--v-theme-on-surface), .16); padding-top: 22px; }
.fg-effects-heading h3, .fg-launch h3 { font-size: 17px; font-weight: 600; }
.fg-effects-heading p, .fg-launch p, .fg-maintenance p { font-size: 13px; line-height: 1.7; color: rgba(var(--v-theme-on-surface), .72); margin-top: 6px; }
.fg-launch { display: flex; justify-content: space-between; align-items: center; gap: 24px; padding: 20px; margin-top: 24px; border-radius: 10px; background: rgba(var(--v-theme-primary), .1); border: 1px solid rgba(var(--v-theme-primary), .3); }
.fg-launch .v-btn { flex-shrink: 0; }
.fg-maintenance { margin-top: 18px; }
.fg-maintenance summary, .fg-motion-setting summary { cursor: pointer; padding: 10px 0; font-size: 13px; }
.fg-motion-setting[open] summary { margin-bottom: 8px; }
summary:focus-visible { outline: 2px solid rgb(var(--v-theme-secondary)); outline-offset: 4px; border-radius: 3px; }
@media (max-width: 650px) { .fg-steps { grid-template-columns: 1fr; }.fg-steps li { padding: 10px 0; gap: 10px; }.fg-launch { flex-direction: column; align-items: stretch; gap: 16px; } }
@media (max-width: 800px) { .fg-footer { gap: 8px 18px; } }
@media (max-width: 450px) { .fg-intro { padding: 20px 18px 0; }.fg-body { padding: 20px 18px; }.fg-footer { padding: 12px 18px; }.fg-title-line h2 { font-size: 21px; }.fg-actions { align-items: stretch; flex-direction: column; }.fg-deployment > div { grid-template-columns: 1fr; gap: 3px; } }
</style>
