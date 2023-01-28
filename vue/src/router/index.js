import VueRouter from "vue-router";
import Yuzu from "@/pages/Yuzu";
import Ryujinx from "@/pages/Ryujinx";
import About from "@/pages/About";
import KeysManagement from "@/pages/KeysManagement";
import Settings from "@/pages/Settings";
import YuzuCheatsManagement from "@/pages/YuzuCheatsManagement";
import FAQ from "@/pages/FAQ";
import OtherLinks from "@/pages/OtherLinks";
import CloudflareST from "@/pages/CloudflareST";

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
        },{
            path: '/settings',
            component: Settings
        },{
            path: '/yuzuCheatsManagement',
            component: YuzuCheatsManagement
        },{
            path: '/faq',
            component: FAQ
        },{
            path: '/otherLinks',
            component: OtherLinks
        },{
            path: '/cloudflareST',
            component: CloudflareST
        },
    ]
})
