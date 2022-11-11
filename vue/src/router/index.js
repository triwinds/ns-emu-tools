import VueRouter from "vue-router";
import Yuzu from "@/pages/Yuzu";
import Ryujinx from "@/pages/Ryujinx";
import About from "@/pages/About";
import KeysManagement from "@/pages/KeysManagement";

export default new VueRouter({
    routes: [
        {
            path: '/yuzu',
            component: Yuzu
        },{
            path: '/ryujinx',
            component: Ryujinx
        },{
            path: '/about',
            component: About
        },{
            path: '/keys',
            component: KeysManagement
        },
    ]
})
