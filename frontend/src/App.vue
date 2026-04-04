<template>
  <v-snackbar
      v-model="appNotice.visible"
      :color="appNotice.color"
      :timeout="appNotice.timeout"
      location="top"
  >
    <v-icon :icon="appNotice.prependIcon" class="mr-2" />{{ appNotice.content }}
    <template #actions>
      <v-btn text="关闭" @click="appNotice.visible = false"></v-btn>
    </template>
  </v-snackbar>
  <router-view />
  <ProgressDialog />
</template>


<script lang="ts" setup>
import { onMounted, onUnmounted, ref } from "vue";
import { useConsoleDialogStore } from "@/stores/ConsoleDialogStore";
import { useProgressStore } from "@/stores/ProgressStore"; // Import store
import ProgressDialog from "@/components/ProgressDialog.vue"; // Import component
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event'; // Import listen
import { ask } from '@tauri-apps/plugin-dialog';
import { deletePath, takePendingGithubMirrorFallbackNotice, type AppNoticeMessage, type NotifyMessage } from '@/utils/tauri';
import { openUrlWithDefaultBrowser } from '@/utils/common';
import { useEmitter } from '@/plugins/mitt';

const cds = useConsoleDialogStore()
const progressStore = useProgressStore() // Init store
const emitter = useEmitter()
let pendingWriteSize = false
let appWindow: any = null
let unlistenInstallation: any = null; // Store unlisten function
let unlistenLogMessage: any = null; // Store unlisten function for log messages
let unlistenNotifyMessage: any = null; // Store unlisten function for notify messages
const appNotice = ref({
  visible: false,
  content: '',
  color: 'info',
  prependIcon: '$info',
  timeout: 3000,
})

function notifyColor(type?: string) {
  switch (type) {
    case 'success':
      return 'success'
    case 'warning':
      return 'warning'
    case 'error':
      return 'error'
    default:
      return 'info'
  }
}

function notifyIcon(type?: string) {
  switch (type) {
    case 'success':
      return '$success'
    case 'warning':
      return '$warning'
    case 'error':
      return '$error'
    default:
      return '$info'
  }
}

function notifyTimeout(type?: string, persistent?: boolean) {
  if (type === 'error') {
    return persistent ? 4500 : 3000
  }

  if (persistent) {
    return 7000
  }

  return 3000
}

function showAppNotice(message: AppNoticeMessage | NotifyMessage | null | undefined) {
  if (!message?.content) {
    return
  }

  appNotice.value = {
    visible: true,
    content: message.content,
    color: notifyColor(message.type),
    prependIcon: notifyIcon(message.type),
    timeout: notifyTimeout(message.type, message.persistent),
  }
}

function handleAppNotice(message: unknown) {
  showAppNotice(message as AppNoticeMessage | NotifyMessage | null | undefined)
}

function handleLinkClick(e: MouseEvent) {
  const target = (e.target as HTMLElement).closest('a')
  if (!target) return
  const href = target.getAttribute('href')
  if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
    e.preventDefault()
    openUrlWithDefaultBrowser(href)
  }
}

try {
  appWindow = getCurrentWindow()
} catch (e) {
  console.log('Running in browser mode, Tauri APIs unavailable')
}

onMounted(async () => {
  window.addEventListener('resize', rememberWindowSize);
  document.addEventListener('click', handleLinkClick);
  emitter.on('showNotifyMessage', handleAppNotice);

  // Listen for installation events
  try {
      unlistenInstallation = await listen('installation-event', (event: any) => {
          const payload = event.payload;
          console.log('Installation Event:', payload);

          switch (payload.type) {
              case 'started':
                  progressStore.reset();
                  progressStore.setSteps(payload.steps);
                  progressStore.openDialog();
                  break;
              case 'stepUpdate':
                  // New unified event - simply update the step
                  progressStore.updateStep(payload.step);
                  break;
              case 'finished':
                  // 如果有成功消息（比如固件版本号），更新最后一个步骤的 title
                  if (payload.success && payload.message) {
                      const lastStep = progressStore.steps[progressStore.steps.length - 1];
                      if (lastStep) {
                          lastStep.title = payload.message;
                      }
                  }
                  break;
              case 'corruptedFile':
                  handleCorruptedFile(payload.path);
                  break;
          }
      });
  } catch (e) {
      console.error('Failed to setup installation event listener', e);
  }

  // Listen for log messages
  try {
      unlistenLogMessage = await listen('log-message', (event: any) => {
          const message = event.payload;
          if (message) {
              cds.appendConsoleMessage(message);
          }
      });
  } catch (e) {
      console.error('Failed to setup log message listener', e);
  }

    // Listen for notify messages (global error/info)
    try {
      unlistenNotifyMessage = await listen('notify-message', (event: any) => {
        const message = event.payload as any;
        if (message?.content) {
          cds.appendConsoleMessage(message.content);
          showAppNotice(message as NotifyMessage)
        }
      });
    } catch (e) {
      console.error('Failed to setup notify message listener', e);
    }

    try {
      const pendingNotice = await takePendingGithubMirrorFallbackNotice()
      if (pendingNotice) {
        showAppNotice({
          type: 'warning',
          content: pendingNotice.message,
          persistent: true,
        })
      }
    } catch (e) {
      console.error('Failed to load pending github mirror fallback notice', e)
    }
})

onUnmounted(() => {
  window.removeEventListener('resize', rememberWindowSize);
  document.removeEventListener('click', handleLinkClick);
  emitter.off('showNotifyMessage', handleAppNotice);
  if (unlistenInstallation) {
      unlistenInstallation();
  }
  if (unlistenLogMessage) {
      unlistenLogMessage();
  }
  if (unlistenNotifyMessage) {
      unlistenNotifyMessage();
  }
})

async function handleCorruptedFile(filePath: string) {
  const shouldDelete = await ask(
      '下载的文件可能已损坏，是否删除该文件以便重新下载？',
      { title: '文件损坏', kind: 'warning' }
  );
  if (shouldDelete) {
      try {
          await deletePath(filePath);
      } catch (e) {
          console.error('删除损坏文件失败:', e);
      }
  }
}

async function rememberWindowSize() {
  if (!pendingWriteSize && appWindow) {
    pendingWriteSize = true
    setTimeout(async () => {
      pendingWriteSize = false
      try {
        const size = await appWindow.outerSize()
        // Note: Tauri automatically saves window state, but we can also manually update settings if needed
        console.log('Window resized:', size.width, size.height)
      } catch (error) {
        console.error('Failed to get window size:', error)
      }
    }, 1000)
  }
}
</script>

<style>
html ::-webkit-scrollbar {
  width: 0 ;
  height: 0 ;
}
div::-webkit-resizer, div::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar {
  width: 5px !important;
  height: 5px !important;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}

div::-webkit-resizer, div ::-webkit-scrollbar-thumb {
  background: #aaa;
  border-radius: 3px;
}

div::-webkit-scrollbar-corner, div ::-webkit-scrollbar-track {
  background: transparent !important;
}

a {
  cursor: pointer;
}
</style>
