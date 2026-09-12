<script setup lang="ts">
import TabBar from '@/components/terminal/TabBar.vue'
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
    <!--
      The terminal tabs, rather than a bar of their own beneath this one: that cost every
      terminal a row of height, and this row held nothing but the toggle. They are here on
      every page, not only on the terminals view - the row is empty elsewhere anyway, and a
      session is worth reaching from wherever you are. The bar fills the rest of the row, so
      it carries the drag region itself: the header's attribute does not reach what covers it.
    -->
    <TabBar />
  </header>
</template>
