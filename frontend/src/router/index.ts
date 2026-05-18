import { createRouter, createWebHistory } from 'vue-router'
import DevicesView from '@/views/DevicesView.vue'
import MediaView from '@/views/MediaView.vue'
import ScenesView from '@/views/ScenesView.vue'
import AboutView from '@/views/AboutView.vue'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/', redirect: '/devices' },
    { path: '/devices', name: 'devices', component: DevicesView },
    { path: '/media', name: 'media', component: MediaView },
    { path: '/scenes', name: 'scenes', component: ScenesView },
    { path: '/about', name: 'about', component: AboutView },
  ],
})

export default router
