import Vue from 'vue'
import App from './App.vue'
import vuetify from './plugins/vuetify'
import VueRouter from "vue-router";
import router from "@/router";
import store from "@/store";
import '@/plugins/mixin'

Vue.config.productionTip = false
Vue.use(VueRouter)

const vm = new Vue({
  vuetify,
  router,
  store,
  render: h => h(App),
  created() {
    // eslint-disable-next-line no-undef
    eel.expose(appendConsoleMessage)
  }
}).$mount('#app')

function appendConsoleMessage(msg) {
  vm.$store.commit('APPEND_CONSOLE_MESSAGE', msg)
}
