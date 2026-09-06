<script setup lang="ts">
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { colorBorder, hasColor, hostIcon } from '@/lib/appearance'
import type { FlatHost } from '@/lib/tree'
import { cn } from '@/lib/utils'
import { PencilIcon, PlugZapIcon } from '@lucide/vue'

defineProps<{ hosts: FlatHost[] }>()

const emit = defineEmits<{
  connect: [hostId: string]
  edit: [hostId: string]
  remove: [hostId: string]
}>()
</script>

<template>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3">
    <ContextMenu v-for="entry in hosts" :key="entry.host.id">
      <ContextMenuTrigger as-child>
        <div
          :class="cn(
            'group relative flex cursor-default flex-col gap-3 rounded-xl border bg-card p-4',
            'transition-colors hover:border-ring',
            colorBorder(entry.host.color),
          )"
          @dblclick="emit('connect', entry.host.id)"
        >
          <div class="flex items-start gap-3">
            <span
              :class="cn(
                'flex size-9 shrink-0 items-center justify-center rounded-lg border bg-muted/60',
                hasColor(entry.host.color) && colorBorder(entry.host.color),
              )"
            >
              <component :is="hostIcon(entry.host.icon)" class="size-4" />
            </span>

            <div class="flex min-w-0 flex-1 flex-col">
              <p class="truncate text-sm font-medium">{{ entry.host.label }}</p>
              <p class="truncate font-mono text-xs text-muted-foreground">
                {{ entry.host.hostname }}<span v-if="entry.host.port">:{{ entry.host.port }}</span>
              </p>
            </div>

            <Tooltip>
              <TooltipTrigger as-child>
                <Button
                  size="icon"
                  variant="ghost"
                  class="size-7 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                  :aria-label="`Edit ${entry.host.label}`"
                  @click="emit('edit', entry.host.id)"
                >
                  <PencilIcon />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Edit</TooltipContent>
            </Tooltip>
          </div>

          <div v-if="entry.groupPath.length || entry.host.tags.length" class="flex flex-wrap items-center gap-1">
            <span v-if="entry.groupPath.length" class="truncate text-xs text-muted-foreground">
              {{ entry.groupPath.join(' / ') }}
            </span>
            <Badge v-for="tag in entry.host.tags" :key="tag" variant="secondary">
              {{ tag }}
            </Badge>
          </div>

          <Button size="sm" variant="secondary" class="w-full" @click="emit('connect', entry.host.id)">
            <PlugZapIcon data-icon="inline-start" />
            Connect
          </Button>
        </div>
      </ContextMenuTrigger>

      <ContextMenuContent>
        <ContextMenuItem @select="emit('connect', entry.host.id)">Connect</ContextMenuItem>
        <ContextMenuItem @select="emit('edit', entry.host.id)">Edit host</ContextMenuItem>
        <ContextMenuItem variant="destructive" @select="emit('remove', entry.host.id)">
          Delete host
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  </div>
</template>
