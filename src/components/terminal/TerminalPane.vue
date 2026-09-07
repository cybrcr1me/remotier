<script setup lang="ts">
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import ConnectionPanel from './ConnectionPanel.vue'
import {
  appendEntry,
  describeStage,
  errorEntry,
  infoEntry,
  type LogEntry,
} from '@/lib/connection-log'
import {
  connectWithHostKeyPrompt,
  type HostKeyPrompt,
  type PasswordPrompt,
} from '@/lib/connect-flow'
import { dropZone, zoneStyle, type DropZone } from '@/lib/dnd'
import { beginDrag, canDropOnPane, dragging, endDrag, isOurDrag } from '@/lib/drag'
import type { ConnectProgress } from '@/lib/types'
import { useVarsStore } from '@/stores/vars'
import { errorMessage, ipc } from '@/lib/ipc'
import { readTerminalTheme, terminalFontFamily } from '@/lib/terminal-theme'
import { cn } from '@/lib/utils'
import { acquire, type PaneTerminal } from '@/lib/terminal-registry'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { useSettingsStore } from '@/stores/settings'
import { Channel } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { WebglAddon } from '@xterm/addon-webgl'
import { GripVerticalIcon, TerminalIcon } from '@lucide/vue'
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { toast } from 'vue-sonner'
import '@xterm/xterm/css/xterm.css'

const props = defineProps<{
  tabId: string
  paneId: string
  hostId: string | null
  sessionId: string | null
  active: boolean
  /** False while the pane's tab is hidden; it keeps running but must not take focus. */
  tabActive: boolean
  /** False for a pane restored from a previous run: it waits for the user. */
  autoConnect?: boolean
}>()

const emit = defineEmits<{
  focus: []
  /** A tab or pane was dropped on this pane, on the given side. */
  drop: [zone: DropZone]
  hostKey: [prompt: HostKeyPrompt, decide: (choice: 'reject' | 'once' | 'save') => void]
  variables: [names: string[], decide: (values: Record<string, string> | null) => void]
  password: [
    prompt: PasswordPrompt,
    decide: (answer: { password: string, remember: boolean } | null) => void,
  ]
}>()

const sessions = useSessionsStore()
const inventory = useInventoryStore()
const settings = useSettingsStore()
const vars = useVarsStore()

const host = ref<HTMLDivElement | null>(null)

/*
 * The terminal and everything the user can see about its connection belong to the pane,
 * not to this component. Dragging a pane elsewhere in the tree remounts the component;
 * were any of it held here, the move would dispose the terminal and end the session.
 */
// shallowRef: a large self-managing object that must not be made reactive.
const pane = shallowRef<PaneTerminal | null>(null)

const status = computed(() => pane.value?.status.value ?? 'idle')
const error = computed(() => pane.value?.error.value ?? null)
const log = computed<LogEntry[]>(() => pane.value?.log.value ?? [])
const target = computed(() => pane.value?.target.value ?? null)

let resizeObserver: ResizeObserver | null = null
let resizeTimer: number | undefined
/** The pane's own name, taken from the same host label the tab bar shows. */
const paneTitle = computed(() => {
  if (!props.hostId) return 'No host'
  return inventory.hostById.get(props.hostId)?.label ?? 'Unknown host'
})

/** The zone a drag is currently hovering, which is also the highlight to draw. */
const hoverZone = ref<DropZone | null>(null)

function acceptsDrop() {
  return canDropOnPane(dragging.value, { tabId: props.tabId, paneId: props.paneId })
}

function onPaneDragStart(event: DragEvent) {
  beginDrag({ kind: 'pane', paneId: props.paneId }, event)
}

function onDragOver(event: DragEvent) {
  if (!isOurDrag(event) || !acceptsDrop()) return

  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'

  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  hoverZone.value = dropZone(box, event.clientX, event.clientY)
}

/*
 * The terminal fills this pane, so crossing into it raises `dragleave` on the pane. Only
 * a move outside the pane entirely should clear the highlight.
 */
function onDragLeave(event: DragEvent) {
  const next = event.relatedTarget as Node | null
  const self = event.currentTarget as HTMLElement
  if (next && self.contains(next)) return
  hoverZone.value = null
}

function onDrop(event: DragEvent) {
  const zone = hoverZone.value
  hoverZone.value = null
  if (!zone || !acceptsDrop()) return

  event.preventDefault()
  event.stopPropagation()
  emit('drop', zone)
}

