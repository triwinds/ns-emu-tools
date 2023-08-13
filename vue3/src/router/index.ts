// Composables
import { createRouter, createWebHistory } from 'vue-router'

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
    ],
  },
]

const router = createRouter({
  history: createWebHistory(process.env.BASE_URL),
  routes,
})

export default router
