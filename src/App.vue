<script setup lang="ts">
import AppHeader from '@/components/layout/AppHeader.vue'
import AppSidebar from '@/components/layout/AppSidebar.vue'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { Toaster } from '@/components/ui/sonner'
import { bootstrap } from '@/lib/bootstrap'
import { onMounted } from 'vue'

onMounted(bootstrap)
</script>

<template>
  <SidebarProvider class="h-svh min-h-0">
    <AppSidebar />
    <SidebarInset class="min-h-0 overflow-hidden">
      <AppHeader />
      <main class="min-h-0 flex-1 overflow-hidden">
        <RouterView />
      </main>
    </SidebarInset>
    <Toaster />
  </SidebarProvider>
</template>
