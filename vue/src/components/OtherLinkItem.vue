<template>
  <div>
    <v-card
        elevation="2"
        outlined
        shaped
    >
      <v-card-title>
        <slot name="title"></slot>
      </v-card-title>
      <v-card-subtitle>
        <slot name="subtitle"></slot>
      </v-card-subtitle>
      <v-card-text>
        <slot name="description"></slot>
      </v-card-text>
      <v-card-actions>
        <slot name="actions"></slot>
      </v-card-actions>
    </v-card>
  </div>
</template>

<script>
export default {
  name: "OtherLinkItem"
}
</script>

<style scoped>

</style>