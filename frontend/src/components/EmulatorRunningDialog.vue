<template>
  <v-dialog
    v-model="isOpen"
    max-width="440"
    persistent
    style="z-index: 2500"
  >
    <v-card class="emulator-running-dialog">
      <v-card-title class="dialog-title">
        <span class="warning-mark">
          <v-icon :icon="mdiAlertOutline" size="20" />
        </span>
        <span>请关闭 {{ emulatorName }}</span>
        <v-btn
          class="close-button"
          icon
          size="small"
          variant="text"
          :disabled="isResponding"
          aria-label="取消安装"
          @click="respond(false)"
        >
          <v-icon :icon="mdiClose" size="20" />
        </v-btn>
      </v-card-title>

      <v-divider />

      <v-card-text class="dialog-content">
        <p class="dialog-message">
          检测到 <strong>{{ emulatorName }}</strong> 仍在运行。
        </p>
        <p class="dialog-hint">
          请关闭模拟器后选择“重新检测”，或取消本次安装。
        </p>
      </v-card-text>

      <v-divider />

      <v-card-actions class="dialog-actions">
        <v-spacer />
        <v-btn
          variant="text"
          color="secondary"
          :disabled="isResponding"
          @click="respond(false)"
        >
          取消安装
        </v-btn>
        <v-btn
          color="warning"
          variant="tonal"
          :loading="isResponding"
          :disabled="isResponding"
          @click="respond(true)"
        >
          重新检测
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { mdiAlertOutline, mdiClose } from '@mdi/js'

interface EmulatorRunningPrompt {
  emulatorName: string
  responseEvent: string
}

const isOpen = ref(false)
const isResponding = ref(false)
const emulatorName = ref('模拟器')
const responseEvent = ref('')
const appWindow = getCurrentWindow()

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<EmulatorRunningPrompt>('emulator-running', (event) => {
    if (isResponding.value) return

    emulatorName.value = event.payload.emulatorName || '模拟器'
    responseEvent.value = event.payload.responseEvent
    isOpen.value = true
  })
})

onUnmounted(() => {
  unlisten?.()
})

async function respond(retry: boolean) {
  if (isResponding.value || !responseEvent.value) return

  isResponding.value = true
  try {
    await appWindow.emit(responseEvent.value, retry)
    isOpen.value = false
    responseEvent.value = ''
  } catch (error) {
    console.error('响应模拟器运行状态检测失败:', error)
  } finally {
    isResponding.value = false
  }
}
</script>

<style scoped>
.emulator-running-dialog {
  overflow: hidden;
  border-radius: 20px !important;
  background: rgb(var(--v-theme-surface)) !important;
}

.dialog-title {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 18px 18px 16px 20px;
  color: rgb(var(--v-theme-on-surface));
  font-size: 1.1rem;
  font-weight: 600;
}

.warning-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  border: 1px solid rgba(var(--v-theme-warning), 0.42);
  border-radius: 50%;
  background: rgba(var(--v-theme-warning), 0.12);
  color: rgb(var(--v-theme-warning));
}

.close-button {
  margin-left: auto;
  color: rgba(var(--v-theme-on-surface), 0.68);
}

.dialog-content {
  padding: 22px 24px 18px;
}

.dialog-message,
.dialog-hint {
  margin: 0;
  line-height: 1.65;
}

.dialog-message {
  color: rgb(var(--v-theme-on-surface));
  font-size: 0.98rem;
}

.dialog-message strong {
  color: rgb(var(--v-theme-warning));
  font-weight: 600;
}

.dialog-hint {
  margin-top: 8px;
  color: rgba(var(--v-theme-on-surface), 0.64);
  font-size: 0.875rem;
}

.dialog-actions {
  gap: 8px;
  padding: 12px 18px 16px;
}
</style>