/** Borrow this pane's terminal, creating it the first time the pane is rendered. */
function attachTerminal() {
  pane.value = acquire(
    props.paneId,
    host.value!,
    {
      fontFamily: terminalFontFamily(),
      fontSize: Number(settings.get('terminal.fontSize')) || 13,
      theme: readTerminalTheme(),
    },
    { write: (_paneId, sessionId, data) => void ipc.sshWrite(sessionId, data) },
  )
}

async function connect() {
  const entry = pane.value
  if (!entry || !props.hostId || entry.status.value === 'connecting') return

  const term = entry.term
  entry.status.value = 'connecting'
  entry.error.value = null
  entry.log.value = []

  // Correlates the progress events with this attempt; the session id does not exist yet.
  const attemptId = `${props.paneId}-${Date.now()}`
  let unlistenProgress: UnlistenFn | null = null

  try {
    unlistenProgress = await listen<ConnectProgress>('ssh://progress', ({ payload }) => {
      if (payload.attemptId !== attemptId) return
      if (payload.stage === 'connecting') entry.target.value = `${payload.host}:${payload.port}`
      entry.log.value = appendEntry(entry.log.value, infoEntry(describeStage(payload)))
    })
  } catch (e) {
    // Losing progress reporting is not a reason to refuse to connect.
    console.warn('could not subscribe to connection progress:', errorMessage(e))
  }

  const onData = new Channel<ArrayBuffer>()
  onData.onmessage = (chunk) => {
    term.write(new Uint8Array(chunk))
    // Drives the unread dot. Asked by pane rather than told a tab id: this handler
    // outlives the component, so a captured `props.tabId` would be wrong the moment the
    // pane was dragged into another tab.
    sessions.noteOutputFromPane(entry.paneId)
  }

  try {
    const sessionId = await connectWithHostKeyPrompt({
      request: { hostId: props.hostId, cols: term.cols, rows: term.rows, attemptId },
      connect: request => ipc.sshConnect(request, onData),
      askAboutHostKey: prompt =>
        new Promise(resolve => emit('hostKey', prompt, resolve)),
      askAboutVariables: names =>
        new Promise(resolve => emit('variables', names, resolve)),
      askForPassword: async (prompt) => {
        const answer = await new Promise<{ password: string, remember: boolean } | null>(
          resolve => emit('password', prompt, resolve),
        )
        if (!answer) return null

        if (answer.remember && props.hostId) {
          try {
            // Saving is opt-in; a failure must not stop the connection that follows.
            await inventory.updateHostPassword(props.hostId, answer.password)
          } catch (e) {
            toast.error('Could not save the password', { description: errorMessage(e) })
          }
        }
        return answer.password
      },
      // Saved against the host itself: that is the most specific scope, so it resolves
      // whichever group declared the variable.
      saveVariables: async (values) => {
        for (const [name, value] of Object.entries(values)) {
          await vars.setValue('host', props.hostId!, name, value)
        }
      },
    })

    sessions.attachSession(props.tabId, props.paneId, sessionId)
    sessions.markConnected(props.paneId)
    entry.sessionId = sessionId
    entry.status.value = 'connected'
    term.focus()
  } catch (e) {
    entry.status.value = 'error'
    entry.error.value = errorMessage(e)
    entry.log.value = appendEntry(entry.log.value, errorEntry(entry.error.value))
  } finally {
    unlistenProgress?.()
  }
}

/**
 * Attach the WebGL renderer.
 *
 * Deferred until the element has a real size: initialising it against a zero-sized
 * container leaves a canvas that never paints, which looks exactly like a connection
 * that produces no output.
 */
function loadAccelerator(entry: PaneTerminal) {
  if (entry.acceleratorLoaded) return
  entry.acceleratorLoaded = true

  // WebGL is the fast path but is unavailable in some VMs and remote sessions; xterm
  // falls back to its DOM renderer, so a failure here is not fatal.
  try {
    const webgl = new WebglAddon()
    webgl.onContextLoss(() => webgl.dispose())
    entry.term.loadAddon(webgl)
  } catch (e) {
    console.warn('WebGL renderer unavailable, falling back:', errorMessage(e))
  }
}

/** Fit locally first so the grid is right, then tell the server. */
function handleResize() {
  const entry = pane.value
  const element = host.value
  if (!entry || !element) return

  // A zero-sized container yields a nonsensical grid; wait for a real layout.
  if (element.clientWidth === 0 || element.clientHeight === 0) return

  entry.fit.fit()
  loadAccelerator(entry)

  window.clearTimeout(resizeTimer)
  resizeTimer = window.setTimeout(() => {
    if (props.sessionId) void ipc.sshResize(props.sessionId, entry.term.cols, entry.term.rows)
  }, 80)
}

