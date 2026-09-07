/**
 * Drag-and-drop geometry.
 *
 * Kept away from the components so the arithmetic that decides "which side of this pane
 * did the pointer land on" can be tested without a layout engine. Everything here takes
 * plain numbers; nothing touches the DOM.
 */

import type { DropEdge } from './layout'

/** The part of `DOMRect` this module needs, so tests can pass object literals. */
export interface Rect {
  left: number
  top: number
  width: number
  height: number
}

/** `center` means "drop into this pane", the four edges mean "split alongside it". */
export type DropZone = DropEdge | 'center'

/**
 * How much of a pane, measured from each edge, counts as that edge.
 *
 * A quarter leaves a centre region the same size as the two edge bands together, which
 * keeps "drop into" reachable in a narrow pane without making the edges fiddly.
 */
export const EDGE_FRACTION = 0.25

/**
 * Which zone of `rect` the point falls in.
 *
 * Distances are measured as a fraction of the pane's own width and height, so a tall
 * narrow pane and a short wide one both get proportional edge bands rather than the tall
 * one being almost entirely edge.
 */
export function dropZone(rect: Rect, x: number, y: number, fraction = EDGE_FRACTION): DropZone {
  if (rect.width <= 0 || rect.height <= 0) return 'center'

  const fromLeft = (x - rect.left) / rect.width
  const fromTop = (y - rect.top) / rect.height

  const distances: { zone: DropEdge, value: number }[] = [
    { zone: 'left', value: fromLeft },
    { zone: 'right', value: 1 - fromLeft },
    { zone: 'top', value: fromTop },
    { zone: 'bottom', value: 1 - fromTop },
  ]

  const nearest = distances.reduce((best, entry) => (entry.value < best.value ? entry : best))
  return nearest.value < fraction ? nearest.zone : 'center'
}

/** The CSS inset for the band an edge covers, as a preview overlay. */
export function zoneStyle(zone: DropZone, fraction = EDGE_FRACTION): Record<string, string> {
  const size = `${fraction * 100}%`
  const rest = `${(1 - fraction) * 100}%`

  switch (zone) {
    case 'left': return { top: '0', left: '0', width: size, height: '100%' }
    case 'right': return { top: '0', left: rest, width: size, height: '100%' }
    case 'top': return { top: '0', left: '0', width: '100%', height: size }
    case 'bottom': return { top: rest, left: '0', width: '100%', height: size }
    case 'center': return { top: '0', left: '0', width: '100%', height: '100%' }
  }
}

/**
 * Where a tab dragged to `x` should be inserted, as an index into the unchanged list.
 *
 * Each tab hands over its half: once the pointer is past a tab's midpoint the dragged tab
 * belongs after it. The result is an insertion index, so it ranges from 0 to `rects.length`.
 */
export function insertionIndex(rects: Rect[], x: number): number {
  let index = 0
  for (const rect of rects) {
    if (x >= rect.left + rect.width / 2) index += 1
  }
  return index
}

/**
 * Move `from` to `to`, where `to` is an insertion index into the *original* list.
 *
 * Removing the item first shifts everything after it down one, so an insertion index
 * past the item's old position has to come back by one to mean the same gap.
 */
export function reorder<T>(items: T[], from: number, to: number): T[] {
  if (from < 0 || from >= items.length) return items

  const target = to > from ? to - 1 : to
  if (target === from || target < 0 || target > items.length - 1) return items

  const next = [...items]
  const [moved] = next.splice(from, 1)
  next.splice(target, 0, moved)
  return next
}
