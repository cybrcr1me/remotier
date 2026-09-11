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
  SidebarFooter,
  SidebarRail,
} from '@/components/ui/sidebar'
import BrandMark from '@/components/layout/BrandMark.vue'
import { navEntries } from '@/lib/nav'
import { BAR_HEIGHT } from '@/lib/ui'
import { cn } from '@/lib/utils'
import { instanceLabel, summarise } from '@/lib/sync-status'
import { settingsRoute } from '@/lib/settings-tabs'
import { useSyncStore } from '@/stores/sync'
import { computed } from 'vue'
import { RouterLink, useRoute } from 'vue-router'

const route = useRoute()
const sync = useSyncStore()

/**
 * The connection-state convention: a 6px square, never a pill or a badge. The colour is
 * the whole message, so the tooltip carries the words.
 */
const squareClass = computed(() => {
  switch (sync.indicator) {
    case 'synced':
      return 'bg-[var(--rm-ok)]'
    case 'syncing':
      return 'bg-[var(--rm-warn)]'
    case 'error':
      return 'bg-[var(--rm-error)]'
    default:
      return 'bg-[var(--rm-idle)]'
  }
})

const label = computed(() =>
  sync.status.signedIn ? instanceLabel(sync.status.instanceUrl) : 'Sync off',
)
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
              <!-- The current page is marked in lime; the stock accent text barely differs from an idle item. -->
              <SidebarMenuButton
                as-child
                :is-active="route.path.startsWith(entry.to)"
                :tooltip="entry.label"
                class="data-active:text-primary"
              >
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

    <SidebarFooter>
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton as-child :tooltip="summarise(sync.status)" size="sm">
            <RouterLink :to="settingsRoute('sync')">
              <span class="size-1.5 shrink-0" :class="squareClass" />
              <span class="truncate font-mono text-xs">{{ label }}</span>
            </RouterLink>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </SidebarFooter>

    <SidebarRail class="" />
  </Sidebar>
</template>
