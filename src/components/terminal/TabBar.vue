<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { insertionIndex, type Rect } from '@/lib/dnd'
import { beginDrag, dragging, endDrag } from '@/lib/drag'
import EntityIcon from '@/components/hosts/EntityIcon.vue'
import { colorText, colorTint, hasColor, tintGradient } from '@/lib/appearance'
import { tabColors, tabHostId, tabIsSplit, tabTitle } from '@/lib/tab-title'
import { TOOLBAR_HEIGHT } from '@/lib/ui'
import { useInventoryStore } from '@/stores/inventory'
import { cn } from '@/lib/utils'
import { useSessionsStore } from '@/stores/sessions'
import WorkspaceMenu from './WorkspaceMenu.vue'
import { Columns2Icon, PlusIcon, TerminalIcon, XIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { ref } from 'vue'

const sessions = useSessionsStore()
const inventory = useInventoryStore()
const { tabs, activeTabId } = storeToRefs(sessions)

type TabLayout = Parameters<typeof tabTitle>[0]

/*
 * Titles are derived from the panes a tab currently holds, not stored on the tab. Merging
 * two tabs would otherwise leave the survivor advertising only the drop target's host.
 */
function titleOf(tab: { layout: TabLayout, name: string }) {
  return tabTitle(tab.layout, id => inventory.hostById.get(id)?.label ?? null, tab.name)
}

/** The host whose icon a single-pane tab wears, coloured in that host's colour. */
function hostOf(tab: { layout: TabLayout, activePaneId: string }) {
  const id = tabHostId(tab.layout, tab.activePaneId)
  return id ? inventory.hostById.get(id) ?? null : null
}

function iconColorOf(tab: { layout: TabLayout, activePaneId: string }) {
  const color = hostOf(tab)?.color
  return hasColor(color) ? colorText(color) : null
}

/** Each distinct colour among the tab's hosts, in pane order; empty when none has one. */
function colorsOf(tab: { layout: TabLayout }) {
  return tabColors(tab.layout, (id) => {
    const color = inventory.hostById.get(id)?.color
    return color && hasColor(color) ? color : null
  })
}

/*
 * The active tab is tinted by all of its hosts, like its title: one colour solid, several
 * blended left to right by `styleOf`, and lime when none has a colour. The others stay
 * untinted - a single-pane tab's icon carries its host's colour, enough to tell hosts apart
 * without every coloured tab looking selected.
 */
function stateOf(tab: { id: string, layout: TabLayout }) {
  if (tab.id !== activeTabId.value) {
    return 'border-transparent text-muted-foreground hover:bg-accent hover:text-foreground'
  }
  const colors = colorsOf(tab)
  if (colors.length === 0) return 'border-primary/30 bg-primary/10 text-foreground'
  if (colors.length === 1) return cn('text-foreground', colorTint(colors[0]))
  // The blend is painted by `styleOf`, through a border left clear for it.
  return 'border-transparent text-foreground'
}

function styleOf(tab: { id: string, layout: TabLayout }) {
  if (tab.id !== activeTabId.value) return undefined
  const colors = colorsOf(tab)
  return colors.length > 1 ? tintGradient(colors) : undefined
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
  <div :class="cn('flex shrink-0 items-center gap-1 border-b px-1.5', TOOLBAR_HEIGHT)">
    <div
      ref="strip"
      class="relative flex min-w-0 flex-1 items-stretch gap-1 self-stretch overflow-x-auto py-1"
      @dragover="onDragOver"
      @dragleave="onDragLeave"
      @drop="onDrop"
    >
      <div
        v-for="(tab, index) in tabs"
        :key="tab.id"
        draggable="true"
        :class="cn(
          // Every tab has a border, transparent unless it is active, so the active tab is
          // not a pixel larger than its neighbours. The close button's 12px glyph sits in a
          // 20px box, so its side needs less padding than the icon's to look the same.
          'group relative flex max-w-52 shrink-0 cursor-pointer items-center gap-1.5 rounded-md border pr-1 pl-2.5 text-xs',
          stateOf(tab),
          dragging?.kind === 'tab' && dragging.tabId === tab.id && 'opacity-40',
        )"
        :style="styleOf(tab)"
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

        <!--
          A tab holding several panes shows a split icon: one host's icon would claim the
          whole tab is that host, and each pane's chip already carries its own. A tab with
          no host yet shows a terminal, rather than a server it is not on.
        -->
        <Columns2Icon v-if="tabIsSplit(tab.layout)" class="size-3.5 shrink-0" aria-hidden="true" />
        <EntityIcon
          v-else-if="hostOf(tab)"
          :icon="hostOf(tab)?.icon"
          :class="cn('size-3.5 shrink-0', iconColorOf(tab))"
          aria-hidden="true"
        />
        <TerminalIcon v-else class="size-3.5 shrink-0" aria-hidden="true" />
        <span class="truncate">{{ titleOf(tab) }}</span>
        <span
          v-if="sessions.hasUnread(tab.id)"
          class="size-1.5 shrink-0 rounded-full bg-primary"
          :aria-label="`${titleOf(tab)} has new output`"
          role="status"
        />
        <!-- Always shown: hidden until hover, it leaves an empty slot on every other tab. -->
        <Button
          variant="ghost"
          size="icon"
          class="size-5 text-muted-foreground"
          :aria-label="`Close ${titleOf(tab)}`"
          @click.stop="sessions.closeTab(tab.id)"
        >
          <XIcon class="size-3" />
        </Button>
      </div>
    </div>

    <Button
      variant="ghost"
      size="icon"
      class="size-7"
      aria-label="New tab"
      @click="emit('newTab')"
    >
      <PlusIcon />
    </Button>

    <WorkspaceMenu />
  </div>
</template>
