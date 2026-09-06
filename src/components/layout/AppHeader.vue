<script setup lang="ts">
import { SidebarTrigger, useSidebar } from '@/components/ui/sidebar'
import { BAR_HEIGHT } from '@/lib/ui'
import { cn } from '@/lib/utils'

const { state } = useSidebar()
</script>

<template>
  <header
    data-tauri-drag-region
    :class="cn(
      'flex shrink-0 items-center gap-2 border-b px-2',
      BAR_HEIGHT,
      // Collapsed, the 3rem rail no longer covers the macOS window buttons,
      // so the trigger has to step out of their way.
      state === 'collapsed' && 'pl-8',
    )"
  >
    <SidebarTrigger />
  </header>
</template>
