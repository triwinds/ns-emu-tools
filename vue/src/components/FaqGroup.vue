<template>
  <v-list-group>
    <template v-slot:activator>
      <v-list-item-title><span class="text-h5 info--text"><slot name="title"></slot></span></v-list-item-title>
    </template>
    <v-list-item>
      <v-list-item-content>
        <slot name="content"></slot>
      </v-list-item-content>
    </v-list-item>

  </v-list-group>
</template>

<script>
export default {
  name: "ListGroup"
}
</script>

<style scoped>

</style>