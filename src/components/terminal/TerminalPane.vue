<script setup lang="ts">
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Spinner } from '@/components/ui/spinner'
import { connectWithHostKeyPrompt, type HostKeyPrompt } from '@/lib/connect-flow'
import { useVarsStore } from '@/stores/vars'
import { errorMessage, ipc } from '@/lib/ipc'
import { readTerminalTheme, terminalFontFamily } from '@/lib/terminal-theme'
import { useSessionsStore } from '@/stores/sessions'
import { useSettingsStore } from '@/stores/settings'
import { Channel } from '@tauri-apps/api/core'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { WebglAddon } from '@xterm/addon-webgl'
import { Terminal } from '@xterm/xterm'
import { PlugZapIcon, TerminalIcon } from '@lucide/vue'
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
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
}>()

const sessions = useSessionsStore()
const settings = useSettingsStore()
const vars = useVarsStore()

const host = ref<HTMLDivElement | null>(null)
const status = ref<'idle' | 'connecting' | 'connected' | 'error'>('idle')
const error = ref<string | null>(null)

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

  const onData = new Channel<ArrayBuffer>()
  onData.onmessage = (chunk) => term.write(new Uint8Array(chunk))

  try {
    const sessionId = await connectWithHostKeyPrompt({
      request: { hostId: props.hostId, cols: term.cols, rows: term.rows },
      connect: request => ipc.sshConnect(request, onData),
      askAboutHostKey: prompt =>
        new Promise(resolve => emit('hostKey', prompt, resolve)),
      askAboutVariables: names =>
        new Promise(resolve => emit('variables', names, resolve)),
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
    term.writeln(`\r\n\x1b[31m${error.value}\x1b[0m`)
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
    <div v-show="hostId" ref="host" class="min-h-0 flex-1 px-2 pt-2" />

    <Empty v-if="!hostId" class="h-full">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TerminalIcon />
        </EmptyMedia>
        <EmptyTitle>No host selected</EmptyTitle>
        <EmptyDescription>Pick a host to open a shell in this pane.</EmptyDescription>
      </EmptyHeader>
    </Empty>

    <div
      v-if="status === 'connecting'"
      class="absolute inset-0 flex items-center justify-center gap-2 bg-background/80 text-sm text-muted-foreground"
    >
      <Spinner />
      Connecting…
    </div>

    <div
      v-else-if="status === 'idle' && hostId"
      class="absolute inset-x-0 bottom-0 flex items-center justify-center border-t bg-background/95 p-2"
    >
      <Button size="sm" variant="secondary" @click="connect">
        <PlugZapIcon data-icon="inline-start" />
        Reconnect
      </Button>
    </div>
  </div>
</template>
