// Composables
import {createRouter, createWebHashHistory} from 'vue-router'

const routes = [
  {
    path: '/',
    component: () => import('@/layouts/default/Default.vue'),
    children: [
      {
        path: '/yuzu',
        name: 'yuzu',
        component: () => import(/* webpackChunkName: "home" */ '@/views/YuzuMain.vue'),
      },
      {
        path: '/ryujinx',
        name: 'ryujinx',
        component: () => import(/* webpackChunkName: "home" */ '@/views/RyujinxMain.vue'),
      },
      {
        path: '/keys',
        name: 'key',
        component: () => import(/* webpackChunkName: "home" */ '@/views/KeysManagement.vue'),
      },
      {
        path: '/yuzuCheatsManagement',
        name: 'yuzuCheatsManagement',
        component: () => import(/* webpackChunkName: "home" */ '@/views/YuzuCheatsManagement.vue'),
      },
      {
        path: '/settings',
        name: 'settings',
        component: () => import(/* webpackChunkName: "home" */ '@/views/SettingsPage.vue'),
      },
      {
        path: '/yuzuSaveManagement',
        name: 'yuzuSaveManagement',
        component: () => import(/* webpackChunkName: "home" */ '@/views/YuzuSaveManagement.vue'),
      },
      {
        path: '/about',
        name: 'about',
        component: () => import(/* webpackChunkName: "home" */ '@/views/AboutPage.vue'),
      },
      {
        path: '/cloudflareST',
        name: 'cloudflareST',
        component: () => import(/* webpackChunkName: "home" */ '@/views/CloudflareST.vue'),
      },
      {
        path: '/otherLinks',
        name: 'otherLinks',
        component: () => import(/* webpackChunkName: "home" */ '@/views/OtherLinks.vue'),
      },
      {
        path: '/faq',
        name: 'faq',
        component: () => import(/* webpackChunkName: "home" */ '@/views/FaqPage.vue'),
      },
    ],
  },
]

const router = createRouter({
  history: createWebHashHistory(process.env.BASE_URL),
  routes,
})

export default router
