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
import { navEntries } from '@/lib/nav'
import { TerminalIcon } from '@lucide/vue'
import { RouterLink, useRoute } from 'vue-router'

const route = useRoute()
</script>

<template>
  <Sidebar collapsible="icon">
    <!-- Reserved strip for the macOS window buttons; also the window drag handle. -->
    <SidebarHeader data-tauri-drag-region class="h-11 shrink-0 justify-center border-b p-0">
      <div class="flex items-center gap-2 pl-20 pr-2 group-data-[collapsible=icon]:hidden">
        <TerminalIcon class="size-4 shrink-0" />
        <span class="truncate text-sm font-medium">Remotier</span>
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

    <SidebarRail />
  </Sidebar>
</template>
