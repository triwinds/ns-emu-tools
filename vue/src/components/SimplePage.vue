<template>
  <v-container style="height: 100%">
    <v-row style="height: 100%">
      <v-spacer v-show="$vuetify.breakpoint.mdAndUp"></v-spacer>
      <v-col md="10" lg="8" xl="7">
        <slot></slot>
      </v-col>
      <v-spacer v-show="$vuetify.breakpoint.mdAndUp"></v-spacer>
    </v-row>
  </v-container>
</template>

<script>
export default {
  name: "SimplePage"
}
</script>

<style scoped>

</style>