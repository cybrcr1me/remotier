<script setup lang="ts">
/**
 * The bar at the top of every management view.
 *
 * Height comes from the shared `TOOLBAR_HEIGHT` so it matches the terminal tab bar exactly
 * and the content does not jump when switching views; controls inside are centred rather
 * than stretched.
 */
import { TOOLBAR_HEIGHT } from '@/lib/ui'
import { cn } from '@/lib/utils'
</script>

<template>
  <div :class="cn('flex shrink-0 items-center gap-2 border-b px-3', TOOLBAR_HEIGHT)">
    <slot />
  </div>
</template>