onMounted(() => {
  attachTerminal()
  pane.value!.sessionId = props.sessionId

  resizeObserver = new ResizeObserver(handleResize)
  if (host.value) resizeObserver.observe(host.value)

  // The observer covers the usual case; this catches a container that is already sized.
  requestAnimationFrame(handleResize)

  // A pane that arrived here by being dragged is already connected, and a restored one
  // is waiting for the user; neither should dial out again.
  const fresh = status.value === 'idle' && !props.sessionId
  if (fresh && props.hostId && props.autoConnect !== false) void connect()
})

/*
 * Deliberately does not dispose the terminal. This runs both when a pane is closed and
 * when it is merely dragged somewhere else, and the two are indistinguishable from here.
 * `TerminalsView` disposes the terminals whose panes have actually gone.
 */
onBeforeUnmount(() => {
  window.clearTimeout(resizeTimer)
  resizeObserver?.disconnect()
})

// Focus follows the active pane, but only within the visible tab: a hidden tab must not
// steal keystrokes from the one on screen.
watch(
  () => [props.active, props.tabActive] as const,
  ([active, tabActive]) => {
    if (active && tabActive) pane.value?.term.focus()
  },
  { immediate: true },
)

// A hidden tab reports no size, so refit when it comes back into view.
watch(
  () => props.tabActive,
  (tabActive) => {
    if (tabActive) requestAnimationFrame(handleResize)
  },
)

// Mirrors the store onto the entry, which is what the terminal's own handlers read.
watch(
  () => props.sessionId,
  (sessionId) => {
    const entry = pane.value
    if (!entry) return
    entry.sessionId = sessionId
    // `lost` already explains itself; do not overwrite it with the blanker `idle`.
    if (!sessionId && entry.status.value === 'connected') entry.status.value = 'idle'
  },
)

defineExpose({ focus: () => pane.value?.term.focus(), connect })
</script>

<template>
  <div
    class="group/pane relative flex h-full min-h-0 w-full flex-col overflow-hidden bg-background"
    @mousedown="emit('focus')"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <div v-show="hostId" ref="host" class="isolate min-h-0 flex-1 px-2 pt-2" />

    <!--
      The pane's chip: what this pane is connected to, and its drag handle. Panes in a
      split are otherwise indistinguishable once a shell has drawn over them, and a tab
      title covering four panes cannot say which is which. It is shown for an empty pane too,
      so a pane with no host yet can still be dragged somewhere useful.

      The terminal owns click and selection across its whole
      surface, so the pane cannot be `draggable` itself - dragging would take precedence
      over selecting text, which is the thing people do in a terminal all day. A div
      rather than a button: WebKit is unreliable about dragging form controls.
    -->
    <div
      draggable="true"
      role="button"
      tabindex="0"
      :class="cn(
        'absolute right-1 top-1 z-20 flex max-w-[70%] cursor-grab items-center gap-1 rounded-md border bg-card px-1.5 py-0.5 text-xs transition-opacity active:cursor-grabbing',
        'opacity-0 focus-visible:opacity-100 group-hover/pane:opacity-100',
        active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
      )"
      :aria-label="`Move ${paneTitle}`"
      :title="`Drag ${paneTitle} onto another pane's edge`"
      @dragstart="onPaneDragStart"
      @dragend="endDrag"
    >
      <GripVerticalIcon class="size-3 shrink-0" />
      <span class="truncate">{{ paneTitle }}</span>
    </div>

    <!--
      Where the dragged thing would land. `.xterm` paints its own layers up to z-index 10
      without creating a stacking context, hence the isolated host above and z-20 here -
      otherwise the terminal would cover the hint.
    -->
    <div
      v-if="hoverZone"
      class="pointer-events-none absolute z-20 border border-primary bg-primary/20 transition-all"
      :style="zoneStyle(hoverZone)"
      aria-hidden="true"
    />

    <Empty v-if="!hostId" class="h-full">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TerminalIcon />
        </EmptyMedia>
        <EmptyTitle>No host selected</EmptyTitle>
        <EmptyDescription>Pick a host to open a shell in this pane.</EmptyDescription>
      </EmptyHeader>
    </Empty>

    <ConnectionPanel
      v-if="hostId && status !== 'connected'"
      :status="status"
      :entries="log"
      :error="error"
      :target="target"
      @retry="connect"
    />
  </div>
</template>
