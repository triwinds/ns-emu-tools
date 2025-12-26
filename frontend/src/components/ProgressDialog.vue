<template>
  <v-dialog v-model="store.dialogOpen" max-width="520" persistent>
    <v-card class="progress-dialog">
      <!-- Header -->
      <v-card-title class="dialog-header">
        <span class="header-title">{{ title }}</span>
      </v-card-title>
      <v-divider></v-divider>

      <!-- Stepper Content -->
      <v-card-text class="dialog-content">
        <div class="stepper">
          <template v-for="(step, index) in store.steps" :key="step.id">
            <div
              class="step-item"
              :class="{
                'completed': step.status === 'success',
                'running': step.status === 'running',
                'error': step.status === 'error'
              }"
            >
              <!-- Timeline connector -->
              <div
                v-if="index < store.steps.length - 1"
                class="step-connector"
                :class="{
                  'connector-completed': step.status === 'success',
                  'connector-running': step.status === 'running'
                }"
              ></div>

              <!-- Step Icon -->
              <div class="step-icon-wrapper">
                <div
                  class="step-icon"
                  :class="{
                    'icon-pending': step.status === 'pending',
                    'icon-running': step.status === 'running',
                    'icon-success': step.status === 'success',
                    'icon-error': step.status === 'error',
                    'icon-cancelled': step.status === 'cancelled'
                  }"
                >
                  <!-- Running spinner -->
                  <v-progress-circular
                    v-if="step.status === 'running'"
                    indeterminate
                    size="18"
                    width="2"
                    color="secondary"
                  ></v-progress-circular>

                  <!-- Success icon -->
                  <v-icon
                    v-else-if="step.status === 'success'"
                    size="18"
                    :icon="mdiCheck"
                  ></v-icon>

                  <!-- Error icon -->
                  <v-icon
                    v-else-if="step.status === 'error'"
                    size="18"
                    :icon="mdiClose"
                  ></v-icon>

                  <!-- Cancelled icon -->
                  <v-icon
                    v-else-if="step.status === 'cancelled'"
                    size="18"
                    :icon="mdiMinus"
                  ></v-icon>

                  <!-- Pending icon -->
                  <v-icon
                    v-else
                    size="18"
                    :icon="mdiCircleOutline"
                  ></v-icon>
                </div>
              </div>

              <!-- Step Content -->
              <div class="step-content">
                <div
                  class="step-title"
                  :class="{ 'title-pending': step.status === 'pending', 'title-cancelled': step.status === 'cancelled' }"
                >
                  {{ step.title }}
                </div>

                <!-- Step Description -->
                <div v-if="step.description" class="step-description">
                  {{ step.description }}
                </div>

                <!-- Download Details -->
                <div v-if="step.type === 'download' && step.status === 'running'" class="download-card">
                  <div class="download-header">
                    <div v-if="step.downloadSource" class="download-source">
                      <v-icon size="14" :icon="mdiWeb"></v-icon>
                      {{ step.downloadSource }}
                    </div>
                    <div class="download-percent">{{ step.progress?.toFixed(1) }}%</div>
                  </div>
                  <div class="progress-track">
                    <div class="progress-fill" :style="{ width: `${step.progress || 0}%` }"></div>
                  </div>
                  <div class="download-stats">
                    <div class="stat">
                      <v-icon size="14" :icon="mdiSpeedometer"></v-icon>
                      {{ step.downloadSpeed }}
                    </div>
                    <div class="stat">
                      <v-icon size="14" :icon="mdiClockOutline"></v-icon>
                      ETA: {{ step.eta }}
                    </div>
                  </div>
                </div>

                <!-- Error Message -->
                <div v-if="step.status === 'error' && step.error" class="error-message">
                  {{ step.error }}
                </div>
              </div>
            </div>
          </template>
        </div>
      </v-card-text>

      <v-divider></v-divider>

      <!-- Actions -->
      <v-card-actions class="dialog-actions">
        <v-spacer></v-spacer>
        <!-- Cancel button when downloading -->
        <v-btn
          v-if="isDownloading"
          variant="text"
          color="error"
          @click="handleCancelDownload"
          :disabled="isCancelling"
        >
          {{ isCancelling ? '取消中...' : '取消' }}
        </v-btn>
        <!-- Retry button on error -->
        <!-- <v-btn
          v-if="hasError"
          variant="text"
          color="primary"
          @click="handleRetry"
        >
          重试
        </v-btn> -->
        <!-- Close button -->
        <v-btn
          variant="text"
          color="primary"
          @click="store.closeDialog()"
          :disabled="isInstalling && !hasError"
        >
          关闭
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>

  <!-- Delete Files Confirmation Dialog -->
  <v-dialog
    v-model="showDeleteConfirm"
    max-width="420"
    persistent
    :scrim="true"
    style="z-index: 2400;"
  >
    <v-card>
      <v-card-title class="dialog-header">
        <span class="header-title">取消下载</span>
      </v-card-title>
      <v-divider></v-divider>

      <v-card-text class="dialog-content" style="padding: 24px;">
        <p style="font-size: 0.9375rem; color: rgb(var(--v-theme-on-surface)); margin: 0;">
          是否同时删除已下载的文件？
        </p>
      </v-card-text>

      <v-divider></v-divider>

      <v-card-actions class="dialog-actions">
        <v-spacer></v-spacer>
        <v-btn
          variant="text"
          color="primary"
          @click="handleCancelOnly"
          :disabled="isCancelling"
        >
          保留文件
        </v-btn>
        <v-btn
          variant="text"
          color="error"
          @click="handleCancelAndDelete"
          :disabled="isCancelling"
        >
          删除文件
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { useProgressStore } from '@/stores/ProgressStore';
import { computed, ref } from 'vue';
import {
  mdiCheck,
  mdiClose,
  mdiCircleOutline,
  mdiMinus,
  mdiWeb,
  mdiSpeedometer,
  mdiClockOutline
} from '@mdi/js';
import { cancelDownload, deletePath } from '@/utils/tauri';

