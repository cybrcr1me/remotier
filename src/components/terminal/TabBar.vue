<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { insertionIndex, type Rect } from '@/lib/dnd'
import { beginDrag, dragging, endDrag } from '@/lib/drag'
import { colorBorder, hasColor } from '@/lib/appearance'
import { tabColor, tabTitle } from '@/lib/tab-title'
import { BAR_HEIGHT } from '@/lib/ui'
import { useInventoryStore } from '@/stores/inventory'
import { cn } from '@/lib/utils'
import { useSessionsStore } from '@/stores/sessions'
import WorkspaceMenu from './WorkspaceMenu.vue'
import { PlusIcon, XIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { ref } from 'vue'

const sessions = useSessionsStore()
const inventory = useInventoryStore()
const { tabs, activeTabId } = storeToRefs(sessions)

/*
 * Titles are derived from the panes a tab currently holds, not stored on the tab. Merging
 * two tabs would otherwise leave the survivor advertising only the drop target's host.
 */
function titleOf(tab: { layout: Parameters<typeof tabTitle>[0], name: string }) {
  return tabTitle(tab.layout, id => inventory.hostById.get(id)?.label ?? null, tab.name)
}

/*
 * Every tab carries a border whether or not its host has a colour, so a coloured tab does
 * not sit a pixel taller than its neighbours.
 */
function borderOf(tab: { layout: Parameters<typeof tabTitle>[0], activePaneId: string }) {
  const color = tabColor(
    tab.layout,
    tab.activePaneId,
    id => inventory.hostById.get(id)?.color ?? null,
  )
  return hasColor(color) ? colorBorder(color) : 'border-transparent'
}

const emit = defineEmits<{ newTab: [] }>()

const strip = ref<HTMLDivElement | null>(null)
/** Where a dropped tab would land, or `null` when nothing is being dragged over the bar. */
const dropIndex = ref<number | null>(null)

function onDragStart(tabId: string, event: DragEvent) {
  beginDrag({ kind: 'tab', tabId }, event)
}

function onDragEnd() {
  endDrag()
  dropIndex.value = null
}

/** The tab elements' rectangles, in bar order. */
function tabRects(): Rect[] {
  const children = strip.value ? [...strip.value.children] : []
  return children.map((child) => {
    const box = child.getBoundingClientRect()
    return { left: box.left, top: box.top, width: box.width, height: box.height }
  })
}

function onDragOver(event: DragEvent) {
  // Only tabs reorder here. A pane dragged over the bar gets no caret, because dropping
  // it would do nothing and an indicator that promises otherwise is worse than none.
  if (dragging.value?.kind !== 'tab') return

  // Without this the browser refuses the drop and plays the "snap back" animation.
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropIndex.value = insertionIndex(tabRects(), event.clientX)
}

/*
 * `dragleave` fires every time the pointer crosses into a child tab, so clearing the
 * caret on any of them would make it flicker across the whole bar. Only a move to
 * somewhere outside the strip counts as leaving.
 */
function onDragLeave(event: DragEvent) {
  const next = event.relatedTarget as Node | null
  if (next && strip.value?.contains(next)) return
  dropIndex.value = null
}

function onDrop(event: DragEvent) {
  const payload = dragging.value
  const index = dropIndex.value

  onDragEnd()
  if (payload?.kind !== 'tab' || index === null) return

  event.preventDefault()
  sessions.moveTab(payload.tabId, index)
}
</script>

<template>
  <div :class="cn('flex shrink-0 items-stretch gap-1 border-b px-2 py-1.5', BAR_HEIGHT)">
    <div
      ref="strip"
      class="relative flex min-w-0 flex-1 items-stretch gap-1 overflow-x-auto"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div
        v-for="(tab, index) in tabs"
        :key="tab.id"
        draggable="true"
        :class="cn(
          'group relative flex min-w-32 max-w-52 shrink-0 cursor-default items-center gap-1 rounded-md border px-2 text-sm',
          tab.id === activeTabId
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:bg-accent/50',
          borderOf(tab),
          dragging?.kind === 'tab' && dragging.tabId === tab.id && 'opacity-40',
        )"
        @click="sessions.focusTab(tab.id)"
        @dragstart="onDragStart(tab.id, $event)"
        @dragend="onDragEnd"
      >
        <!--
          The insertion caret. It hangs off the tab it precedes rather than being a
          separate element in the strip, so it cannot disturb the measurements the drop
          index is calculated from.
        -->
        <span
          v-if="dropIndex === index"
          class="absolute -left-0.5 top-1 bottom-1 w-0.5 rounded-full bg-primary"
          aria-hidden="true"
        />
        <span
          v-if="dropIndex === index + 1 && index === tabs.length - 1"
          class="absolute -right-0.5 top-1 bottom-1 w-0.5 rounded-full bg-primary"
          aria-hidden="true"
        />

        <span
          v-if="sessions.hasUnread(tab.id)"
          class="size-1.5 shrink-0 rounded-full bg-primary"
          :aria-label="`${titleOf(tab)} has new output`"
          role="status"
        />
        <span class="truncate">{{ titleOf(tab) }}</span>
        <Button
          variant="ghost"
          size="icon"
          class="ml-auto size-5 opacity-0 group-hover:opacity-100"
          :aria-label="`Close ${titleOf(tab)}`"
          @click.stop="sessions.closeTab(tab.id)"
        >
          <XIcon />
        </Button>
      </div>
    </div>

    <Button
      variant="ghost"
      size="icon"
      class="size-7 self-center"
      aria-label="New tab"
      @click="emit('newTab')"
    >
      <PlusIcon />
    </Button>

    <WorkspaceMenu />
  </div>
</template>
