/**
 * Terminals, held by pane id rather than by component instance.
 *
 * A pane can be dragged to another split or another tab, and Vue answers a change of
 * position in the tree by unmounting the component and mounting a fresh one. If the xterm
 * instance belonged to the component, that move would dispose it, drop the IPC channel and
 * end the SSH session behind it - the same failure as unmounting a hidden tab, arriving by
 * a different route.
 *
 * So the terminal, its own container element and the connection state a user can see all
 * live here. `TerminalPane` borrows an entry for as long as it is mounted and adopts its
 * container; nothing is disposed until the pane itself is gone, which `releaseMissing`
 * decides by looking at the layout tree.
 *
 * Handlers registered on the terminal read `entry.sessionId` and `entry.paneId` rather
 * than closing over component props: the handler outlives the component that installed it,
 * so a captured prop would go stale the first time the pane moved.
 */

import type { LogEntry } from './connection-log'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { Unicode11Addon } from '@xterm/addon-unicode11'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import { ref, type Ref } from 'vue'

export type PaneStatus = 'idle' | 'connecting' | 'connected' | 'checking' | 'lost' | 'error'

export interface PaneTerminal {
  readonly paneId: string
  /** xterm is opened into this element once; moving a pane re-parents it, never reopens. */
  readonly container: HTMLDivElement
  readonly term: Terminal
  readonly fit: FitAddon
  /** Connection state, kept here so a pane that moves does not appear to reset. */
  readonly status: Ref<PaneStatus>
  readonly error: Ref<string | null>
  readonly log: Ref<LogEntry[]>
  readonly target: Ref<string | null>
  /** Mirrors the store, for handlers that must not capture a component's props. */
  sessionId: string | null
  /** WebGL is attached once the element has a real size; see `TerminalPane`. */
  acceleratorLoaded: boolean
}

export interface TerminalOptions {
  fontFamily: string
  fontSize: number
  theme: Terminal['options']['theme']
}

/** What the terminal does with keystrokes and with output it has been handed. */
export interface PaneHandlers {
  write: (paneId: string, sessionId: string, data: Uint8Array) => void
}

const entries = new Map<string, PaneTerminal>()

function build(paneId: string, options: TerminalOptions, handlers: PaneHandlers): PaneTerminal {
  const term = new Terminal({
    allowProposedApi: true,
    cursorBlink: true,
    fontFamily: options.fontFamily,
    fontSize: options.fontSize,
    theme: options.theme,
    scrollback: 10_000,
    macOptionIsMeta: true,
  })

  const fit = new FitAddon()
  term.loadAddon(fit)
  term.loadAddon(new SearchAddon())
  term.loadAddon(new WebLinksAddon())

  const unicode = new Unicode11Addon()
  term.loadAddon(unicode)
  term.unicode.activeVersion = '11'

  const container = document.createElement('div')
  container.className = 'h-full w-full'

  const entry: PaneTerminal = {
    paneId,
    container,
    term,
    fit,
    status: ref<PaneStatus>('idle'),
    error: ref<string | null>(null),
    log: ref<LogEntry[]>([]),
    target: ref<string | null>(null),
    sessionId: null,
    acceleratorLoaded: false,
  }

  term.onData((data) => {
    if (entry.sessionId) handlers.write(paneId, entry.sessionId, new TextEncoder().encode(data))
  })
  term.onBinary((data) => {
    // xterm hands binary over as a string of char codes, one byte each.
    const bytes = new Uint8Array(data.length)
    for (let i = 0; i < data.length; i += 1) bytes[i] = data.charCodeAt(i) & 0xff
    if (entry.sessionId) handlers.write(paneId, entry.sessionId, bytes)
  })

  entries.set(paneId, entry)
  return entry
}

/**
 * The pane's terminal, created on first use.
 *
 * `host` is where the container should hang. Calling this again after a move re-parents
 * the existing element, which xterm tolerates - what it does not tolerate is `open()` a
 * second time.
 */
export function acquire(
  paneId: string,
  host: HTMLElement,
  options: TerminalOptions,
  handlers: PaneHandlers,
): PaneTerminal {
  const existing = entries.get(paneId)

  if (existing) {
    if (existing.container.parentElement !== host) host.appendChild(existing.container)
    return existing
  }

  const entry = build(paneId, options, handlers)
  host.appendChild(entry.container)
  entry.term.open(entry.container)
  return entry
}

/**
 * Record that a pane's connection died under it.
 *
 * Reached from the session-event listener rather than from the pane component: the pane
 * that owns the terminal may not even be mounted - it could be in a background tab - and
 * the state has to be right for whenever it is next looked at.
 */
export function markLost(paneId: string, message: string): void {
  const entry = entries.get(paneId)
  if (!entry) return

  entry.status.value = 'lost'
  entry.error.value = message
  entry.sessionId = null
}

export function peek(paneId: string): PaneTerminal | undefined {
  return entries.get(paneId)
}

/** Dispose one pane's terminal. Safe to call for a pane that never had one. */
export function release(paneId: string): void {
  const entry = entries.get(paneId)
  if (!entry) return

  entries.delete(paneId)
  entry.term.dispose()
  entry.container.remove()
}

/**
 * Dispose every terminal whose pane is no longer anywhere in the app.
 *
 * This is the only thing that ends a terminal's life. Driving it from the set of live
 * panes rather than from a component's teardown is what makes closing a pane and merely
 * moving one different events - which, from the component's point of view, they are not.
 */
export function releaseMissing(livePaneIds: Iterable<string>): string[] {
  const live = new Set(livePaneIds)
  const dropped: string[] = []

  for (const paneId of [...entries.keys()]) {
    if (live.has(paneId)) continue
    release(paneId)
    dropped.push(paneId)
  }

  return dropped
}

/** Test seam: forget everything without touching the DOM-bound terminals. */
export function reset(): void {
  for (const paneId of [...entries.keys()]) release(paneId)
}

export function size(): number {
  return entries.size
}