const store = useProgressStore();
const isCancelling = ref(false);
const showDeleteConfirm = ref(false);

// Compute dialog title based on current state
const title = computed(() => {
  if (hasError.value) return '安装失败';
  if (store.steps.every(s => s.status === 'success')) return '安装完成';
  return '安装进度';
});

const isInstalling = computed(() => {
  return store.steps.some(s => s.status === 'running');
});

const isDownloading = computed(() => {
  return store.steps.some(s => s.type === 'download' && s.status === 'running');
});

const hasError = computed(() => {
  return store.steps.some(s => s.status === 'error');
});

async function handleCancelDownload() {
  if (isCancelling.value) return;

  // 先弹出确认对话框
  showDeleteConfirm.value = true;
}

async function handleCancelAndDelete() {
  showDeleteConfirm.value = false;
  isCancelling.value = true;

  try {
    const result = await cancelDownload();
    console.log('取消下载结果:', result);

    // 获取下载的文件路径并删除
    const filePath = result?.data;
    console.log('下载文件路径:', filePath);

    if (filePath) {
      try {
        await deletePath(filePath);
        console.log('已删除文件:', filePath);
      } catch (error) {
        console.error('删除文件失败:', error);
      }
    }
  } catch (error) {
    console.error('取消下载失败:', error);
  } finally {
    isCancelling.value = false;
  }
}

async function handleCancelOnly() {
  showDeleteConfirm.value = false;
  isCancelling.value = true;

  try {
    const result = await cancelDownload();
    console.log('取消下载结果（不删除文件）:', result);
  } catch (error) {
    console.error('取消下载失败:', error);
  } finally {
    isCancelling.value = false;
  }
}

function handleRetry() {
  // For now, just close the dialog. The user can retry from the main UI.
  store.closeDialog();
}
</script>

<style scoped>
.progress-dialog {
  background: rgb(var(--v-theme-surface)) !important;
  border-radius: 28px !important;
}

.dialog-header {
  padding: 20px 24px 16px;
}

.header-title {
  font-size: 1.375rem;
  font-weight: 500;
  color: rgb(var(--v-theme-on-surface));
}

.dialog-content {
  padding: 24px;
}

/* Stepper Styles */
.stepper {
  position: relative;
}

.step-item {
  display: flex;
  position: relative;
  padding-bottom: 24px;
}

