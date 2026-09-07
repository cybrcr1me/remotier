/**
 * Terminal keyboard shortcuts.
 *
 * Handled on the window in the capture phase so they fire before xterm's own key
 * handling, which would otherwise swallow them into the remote shell.
 */

import { onBeforeUnmount, onMounted } from 'vue'

export interface TerminalActions {
  newTab: () => void
  quickConnect: () => void
  splitRow: () => void
  splitCol: () => void
  closePane: () => void
  nextPane: () => void
  previousPane: () => void
  moveTabLeft: () => void
  moveTabRight: () => void
}

/** The primary modifier: Command on macOS, Control elsewhere. */
function hasPrimaryModifier(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey
}

/**
 * Match a key event to an action.
 *
 * Exported separately from the listener so the mapping can be tested without a DOM.
 * Returns `null` when the event is not a shortcut and should reach the terminal.
 */
export function matchShortcut(event: KeyboardEvent): keyof TerminalActions | null {
  if (!hasPrimaryModifier(event) || event.altKey) return null

  const key = event.key.toLowerCase()

  switch (key) {
    case 't':
      return event.shiftKey ? null : 'newTab'
    case 'k':
      return event.shiftKey ? null : 'quickConnect'
    case 'd':
      // Shift picks the other axis, matching the convention in most terminals.
      return event.shiftKey ? 'splitCol' : 'splitRow'
    case 'w':
      return event.shiftKey ? null : 'closePane'
    // Shifted brackets move the tab itself. Most layouts send the shifted character
    // rather than the bracket, so both spellings are matched.
    case ']':
      return event.shiftKey ? 'moveTabRight' : 'nextPane'
    case '[':
      return event.shiftKey ? 'moveTabLeft' : 'previousPane'
    case '}':
      return 'moveTabRight'
    case '{':
      return 'moveTabLeft'
    default:
      return null
  }
}

export function useTerminalShortcuts(actions: TerminalActions) {
  function handle(event: KeyboardEvent) {
    const action = matchShortcut(event)
    if (!action) return

    // Without this the combination also reaches the remote shell.
    event.preventDefault()
    event.stopPropagation()
    actions[action]()
  }

  onMounted(() => window.addEventListener('keydown', handle, { capture: true }))
  onBeforeUnmount(() => window.removeEventListener('keydown', handle, { capture: true }))
}
