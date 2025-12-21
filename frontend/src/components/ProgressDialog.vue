<template>
  <v-dialog v-model="store.dialogOpen" max-width="600" persistent>
    <v-card>
      <dialog-title>
        进度
      </dialog-title>
      <v-divider></v-divider>

      <v-card-text class="pa-0">
        <v-list class="py-0">
            <template v-for="(step, index) in store.steps" :key="step.id">
                <v-list-item>
                    <template v-slot:prepend>
                        <div class="mr-4 d-flex align-center justify-center" style="width: 24px; height: 24px;">
                            <!-- Running -->
                            <v-progress-circular
                                v-if="step.status === 'running'"
                                indeterminate
                                color="primary"
                                size="20"
                                width="2"
                            ></v-progress-circular>

                            <!-- Success -->
                            <v-icon
                                v-else-if="step.status === 'success'"
                                color="success"
                                :icon="mdiCheckCircle"
                            ></v-icon>

                             <!-- Error -->
                             <v-icon
                                v-else-if="step.status === 'error'"
                                color="error"
                                :icon="mdiCloseCircle"
                            ></v-icon>

                            <!-- Cancelled -->
                            <v-icon
                                v-else-if="step.status === 'cancelled'"
                                color="grey"
                                :icon="mdiMinusCircle"
                            ></v-icon>

                            <!-- Pending -->
                            <v-icon
                                v-else
                                color="grey"
                                :icon="mdiCircleOutline"
                                size="small"
                            ></v-icon>
                        </div>
                    </template>

                    <v-list-item-title
                        class="text-body-1 font-weight-medium"
                        :class="{ 'text-decoration-line-through text-grey': step.status === 'cancelled' }"
                    >
                        {{ step.title }}
                    </v-list-item-title>
                    <v-list-item-subtitle v-if="step.description">
                        {{ step.description }}
                    </v-list-item-subtitle>

                    <!-- Error message -->
                    <div v-if="step.status === 'error' && step.error" class="mt-2">
                        <v-alert
                            type="error"
                            density="compact"
                            variant="tonal"
                            :text="step.error"
                        ></v-alert>
                    </div>

                    <!-- Download Progress details -->
                    <div v-if="step.type === 'download' && step.status === 'running'" class="mt-2">
                         <v-progress-linear
                            :model-value="step.progress"
                            color="primary"
                            height="6"
                            rounded
                            striped
                        ></v-progress-linear>
                        <div class="d-flex justify-space-between text-caption mt-1 text-grey">
                            <span>{{ step.progress?.toFixed(1) }}%</span>
                            <span>{{ step.downloadSpeed }} - ETA: {{ step.eta }}</span>
                        </div>
                    </div>
                </v-list-item>
                <v-divider v-if="index < store.steps.length - 1"></v-divider>
            </template>
        </v-list>
      </v-card-text>

      <v-divider></v-divider>

      <v-card-actions>
        <v-spacer></v-spacer>
        <!-- Cancel button when downloading -->
        <v-btn
          v-if="isDownloading"
          color="error"
          variant="text"
          @click="handleCancelDownload"
          :disabled="isCancelling"
        >
          {{ isCancelling ? '取消中...' : '取消下载' }}
        </v-btn>
        <!-- Only show close when finished (success or error on last step, or generally if not running?
             For now, let's just allow close if all done or error)
        -->
        <v-btn
          color="primary"
          variant="text"
          @click="store.closeDialog()"
          :disabled="isInstalling"
        >
          Close
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { useProgressStore } from '@/stores/ProgressStore';
import { computed, ref } from 'vue';
import { mdiCheckCircle, mdiCloseCircle, mdiCircleOutline, mdiMinusCircle } from '@mdi/js';
import { cancelYuzuDownload, cancelRyujinxDownload } from '@/utils/tauri';
import DialogTitle from '@/components/DialogTitle.vue';

const store = useProgressStore();
const isCancelling = ref(false);

const isInstalling = computed(() => {
    // Simple check: if any step is running, we are installing.
    // Or if the last step is pending.
    return store.steps.some(s => s.status === 'running');
});

const isDownloading = computed(() => {
    // Check if there's a download step that is currently running
    return store.steps.some(s => s.type === 'download' && s.status === 'running');
});

// Detect which emulator is being installed based on download step title
const downloadingEmulator = computed(() => {
    const downloadStep = store.steps.find(s => s.type === 'download' && s.status === 'running');
    if (!downloadStep) return null;

    if (downloadStep.title.includes('Ryujinx')) {
        return 'ryujinx';
    } else if (downloadStep.title.includes('Eden') || downloadStep.title.includes('Citron') || downloadStep.title.includes('Yuzu')) {
        return 'yuzu';
    }
    return null;
});

async function handleCancelDownload() {
    if (isCancelling.value) return;

    isCancelling.value = true;
    try {
        if (downloadingEmulator.value === 'ryujinx') {
            await cancelRyujinxDownload();
        } else {
            await cancelYuzuDownload();
        }
    } catch (error) {
        console.error('取消下载失败:', error);
    } finally {
        isCancelling.value = false;
    }
}
</script>
