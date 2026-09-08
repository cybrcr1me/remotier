<script setup lang="ts">
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from '@/components/ui/sidebar'
import BrandMark from '@/components/layout/BrandMark.vue'
import { navEntries } from '@/lib/nav'
import { BAR_HEIGHT } from '@/lib/ui'
import { cn } from '@/lib/utils'
import { RouterLink, useRoute } from 'vue-router'

const route = useRoute()
</script>

<template>
  <Sidebar collapsible="icon">
    <!-- Reserved strip for the macOS window buttons; also the window drag handle. -->
    <SidebarHeader
      data-tauri-drag-region
      :class="cn('shrink-0 justify-center border-b p-0', BAR_HEIGHT)"
    >
      <div class="flex items-center gap-2 pl-20 pr-2 group-data-[collapsible=icon]:hidden">
        <BrandMark class="size-4 shrink-0" />
        <!-- Wordmark: Martian Mono 700, uppercase, tracking -3.5%. Set nowhere else. -->
        <span class="truncate font-display text-sm font-bold uppercase tracking-wordmark">Remotier</span>
      </div>
    </SidebarHeader>

    <SidebarContent>
      <SidebarGroup>
        <SidebarGroupContent>
          <SidebarMenu>
            <SidebarMenuItem v-for="entry in navEntries" :key="entry.to">
              <SidebarMenuButton as-child :is-active="route.path.startsWith(entry.to)" :tooltip="entry.label">
                <RouterLink :to="entry.to">
                  <component :is="entry.icon" />
                  <span>{{ entry.label }}</span>
                </RouterLink>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>
    </SidebarContent>

    <SidebarRail class="" />
  </Sidebar>
</template>
