/**
 * What is currently being dragged, shared across the terminal UI.
 *
 * The payload is kept here rather than in `DataTransfer` because a `dragover` handler is
 * only allowed to read the *types* it carries, never the values - and the drop targets
 * have to know which tab is coming to decide whether to highlight themselves at all. The
 * `DataTransfer` is still set, so a drag that leaves the window behaves normally.
 *
 * Deliberately not part of the sessions store: that one is watched deeply and written to
 * disk, and a pointer gesture has no business being persisted.
 */

import { readonly, ref } from 'vue'

export type DragPayload =
  | { kind: 'tab', tabId: string }
  | { kind: 'pane', paneId: string }

/** A private mime type, so the drop targets ignore files and text dragged in. */
export const DRAG_MIME = 'application/x-remotier'

const payload = ref<DragPayload | null>(null)

export const dragging = readonly(payload)

export function beginDrag(next: DragPayload, event?: DragEvent) {
  payload.value = next
  if (event?.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData(DRAG_MIME, JSON.stringify(next))
  }
}

export function endDrag() {
  payload.value = null
}

/** True when `event` is one of our drags, not a file or a selection from elsewhere. */
export function isOurDrag(event: DragEvent): boolean {
  return payload.value !== null || (event.dataTransfer?.types.includes(DRAG_MIME) ?? false)
}

/** The pane a drag is hovering, and the tab that currently owns it. */
export interface PaneTarget {
  tabId: string
  paneId: string
}

/**
 * Whether dropping `payload` on `target` would do anything.
 *
 * Nothing can be dropped into itself: not a pane onto its own body, and not a tab onto a
 * pane it already owns - a tab's layout cannot be inserted into itself. Both are no-ops,
 * so they must not accept the drop or light up as targets.
 *
 * The tab case is why a tab must not become active on `mousedown`: focusing it there would
 * put its own panes on screen before the drag began, making every drop a self-drop.
 */
export function canDropOnPane(payload: DragPayload | null, target: PaneTarget): boolean {
  if (!payload) return false
  return payload.kind === 'tab'
    ? payload.tabId !== target.tabId
    : payload.paneId !== target.paneId
}
