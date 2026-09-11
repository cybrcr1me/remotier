<script setup lang="ts">
import { SidebarTrigger, useSidebar } from '@/components/ui/sidebar'
import { BAR_HEIGHT, HEADER_SLOT_ID } from '@/lib/ui'
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
    <!--
      Where a view puts content of its own - the terminal tabs - instead of adding a bar
      beneath the header. It fills the rest of the row, so it carries the drag region too:
      the header's own attribute does not reach an element laid over it.
    -->
    <div :id="HEADER_SLOT_ID" data-tauri-drag-region class="flex min-w-0 flex-1 self-stretch" />
  </header>
</template>
