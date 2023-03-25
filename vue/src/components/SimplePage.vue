<template>
  <v-container style="overflow-y: auto; padding-bottom: 0" ref="page-container">
    <v-row style="height: 100%; margin-bottom: 0">
      <v-spacer v-show="$vuetify.breakpoint.mdAndUp"></v-spacer>
      <v-col md="11" lg="9" xl="8" style="padding-bottom: 0">
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
    this.$refs["page-container"].style.maxHeight = `${window.innerHeight - 70}px`
    window.onresize = () => {
      this.$refs["page-container"].style.maxHeight = `${window.innerHeight - 70}px`
    }
  },
  destroyed() {
    window.onresize = null
  }
}
</script>

<style scoped>

</style>