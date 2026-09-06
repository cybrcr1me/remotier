<script setup lang="ts">
/** Scrollable page content, padded to line up with `ViewToolbar`. */
import { ScrollArea } from '@/components/ui/scroll-area'
</script>

<template>
  <ScrollArea class="min-h-0 flex-1">
    <div class="px-3 py-3">
      <slot />
    </div>
  </ScrollArea>
</template>
