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
import type { ConnectProgress } from '@/lib/types'
import { useVarsStore } from '@/stores/vars'
import { errorMessage, ipc } from '@/lib/ipc'
import { readTerminalTheme, terminalFontFamily } from '@/lib/terminal-theme'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { useSettingsStore } from '@/stores/settings'
import { Channel } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import { TerminalIcon } from '@lucide/vue'
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { toast } from 'vue-sonner'
import '@xterm/xterm/css/xterm.css'

const props = defineProps<{
  tabId: string
  paneId: string
  hostId: string | null
  sessionId: string | null
  active: boolean
  /** False for a pane restored from a previous run: it waits for the user. */
  autoConnect?: boolean
}>()

const emit = defineEmits<{
  focus: []
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
const status = ref<'idle' | 'connecting' | 'connected' | 'error'>('idle')
const error = ref<string | null>(null)
const log = ref<LogEntry[]>([])
const target = ref<string | null>(null)

// shallowRef: these are large, self-managing objects that must not be made reactive.
const terminal = shallowRef<Terminal | null>(null)
const fit = shallowRef<FitAddon | null>(null)

let resizeObserver: ResizeObserver | null = null
let resizeTimer: number | undefined

function createTerminal() {
  const term = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    fontFamily: terminalFontFamily(),
    fontSize: Number(settings.get('terminal.fontSize')) || 13,
    theme: readTerminalTheme(),
    scrollback: 10_000,
    macOptionIsMeta: true,
  })

  const fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.loadAddon(new SearchAddon())
  term.loadAddon(new WebLinksAddon())

  const unicode = new Unicode11Addon()
  term.loadAddon(unicode)
  term.unicode.activeVersion = '11'

  term.open(host.value!)

  // WebGL is the fast path but is unavailable in some VMs and remote sessions; the
  // canvas renderer is the built-in fallback, so a failure here is not fatal.
  try {
    const webgl = new WebglAddon()
    webgl.onContextLoss(() => webgl.dispose())
    term.loadAddon(webgl)
  } catch (e) {
    console.warn('WebGL renderer unavailable, falling back:', errorMessage(e))
  }

  term.onData((data) => {
    if (props.sessionId) void ipc.sshWrite(props.sessionId, encode(data))
  })
  term.onBinary((data) => {
    if (props.sessionId) void ipc.sshWrite(props.sessionId, encodeBinary(data))
  })

  terminal.value = term
  fit.value = fitAddon
  fitAddon.fit()
}

function encode(data: string) {
  return new TextEncoder().encode(data)
}

/** xterm hands binary events over as a string of char codes, one byte each. */
function encodeBinary(data: string) {
  const bytes = new Uint8Array(data.length)
  for (let i = 0; i < data.length; i += 1) bytes[i] = data.charCodeAt(i) & 0xff
  return bytes
}

async function connect() {
  const term = terminal.value
  if (!term || !props.hostId || status.value === 'connecting') return

  status.value = 'connecting'
  error.value = null
  log.value = []

  // Correlates the progress events with this attempt; the session id does not exist yet.
  const attemptId = `${props.paneId}-${Date.now()}`
  let unlistenProgress: UnlistenFn | null = null

  try {
    unlistenProgress = await listen<ConnectProgress>('ssh://progress', ({ payload }) => {
      if (payload.attemptId !== attemptId) return
      if (payload.stage === 'connecting') target.value = `${payload.host}:${payload.port}`
      log.value = appendEntry(log.value, infoEntry(describeStage(payload)))
    })
  } catch (e) {
    // Losing progress reporting is not a reason to refuse to connect.
    console.warn('could not subscribe to connection progress:', errorMessage(e))
  }

  const onData = new Channel<ArrayBuffer>()
  onData.onmessage = (chunk) => term.write(new Uint8Array(chunk))

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
    status.value = 'connected'
    term.focus()
  } catch (e) {
    status.value = 'error'
    error.value = errorMessage(e)
    log.value = appendEntry(log.value, errorEntry(error.value))
  } finally {
    unlistenProgress?.()
  }
}

/** Fit locally first so the grid is right, then tell the server. */
function handleResize() {
  const term = terminal.value
  if (!term || !fit.value) return

  fit.value.fit()

  window.clearTimeout(resizeTimer)
  resizeTimer = window.setTimeout(() => {
    if (props.sessionId) void ipc.sshResize(props.sessionId, term.cols, term.rows)
  }, 80)
}

onMounted(() => {
  createTerminal()

  resizeObserver = new ResizeObserver(handleResize)
  if (host.value) resizeObserver.observe(host.value)

  if (props.hostId && props.autoConnect !== false) void connect()
})

onBeforeUnmount(() => {
  window.clearTimeout(resizeTimer)
  resizeObserver?.disconnect()
  terminal.value?.dispose()
})

// Focus follows the active pane so keystrokes land where the highlight is.
watch(
  () => props.active,
  (active) => {
    if (active) terminal.value?.focus()
  },
)

watch(
  () => props.sessionId,
  (sessionId) => {
    if (!sessionId && status.value === 'connected') status.value = 'idle'
  },
)

defineExpose({ focus: () => terminal.value?.focus(), connect })
</script>

<template>
  <div
    class="relative flex h-full min-h-0 w-full flex-col overflow-hidden bg-background"
    @mousedown="emit('focus')"
  >
    <div v-show="hostId" ref="host" class="isolate min-h-0 flex-1 px-2 pt-2" />

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
