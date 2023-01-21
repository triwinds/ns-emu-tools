<template>
  <v-container style="overflow-y: auto" ref="page-container">
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
  name: "SimplePage",
  mounted() {
    this.$refs["page-container"].style.maxHeight = `${window.innerHeight - 80}px`
    window.onresize = () => {
      this.$refs["page-container"].style.maxHeight = `${window.innerHeight - 80}px`
    }
  },
  destroyed() {
    window.onresize = null
  }
}
</script>

<style scoped>

</style>