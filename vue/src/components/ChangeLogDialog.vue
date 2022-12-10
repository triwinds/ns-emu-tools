<template>
  <v-dialog
    v-model="dialog"
    max-width="900"
  >
    <template v-slot:activator="{ on, attrs }">
      <slot name="activator" v-bind:on="on" v-bind:attrs="attrs">

      </slot>
    </template>

    <v-card>
      <v-card-title class="text-h5 primary white--text">
        更新日志
      </v-card-title>

      <div style=" padding: 15px; overflow-y: auto; max-height: 50vh">
        <slot name="content">

        </slot>
      </div>

      <v-divider></v-divider>

      <v-card-actions>
        <v-spacer></v-spacer>
        <v-btn
          color="primary"
          text
          @click="dialog = false"
        >
          关闭
        </v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script>
export default {
  name: "ChangeLogDialog",
  data() {
    return {
      dialog: false
    }
  }
}
</script>

<style scoped>

</style>