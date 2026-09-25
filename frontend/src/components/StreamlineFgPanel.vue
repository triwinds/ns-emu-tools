<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import StreamlineFgLive from './StreamlineFgLive.vue'
import { mdiCheckCircleOutline, mdiAlertCircleOutline, mdiClockOutline, mdiLayersOutline } from '@mdi/js'
import type { GraphicsApi } from '@/utils/graphics'
import { detectStreamlineFg, operateStreamlineFg, type FgCheck, type FgPreflight } from '@/utils/streamlineFg'

const props = defineProps<{ executable: string; api: GraphicsApi; disabled: boolean }>()
const report = ref<FgPreflight | null>(null)
const loading = ref(false)
const error = ref('')
const message = ref('')
const session = ref('')
const details = ref(false)
const allowUnverified = ref(false)
let revision = 0
const blocked = computed(() => report.value?.checks.filter(c => c.status === 'blocked') ?? [])
const targetMatches = computed(() => report.value?.compatibility === 'verified')
const statusLabel = computed(() => loading.value ? '正在检查' : error.value ? '操作失败' : !report.value ? '等待检查' : blocked.value.length ? '不满足启用条件' : report.value.requiresTrialConfirmation && !allowUnverified.value ? '兼容性未验证' : report.value.installationState === 'installed' ? '已安装' : report.value.installationState === 'damaged' ? '安装需检查' : report.value.packageAvailable ? '可以安装' : '缺少组件包')
const canUse = computed(() => !!report.value && !blocked.value.length && (!report.value.requiresTrialConfirmation || allowUnverified.value))
const checkIcon = (status: FgCheck['status']) => ({ passed: mdiCheckCircleOutline, blocked: mdiAlertCircleOutline, pending: mdiClockOutline })[status]
const checkLabel = (status: FgCheck['status']) => ({ passed: '通过', blocked: '不满足', pending: '待确认' })[status]
const checkColor = (status: FgCheck['status']) => ({ passed: 'success', blocked: 'error', pending: 'warning' })[status]
const cleanPath = (value: string) => value.replace(/^\\\\\?\\UNC\\/i, '\\\\').replace(/^\\\\\?\\([a-z]:\\)/i, '$1')
const checkedTime = computed(() => report.value ? new Date(report.value.checkedAt).toLocaleString('zh-CN', { hour12: false }) : '')

