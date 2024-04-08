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
        component: () => import('@/views/YuzuMain.vue'),
      },
      {
        path: '/suyu',
        name: 'suyu',
        component: () => import('@/views/SuyuMain.vue'),
      },
      {
        path: '/ryujinx',
        name: 'ryujinx',
        component: () => import('@/views/RyujinxMain.vue'),
      },
      {
        path: '/keys',
        name: 'key',
        component: () => import('@/views/KeysManagement.vue'),
      },
      {
        path: '/yuzuCheatsManagement',
        name: 'yuzuCheatsManagement',
        component: () => import('@/views/YuzuCheatsManagement.vue'),
      },
      {
        path: '/settings',
        name: 'settings',
        component: () => import('@/views/SettingsPage.vue'),
      },
      {
        path: '/yuzuSaveManagement',
        name: 'yuzuSaveManagement',
        component: () => import('@/views/YuzuSaveManagement.vue'),
      },
      {
        path: '/about',
        name: 'about',
        component: () => import('@/views/AboutPage.vue'),
      },
      {
        path: '/cloudflareST',
        name: 'cloudflareST',
        component: () => import('@/views/CloudflareST.vue'),
      },
      {
        path: '/otherLinks',
        name: 'otherLinks',
        component: () => import('@/views/OtherLinks.vue'),
      },
      {
        path: '/faq',
        name: 'faq',
        component: () => import('@/views/FaqPage.vue'),
      },
    ],
  },
]

const router = createRouter({
  history: createWebHashHistory(process.env.BASE_URL),
  routes,
})

export default router
