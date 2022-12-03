import Vue from 'vue'
import App from './App.vue'
import vuetify from './plugins/vuetify'
import VueRouter from "vue-router";
import router from "@/router";
import store from "@/store";
import '@/plugins/mixin'

Vue.config.productionTip = false
Vue.use(VueRouter)

window.$vm = new Vue({
    vuetify,
    router,
    store,
    render: h => h(App),
    beforeCreate() {
        //事件总线
        Vue.prototype.$bus = this;
    }
}).$mount('#app')