// Clear old evidence immediately, including when an earlier request is still running.
watch(() => [props.executable, props.api], () => {
  revision++
  allowUnverified.value = false
  report.value = null
  error.value = ''
  message.value = ''
  session.value = ''
  loading.value = false
  details.value = false
}, { flush: 'sync' })

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
async function operate(action: 'install' | 'launch' | 'uninstall') {
  if (!report.value || loading.value || props.disabled) return
  const token = ++revision
  const executable = props.executable
  const api = props.api
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
    if (token === revision) loading.value = false
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
          DLSS 帧生成
        </h2>
        <v-chip
          size="x-small"
          variant="outlined"
        >
          实验版
        </v-chip>
      </div>
      <p class="fg-description">
        在两个游戏帧之间补充一帧，让移动中的画面更连贯。
      </p>
      <div class="fg-tags">
        <span>Ryujinx / Ryubing</span><span>Vulkan</span><span>2× 模式</span>
      </div>
    </div>

    <div class="fg-body">
      <div class="fg-explainer">
        <figure
          class="fg-sequence"
          aria-label="插帧原理示意：原始帧、生成帧、下一原始帧。这不是实时画面。"
        >
          <div
            v-for="(label, index) in ['原始帧', '生成帧', '原始帧']"
            :key="index"
            class="fg-frame"
            :class="{ 'fg-frame-generated': index === 1 }"
          >
            <svg
              viewBox="0 0 160 94"
              aria-hidden="true"
            >
              <path
                d="M0 0H160V94H0Z"
                class="fg-sky"
              />
              <circle
                cx="127"
                cy="22"
                r="9"
                class="fg-sun"
              />
              <path
                d="M0 67L36 31L66 57L98 37L160 76V94H0Z"
                class="fg-mountain"
              />
              <path
                d="M0 76Q44 56 85 77T160 70V94H0Z"
                class="fg-ground"
              />
              <path
                :transform="`translate(${28 + index * 35} 57)`"
                d="M0 0L8 -9L16 0L8 9Z"
                class="fg-subject"
              />
              <path
                d="M0 89H160"
                class="fg-track"
              />
            </svg>
            <span class="fg-frame-label">{{ label }}</span>
          </div>
        </figure>
        <p class="fg-caption">
          示意：30 FPS 游戏画面可补充至约 60 帧输出，实际效果取决于运行条件。
        </p>
        <p class="fg-fps-note">
          模拟器的 FPS 通常只统计原始帧，开启后仍可能显示 30 FPS。
        </p>
      </div>
      <div class="fg-setup">
        <div class="fg-status-line">
          <h3>安装到所选模拟器</h3><span
            class="fg-status"
            role="status"
          >{{ statusLabel }}</span>
        </div>
        <p v-if="!executable">
          先在上方选择模拟器主程序，再检查安装条件。
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
          v-else
          class="fg-package-note"
        >
          {{ report?.packageMessage ?? '检查主程序与本地组件包后安装。' }}
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
        <div class="fg-actions">
          <v-btn
            color="primary"
            variant="flat"
            :loading="loading"
            :disabled="!executable || disabled || loading"
            @click="inspect"
          >
            {{ report || error ? '重新检查' : '检查安装条件' }}
          </v-btn>
          <v-btn
            variant="text"
            :disabled="loading || disabled"
            @click="details = true"
          >
            查看安装方案
          </v-btn>
          <v-btn
            v-if="report?.installationState === 'installed'"
            color="primary"
            :disabled="!canUse || disabled || loading"
            @click="operate('launch')"
          >
            以 FG 启动
          </v-btn>
          <v-btn
            v-if="report && report.installationState !== 'unmanaged'"
            variant="text"
            :disabled="disabled || loading"
            @click="operate('uninstall')"
          >
            卸载 FG 组件
          </v-btn>
        </div>
      </div>
    </div>
    <StreamlineFgLive :executable="executable" />
    <div class="fg-footer">
      <span>安装：{{ report?.installationState === 'installed' ? '已安装' : report?.installationState === 'damaged' ? '需检查' : '未安装' }}</span>
      <span>仅专用启动生效</span>
    </div>

    <v-dialog
      v-model="details"
      max-width="760"
      aria-labelledby="fg-plan-title"
      scrollable
    >
      <v-card class="fg-dialog">
        <v-card-title id="fg-plan-title">
          安装 DLSS 帧生成
        </v-card-title>
        <v-card-text class="fg-dialog-body">
          <p class="fg-dialog-lead">
            独立部署帧生成组件，通过工具箱的专用入口启动游戏。
          </p>
          <div class="fg-plan-target">
            <span>目标模拟器</span><strong>{{ executable ? cleanPath(executable) : '尚未选择，请先返回选择模拟器' }}</strong>
          </div>
          <h3>安装条件</h3>
          <p
            v-if="!report"
            class="fg-muted"
          >
            {{ loading ? '正在核对所选主程序…' : '尚未检测。关闭此窗口后，点击“检查安装条件”。' }}
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
            <div><dt>组件</dt><dd>帧生成图层、Streamline 运行库、DLSS-G 与 Reflex</dd></div>
            <div>
              <dt>安装位置</dt><dd class="fg-path">
                {{ report ? cleanPath(report.plannedDestination) : '工具箱配置目录 / graphics / streamline-fg' }}<small>组件部署到独立版本目录，运行记录单独保存。</small>
              </dd>
            </div>
            <div><dt>生效方式</dt><dd>点击“以 FG 启动”请求 2× 帧生成；关闭模拟器后，普通启动即可停用。</dd></div>
          </dl>
          <p class="fg-muted">
            不替换模拟器主程序，不修改存档或全局 Vulkan 注册。ReShade / DLSS5 组合使用尚未验证。
          </p>
          <div class="fg-release-note">
            <v-icon
              :icon="mdiClockOutline"
              size="20"
            /><div><strong>{{ report?.packageAvailable ? '本地实验组件包' : '组件包未就绪' }}</strong><p>{{ report?.packageMessage ?? '请先检查安装条件。' }}</p></div>
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
          </v-btn><v-btn
            color="primary"
            variant="flat"
            :disabled="!canUse || !report?.packageAvailable || report?.installationState !== 'unmanaged' || loading || disabled"
            :loading="loading"
            @click="operate('install')"
          >
            安装 FG 组件
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
.fg-body { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 30px; padding: 24px 26px; }
.fg-explainer { padding-top: 3px; }
.fg-sequence { display: flex; gap: 8px; margin: 0 0 16px; }
.fg-frame { flex: 1; min-width: 0; }
.fg-frame svg { width: 100%; display: block; border-radius: 7px; border: 1px solid #546e7a; }
.fg-frame-generated svg { border: 2px solid #4db6ac; }
.fg-frame-label { display: block; font-size: 12px; text-align: center; margin-top: 8px; }
.fg-frame-generated .fg-frame-label { font-weight: 650; }
.fg-sky { fill: #243f51; }.fg-sun { fill: #eac780; }.fg-mountain { fill: #54778a; }.fg-ground { fill: #304f5c; }.fg-subject { fill: #b2dfdb; }.fg-track { stroke: #8bb3bd; stroke-width: 1; }
.fg-caption, .fg-fps-note { font-size: 12px; line-height: 1.8; color: rgba(var(--v-theme-on-surface), .72); }
.fg-fps-note { margin-top: 6px; }
.fg-setup { padding-left: 26px; border-left: 1px solid rgba(var(--v-theme-on-surface), .12); }
.fg-status-line { gap: 12px; justify-content: space-between; flex-wrap: wrap; }
.fg-status-line h3 { font-size: 15px; font-weight: 600; }
.fg-status { font-size: 12px; color: rgba(var(--v-theme-on-surface), .7); }
.fg-setup > p, .fg-blockers { font-size: 13px; line-height: 1.75; margin-top: 10px; }
.fg-path { overflow-wrap: anywhere; word-break: break-word; }
.fg-package-note { color: rgba(var(--v-theme-on-surface), .7); }
.fg-blockers { padding-left: 18px; }
.fg-trial { margin-top: 12px; font-size: 13px; line-height: 1.7; padding: 12px; background: rgba(var(--v-theme-warning), .09); border-radius: 8px; }
.fg-error { color: rgb(var(--v-theme-error)); overflow-wrap: anywhere; }
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
@media (max-width: 800px) { .fg-body { grid-template-columns: 1fr; gap: 24px; }.fg-setup { padding: 22px 0 0; border-left: 0; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); }.fg-sequence { max-width: 470px; }.fg-footer { gap: 8px 18px; } }
@media (max-width: 450px) { .fg-intro { padding: 20px 18px 0; }.fg-body { padding: 20px 18px; }.fg-footer { padding: 12px 18px; }.fg-title-line h2 { font-size: 21px; }.fg-actions { align-items: stretch; flex-direction: column; }.fg-deployment > div { grid-template-columns: 1fr; gap: 3px; } }
</style>
