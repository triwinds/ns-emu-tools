<template>
<v-dialog
    v-model="dialog"
    max-width="900"
  >
    <template v-slot:activator="{ props }">
      <slot name="activator" v-bind:props="props">

      </slot>
    </template>

    <v-card>
      <dialog-title>
        更新日志
      </dialog-title>

      <div class="change-log-content" style=" padding: 15px; overflow-y: auto; max-height: 50vh">
        <slot name="content"></slot>
      </div>

      <v-divider></v-divider>

      <v-card-actions>
        <v-spacer></v-spacer>
        <v-btn
          color="primary"
          variant="text"
          @click="dialog = false"
        >
          关闭
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import {ref} from "vue";
import DialogTitle from "@/components/DialogTitle.vue";

let dialog = ref(false)
</script>

<style>
.change-log-content li {
  margin-left: 30px !important;
}
</style>
