/**
 * Serialising open tabs so they can be restored next launch, and saved as workspaces.
 *
 * Only the structure is stored - which hosts are open, in what layout. Live session ids
 * are deliberately dropped: an SSH session does not survive the app closing, and keeping
 * a stale id would make a pane look connected when it is not.
 */

import { isPane, type LayoutNode } from '@/lib/layout'

/** Bumped when the shape changes incompatibly; older snapshots are then ignored. */
export const SNAPSHOT_VERSION = 1

export interface TabSnapshot {
  id: string
  name: string
  layout: LayoutNode
  activePaneId: string
}

export interface SessionSnapshot {
  version: number
  tabs: TabSnapshot[]
  activeTabId: string | null
}

/** Strip live session ids from a layout, leaving the hosts in place. */
export function detachSessions(node: LayoutNode): LayoutNode {
  if (isPane(node)) {
    return { ...node, sessionId: null }
  }
  return { ...node, children: node.children.map(detachSessions) }
}

export function snapshot(tabs: TabSnapshot[], activeTabId: string | null): SessionSnapshot {
  return {
    version: SNAPSHOT_VERSION,
    tabs: tabs.map(tab => ({ ...tab, layout: detachSessions(tab.layout) })),
    activeTabId,
  }
}

export function serialize(tabs: TabSnapshot[], activeTabId: string | null): string {
  return JSON.stringify(snapshot(tabs, activeTabId))
}

function isLayoutNode(value: unknown): value is LayoutNode {
  if (typeof value !== 'object' || value === null) return false
  const node = value as Partial<LayoutNode> & { children?: unknown[] }

  if (node.kind === 'pane') {
    return typeof node.id === 'string'
  }
  if (node.kind === 'split') {
    return (
      typeof node.id === 'string'
      && (node.dir === 'row' || node.dir === 'col')
      && Array.isArray(node.children)
      && node.children.length > 0
      && node.children.every(isLayoutNode)
    )
  }
  return false
}

function isTabSnapshot(value: unknown): value is TabSnapshot {
  if (typeof value !== 'object' || value === null) return false
  const tab = value as Partial<TabSnapshot>
  return (
    typeof tab.id === 'string'
    && typeof tab.name === 'string'
    && typeof tab.activePaneId === 'string'
    && isLayoutNode(tab.layout)
  )
}

/**
 * Parse a stored snapshot.
 *
 * Anything malformed yields `null` rather than throwing: a corrupt snapshot should cost
 * the user their tab layout, not prevent the app from starting. Individual tabs that fail
 * validation are dropped so one bad tab does not discard the rest.
 */
export function deserialize(payload: string | null | undefined): SessionSnapshot | null {
  if (!payload) return null

  let parsed: unknown
  try {
    parsed = JSON.parse(payload)
  } catch {
    return null
  }

  if (typeof parsed !== 'object' || parsed === null) return null
  const candidate = parsed as Partial<SessionSnapshot>

  if (candidate.version !== SNAPSHOT_VERSION) return null
  if (!Array.isArray(candidate.tabs)) return null

  const tabs = candidate.tabs
    .filter(isTabSnapshot)
    .map(tab => ({ ...tab, layout: detachSessions(tab.layout) }))

  if (tabs.length === 0) return null

  const activeTabId
    = typeof candidate.activeTabId === 'string'
      && tabs.some(tab => tab.id === candidate.activeTabId)
      ? candidate.activeTabId
      : tabs[0].id

  return { version: SNAPSHOT_VERSION, tabs, activeTabId }
}
