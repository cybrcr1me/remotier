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
 * The host of the tab's active pane, or `null` for none yet: the one whose icon a
 * single-pane tab wears. A split tab shows a split icon instead (`tabIsSplit`) and is tinted
 * by all of its hosts (`tabColors`), never by whichever pane happens to be focused.
 */
export function tabHostId(node: LayoutNode, activePaneId: string): string | null {
  const pane = findPane(node, activePaneId) ?? listPanes(node)[0] ?? null
  return pane?.hostId ?? null
}

/**
 * True when the tab holds more than one pane. Such a tab shows a split icon: any one
 * host's icon would claim the whole tab is that host. Each pane's chip carries its own.
 */
export function tabIsSplit(node: LayoutNode): boolean {
  return listPanes(node).length > 1
}

/**
 * The colours a tab is tinted in: each distinct colour among its hosts, in pane order, with
 * `null` for a host that has none. Empty when no host has one.
 *
 * All of them rather than the active pane's, the same as the title and icon: a split tab
 * tinted in one host's colour claims the whole tab is that host. `null` stays in as a stop
 * so a tab that is only partly one colour does not look entirely so.
 */
export function tabColors(
  node: LayoutNode,
  colorOf: (hostId: string) => string | null,
): (string | null)[] {
  const hosts = new Set<string>()
  const colors: (string | null)[] = []

  for (const pane of listPanes(node)) {
    // An empty pane has no host to speak for, the same as in the title.
    if (!pane.hostId || hosts.has(pane.hostId)) continue
    hosts.add(pane.hostId)
    const color = colorOf(pane.hostId)
    if (!colors.includes(color)) colors.push(color)
  }

  return colors.some(color => color !== null) ? colors : []
}
