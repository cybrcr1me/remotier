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
import { colorBorder, hasColor } from '@/lib/appearance'
import EntityIcon from './EntityIcon.vue'
import { summarise, type FlatHost, type GroupNode } from '@/lib/tree'
import { cn } from '@/lib/utils'
import { ChevronRightIcon, PencilIcon, PlugZapIcon } from '@lucide/vue'

defineProps<{
  /** Groups at the level being browsed, shown as folders. */
  groups: GroupNode[]
  hosts: FlatHost[]
}>()

const emit = defineEmits<{
  open: [groupId: string]
  connect: [hostId: string]
  edit: [hostId: string]
  remove: [hostId: string]
  editGroup: [groupId: string]
  removeGroup: [groupId: string]
}>()

/** "3 hosts · 1 group", with the empty case said plainly rather than as two zeroes. */
function contents(node: GroupNode): string {
  const { hosts, groups } = summarise(node)
  if (hosts === 0 && groups === 0) return 'Empty'

  const parts: string[] = []
  if (hosts > 0) parts.push(`${hosts} host${hosts === 1 ? '' : 's'}`)
  if (groups > 0) parts.push(`${groups} group${groups === 1 ? '' : 's'}`)
  return parts.join(' · ')
}
</script>

<template>
  <!--
    Two categories, groups first, the way a file browser puts folders above files: the level
    reads as somewhere you can go before it reads as a list of things you can connect to.
    Each has its own grid, so the first host starts a row of its own instead of trailing on
    after the last folder.
  -->
  <div class="flex flex-col gap-6">
    <section v-if="groups.length" class="flex flex-col gap-3">
      <h2 class="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
        Groups
      </h2>

      <div class="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-3">
        <ContextMenu v-for="node in groups" :key="node.id">
          <ContextMenuTrigger as-child>
            <!--
              A div rather than a button: the card carries an edit button of its own, and a
              button inside a button is invalid and does not reliably fire. Keyboard
              activation is wired by hand to keep what the element loses.
            -->
            <div
              role="button"
              tabindex="0"
              :class="cn(
                'group relative flex cursor-pointer flex-col gap-3 rounded-xl border bg-card p-4 text-left',
                'transition-colors hover:border-ring focus-visible:border-ring focus-visible:outline-none',
                colorBorder(node.group.color),
              )"
              :aria-label="`Open ${node.name}`"
              @click="emit('open', node.id)"
              @keydown.enter.prevent="emit('open', node.id)"
              @keydown.space.prevent="emit('open', node.id)"
            >
              <div class="flex items-start gap-3">
                <span
                  :class="cn(
                    'flex size-9 shrink-0 items-center justify-center rounded-lg border bg-muted/60',
                    hasColor(node.group.color) && colorBorder(node.group.color),
                  )"
                >
                  <EntityIcon :icon="node.group.icon" kind="group" class="size-4" />
                </span>

                <div class="flex min-w-0 flex-1 flex-col">
                  <p class="truncate text-sm font-medium">{{ node.name }}</p>
                  <p class="truncate text-xs text-muted-foreground">{{ contents(node) }}</p>
                </div>

                <!--
                  Same affordance the host cards have. A context menu alone is not
                  discoverable: nothing on the card said a group could be edited at all.
                -->
                <Tooltip>
                  <TooltipTrigger as-child>
                    <Button
                      size="icon"
                      variant="ghost"
                      class="size-7 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                      :aria-label="`Edit ${node.name}`"
                      @click.stop="emit('editGroup', node.id)"
                    >
                      <PencilIcon />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Edit group</TooltipContent>
                </Tooltip>

                <ChevronRightIcon class="size-4 shrink-0 text-muted-foreground" />
              </div>
            </div>
          </ContextMenuTrigger>

          <ContextMenuContent>
            <ContextMenuItem @select="emit('open', node.id)">Open group</ContextMenuItem>
            <ContextMenuItem @select="emit('editGroup', node.id)">Edit group</ContextMenuItem>
            <ContextMenuItem variant="destructive" @select="emit('removeGroup', node.id)">
              Delete group
            </ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      </div>
    </section>

    <section v-if="hosts.length" class="flex flex-col gap-3">
      <h2 class="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
        Hosts
      </h2>

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
                  <EntityIcon :icon="entry.host.icon" class="size-4" />
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

              <!--
                The path only appears on search results, which can come from anywhere below
                the folder being browsed. While browsing, the breadcrumb already says where
                you are.
              -->
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
    </section>
  </div>
</template>
