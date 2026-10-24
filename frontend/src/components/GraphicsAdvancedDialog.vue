<script setup lang="ts">
import { mdiClose } from '@mdi/js'

defineProps<{ title: string; titleId: string; description: string; busy?: boolean }>()
const open = defineModel<boolean>({ default: false })
</script>

<template>
  <v-dialog
    v-model="open"
    max-width="620"
    :aria-labelledby="titleId"
    :persistent="busy"
    scrollable
  >
    <v-card class="graphics-dialog">
      <div class="graphics-dialog-heading">
        <h2 :id="titleId">
          {{ title }}
        </h2>
        <v-btn
          :icon="mdiClose"
          variant="text"
          size="small"
          :aria-label="`关闭${title}`"
          :disabled="busy"
          @click="open = false"
        />
      </div>
      <v-card-text class="graphics-dialog-body">
        <p class="graphics-dialog-description">
          {{ description }}
        </p>
        <slot />
      </v-card-text>
      <v-card-actions class="graphics-dialog-actions">
        <slot name="actions" />
        <v-spacer />
        <v-btn
          color="primary"
          variant="flat"
          :disabled="busy"
          @click="open = false"
        >
          完成
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.graphics-dialog { font-family: 'Segoe UI', 'Microsoft YaHei UI', sans-serif; border-radius: 16px !important; }
.graphics-dialog-heading { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 20px 24px 16px; border-bottom: 1px solid rgba(var(--v-theme-on-surface), .12); }
.graphics-dialog-heading h2 { font-size: 20px; font-weight: 600; line-height: 1.4; }
.graphics-dialog-body { padding: 22px 24px !important; font-size: 14px; line-height: 1.7; }
.graphics-dialog-description { color: rgba(var(--v-theme-on-surface), .72); margin-bottom: 24px; }
.graphics-dialog-actions { padding: 14px 24px; border-top: 1px solid rgba(var(--v-theme-on-surface), .12); flex-wrap: wrap; }
@media (max-width: 450px) { .graphics-dialog-heading { padding: 16px 18px; }.graphics-dialog-heading h2 { font-size: 18px; }.graphics-dialog-body { padding: 20px 18px !important; }.graphics-dialog-actions { padding: 12px 18px; } }
@media (prefers-reduced-motion: reduce) { .graphics-dialog :deep(*) { transition: none !important; animation: none !important; } }
</style>
