/**
 * What a tab calls itself.
 *
 * A tab used to be named once, when it was opened. Now that tabs can be merged and panes
 * dragged between them, a fixed name goes stale the moment a tab gains a second host - the
 * merged tab would still advertise only whichever of the two happened to be the target. So
 * the title is derived from whatever the tab currently holds.
 *
 * Pure, and given a lookup rather than a store, so it can be tested without either.
 */

import { findPane, listPanes, type LayoutNode } from './layout'

export const UNTITLED = 'New tab'

/** Resolves a host id to its label, or `null` when the host is unknown. */
export type LabelLookup = (hostId: string) => string | null

/**
 * Host labels in the tab, in pane order, with repeats counted rather than repeated.
 *
 * Two panes on the same host is the normal way to work - one shell watching, one shell
 * typing - so listing the name twice is noise. `terminal.shop ×2` says the same thing in
 * less space and stays readable when the tab is narrow.
 */
export function titleParts(node: LayoutNode, labelOf: LabelLookup): string[] {
  const counts = new Map<string, number>()

  for (const pane of listPanes(node)) {
    // An empty pane contributes nothing: it has no identity to report yet.
    if (!pane.hostId) continue
    const label = labelOf(pane.hostId)
    if (!label) continue
    counts.set(label, (counts.get(label) ?? 0) + 1)
  }

  return [...counts].map(([label, count]) => (count > 1 ? `${label} ×${count}` : label))
}

/**
 * The tab's title.
 *
 * `fallback` is the tab's stored name, used while nothing in it has a host yet so a fresh
 * tab still reads as something rather than as an empty string.
 */
export function tabTitle(node: LayoutNode, labelOf: LabelLookup, fallback = UNTITLED): string {
  const parts = titleParts(node, labelOf)
  return parts.length > 0 ? parts.join(' · ') : fallback
}

/**
 * The host a tab stands for - whose icon and colour it wears - or `null` for none yet.
 *
 * Taken from the host of the tab's active pane rather than from whatever it holds: a tab
 * can carry several hosts with several colours, and mixing them produces either a lie or a
 * stripe. The active pane is the one the tab would show if you clicked it, which makes the
 * icon and colour a promise the tab can keep.
 */
export function tabHostId(node: LayoutNode, activePaneId: string): string | null {
  const pane = findPane(node, activePaneId) ?? listPanes(node)[0] ?? null
  return pane?.hostId ?? null
}
