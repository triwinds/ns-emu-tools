<template>
  <v-list-group>
    <template v-slot:activator="{props}">
      <v-list-item v-bind="props" title="">
        <span class="text-info" style="font-size: 22px">
        <slot name="title"></slot>
        </span>
      </v-list-item>
    </template>
    <v-list-item title="">
      <slot name="content"></slot>
    </v-list-item>

  </v-list-group>
</template>

<script setup lang="ts">

</script>

<style scoped>

</style>
