import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  { path: '/', redirect: '/terminals' },
  { path: '/terminals', name: 'terminals', component: () => import('@/views/TerminalsView.vue') },
  { path: '/hosts', name: 'hosts', component: () => import('@/views/HostsView.vue') },
  { path: '/keychain', name: 'keychain', component: () => import('@/views/KeychainView.vue') },
  { path: '/identities', name: 'identities', component: () => import('@/views/IdentitiesView.vue') },
  { path: '/known-hosts', name: 'known-hosts', component: () => import('@/views/KnownHostsView.vue') },
  { path: '/settings', name: 'settings', component: () => import('@/views/SettingsView.vue') },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})
