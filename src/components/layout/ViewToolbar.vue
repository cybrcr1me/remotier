<script setup lang="ts">
/**
 * The bar at the top of every management view.
 *
 * Height comes from the shared `BAR_HEIGHT` so it matches the window header and the
 * terminal tab bar exactly; controls inside are centred rather than stretched.
 */
import { BAR_HEIGHT } from '@/lib/ui'
import { cn } from '@/lib/utils'
</script>

<template>
  <div :class="cn('flex shrink-0 items-center gap-2 border-b px-3', BAR_HEIGHT)">
    <slot />
  </div>
</template>
