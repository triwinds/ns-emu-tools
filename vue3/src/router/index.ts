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
    ],
  },
]

const router = createRouter({
  history: createWebHistory(process.env.BASE_URL),
  routes,
})

export default router