.step-item:last-child {
  padding-bottom: 0;
}

/* Timeline connector line */
.step-connector {
  position: absolute;
  left: 15px;
  top: 36px;
  bottom: 0;
  width: 2px;
  background: rgba(var(--v-theme-on-surface), 0.12);
  transition: background 0.3s ease;
}

.step-connector.connector-completed {
  background: rgb(var(--v-theme-success));
}

.step-connector.connector-running {
  background: linear-gradient(180deg, rgb(var(--v-theme-secondary)) 0%, rgba(var(--v-theme-on-surface), 0.12) 100%);
}

/* Step Icon */
.step-icon-wrapper {
  width: 32px;
  display: flex;
  justify-content: center;
  flex-shrink: 0;
  z-index: 1;
}

.step-icon {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(var(--v-theme-surface));
  border: 2px solid rgba(var(--v-theme-on-surface), 0.12);
  color: rgba(var(--v-theme-on-surface), 0.38);
  transition: all 0.3s ease;
}

.step-icon.icon-pending {
  background: rgb(var(--v-theme-surface));
  border-color: rgba(var(--v-theme-on-surface), 0.12);
  color: rgba(var(--v-theme-on-surface), 0.38);
}

.step-icon.icon-running {
  border-color: rgb(var(--v-theme-secondary));
  background: rgba(var(--v-theme-secondary), 0.12);
  color: rgb(var(--v-theme-secondary));
}

.step-icon.icon-success {
  border-color: rgb(var(--v-theme-success));
  background: rgb(var(--v-theme-success));
  color: rgb(var(--v-theme-background));
}

.step-icon.icon-error {
  border-color: rgb(var(--v-theme-error));
  background: rgb(var(--v-theme-error));
  color: rgb(var(--v-theme-background));
}

.step-icon.icon-cancelled {
  border-color: rgba(var(--v-theme-on-surface), 0.38);
  background: rgba(var(--v-theme-on-surface), 0.08);
  color: rgba(var(--v-theme-on-surface), 0.38);
}

/* Step Content */
.step-content {
  flex: 1;
  padding-left: 16px;
  min-width: 0;
}

.step-title {
  font-size: 0.9375rem;
  font-weight: 500;
  color: rgb(var(--v-theme-on-surface));
  line-height: 32px;
}

.step-title.title-pending {
  color: rgba(var(--v-theme-on-surface), 0.6);
}

.step-title.title-cancelled {
  color: rgba(var(--v-theme-on-surface), 0.38);
  text-decoration: line-through;
}

.step-description {
  font-size: 0.8125rem;
  color: rgba(var(--v-theme-on-surface), 0.6);
  margin-top: 2px;
}

/* Download Card */
.download-card {
  background: rgba(var(--v-theme-on-surface), 0.04);
  border-radius: 12px;
  padding: 16px;
  margin-top: 12px;
}

.download-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.download-source {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px;
  background: rgba(var(--v-theme-secondary), 0.12);
  border-radius: 12px;
  font-size: 0.75rem;
  color: rgb(var(--v-theme-secondary));
}

.download-percent {
  font-size: 1.5rem;
  font-weight: 600;
  color: rgb(var(--v-theme-secondary));
}

.progress-track {
  height: 8px;
  background: rgba(var(--v-theme-on-surface), 0.08);
  border-radius: 4px;
  overflow: hidden;
  margin-bottom: 12px;
}

.progress-fill {
  height: 100%;
  background: linear-gradient(90deg, rgb(var(--v-theme-secondary)), #a78bfa);
  border-radius: 4px;
  transition: width 0.3s ease;
}

.download-stats {
  display: flex;
  gap: 16px;
  font-size: 0.8125rem;
  color: rgba(var(--v-theme-on-surface), 0.6);
}

.download-stats .stat {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* Error Message */
.error-message {
  background: rgba(var(--v-theme-error), 0.12);
  border-left: 3px solid rgb(var(--v-theme-error));
  padding: 12px 16px;
  border-radius: 0 12px 12px 0;
  color: rgb(var(--v-theme-error));
  font-size: 0.875rem;
  margin-top: 12px;
}

/* Actions */
.dialog-actions {
  padding: 12px 24px 20px;
}
</style>
