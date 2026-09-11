<script setup lang="ts">
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { cn } from '@/lib/utils'
import type { TreeNode } from '@/lib/tree'
import { colorBorder, hasColor } from '@/lib/appearance'
import EntityIcon from './EntityIcon.vue'
import { ChevronRightIcon } from '@lucide/vue'

defineOptions({ name: 'HostTree' })

const props = defineProps<{
  nodes: TreeNode[]
  expanded: Set<string>
  selectedId: string | null
  depth?: number
}>()

const emit = defineEmits<{
  toggle: [groupId: string]
  select: [node: TreeNode]
  connect: [hostId: string]
  edit: [node: TreeNode]
  remove: [node: TreeNode]
  addHost: [groupId: string]
  addGroup: [parentId: string]
}>()

const depth = props.depth ?? 0
</script>

<template>
  <ul class="flex flex-col gap-0.5">
    <li v-for="node in props.nodes" :key="node.id">
      <ContextMenu>
        <ContextMenuTrigger as-child>
          <div
            :class="cn(
              'flex h-8 cursor-default items-center gap-2 rounded-md pr-2 text-sm',
              props.selectedId === node.id ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/50',
            )"
            :style="{ paddingLeft: `${depth * 12 + 8}px` }"
            @click="emit('select', node)"
            @dblclick="node.kind === 'host' && emit('connect', node.host.id)"
          >
            <template v-if="node.kind === 'group'">
              <Button
                variant="ghost"
                size="icon"
                class="size-5 shrink-0"
                :aria-label="props.expanded.has(node.id) ? `Collapse ${node.name}` : `Expand ${node.name}`"
                @click.stop="emit('toggle', node.id)"
              >
                <ChevronRightIcon
                  :class="cn('transition-transform', props.expanded.has(node.id) && 'rotate-90')"
                />
              </Button>
              <EntityIcon
                :icon="node.group.icon"
                kind="group"
                :class="cn(
                  'size-4 shrink-0 text-muted-foreground',
                  hasColor(node.group.color) && 'text-foreground',
                )"
              />
              <span
                :class="cn(
                  'truncate',
                  hasColor(node.group.color) && `border-l-2 pl-2 ${colorBorder(node.group.color)}`,
                )"
              >{{ node.name }}</span>
            </template>

            <template v-else>
              <span class="w-5 shrink-0" />
              <EntityIcon :icon="node.host.icon" class="size-4 shrink-0 text-muted-foreground" />
              <span
                :class="cn(
                  'truncate',
                  hasColor(node.host.color) && `border-l-2 pl-2 ${colorBorder(node.host.color)}`,
                )"
              >{{ node.host.label }}</span>
              <span class="ml-auto truncate text-xs text-muted-foreground">
                {{ node.host.hostname }}
              </span>
              <Badge v-for="tag in node.host.tags" :key="tag" variant="secondary">
                {{ tag }}
              </Badge>
            </template>
          </div>
        </ContextMenuTrigger>

        <ContextMenuContent>
          <template v-if="node.kind === 'host'">
            <ContextMenuItem @select="emit('connect', node.host.id)">
              Connect
            </ContextMenuItem>
            <ContextMenuItem @select="emit('edit', node)">
              Edit host
            </ContextMenuItem>
            <ContextMenuItem variant="destructive" @select="emit('remove', node)">
              Delete host
            </ContextMenuItem>
          </template>
          <template v-else>
            <ContextMenuItem @select="emit('addHost', node.id)">
              New host here
            </ContextMenuItem>
            <ContextMenuItem @select="emit('addGroup', node.id)">
              New subgroup
            </ContextMenuItem>
            <ContextMenuItem @select="emit('edit', node)">
              Edit group
            </ContextMenuItem>
            <ContextMenuItem variant="destructive" @select="emit('remove', node)">
              Delete group
            </ContextMenuItem>
          </template>
        </ContextMenuContent>
      </ContextMenu>

      <HostTree
        v-if="node.kind === 'group' && props.expanded.has(node.id)"
        :nodes="node.children"
        :expanded="props.expanded"
        :selected-id="props.selectedId"
        :depth="depth + 1"
        @toggle="id => emit('toggle', id)"
        @select="n => emit('select', n)"
        @connect="id => emit('connect', id)"
        @edit="n => emit('edit', n)"
        @remove="n => emit('remove', n)"
        @add-host="id => emit('addHost', id)"
        @add-group="id => emit('addGroup', id)"
      />
    </li>
  </ul>
</template>
