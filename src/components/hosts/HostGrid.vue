<script setup lang="ts">
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import type { FlatHost } from '@/lib/tree'
import { PencilIcon, PlugZapIcon, ServerIcon } from '@lucide/vue'

defineProps<{ hosts: FlatHost[] }>()

const emit = defineEmits<{
  connect: [hostId: string]
  edit: [hostId: string]
  remove: [hostId: string]
}>()
</script>

<template>
  <div class="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3">
    <ContextMenu v-for="entry in hosts" :key="entry.host.id">
      <ContextMenuTrigger as-child>
        <Card
          class="cursor-default gap-0 py-4 transition-colors hover:border-ring"
          @dblclick="emit('connect', entry.host.id)"
        >
          <CardHeader class="gap-1 px-4">
            <div class="flex items-center gap-2">
              <ServerIcon class="size-4 shrink-0 text-muted-foreground" />
              <CardTitle class="truncate text-sm">{{ entry.host.label }}</CardTitle>
            </div>
          </CardHeader>

          <CardContent class="flex flex-col gap-2 px-4 pt-2">
            <p class="truncate font-mono text-xs text-muted-foreground">
              {{ entry.host.hostname }}<span v-if="entry.host.port">:{{ entry.host.port }}</span>
            </p>

            <p v-if="entry.groupPath.length" class="truncate text-xs text-muted-foreground">
              {{ entry.groupPath.join(' / ') }}
            </p>

            <div v-if="entry.host.tags.length" class="flex flex-wrap gap-1">
              <Badge v-for="tag in entry.host.tags" :key="tag" variant="secondary">
                {{ tag }}
              </Badge>
            </div>
          </CardContent>

          <CardFooter class="gap-2 px-4 pt-3">
            <Button size="sm" variant="secondary" @click="emit('connect', entry.host.id)">
              <PlugZapIcon data-icon="inline-start" />
              Connect
            </Button>
            <Button
              size="icon"
              variant="ghost"
              class="ml-auto size-8"
              :aria-label="`Edit ${entry.host.label}`"
              @click="emit('edit', entry.host.id)"
            >
              <PencilIcon />
            </Button>
          </CardFooter>
        </Card>
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
