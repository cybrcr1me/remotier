/**
 * The split layout tree.
 *
 * One shape drives live tabs, saved workspaces and session resume, so it stays plain
 * JSON with no class instances or terminal handles in it.
 *
 * Every operation returns a new tree rather than mutating in place: it keeps Vue's
 * reactivity honest and makes the behaviour straightforward to test.
 */

export interface PaneNode {
  kind: 'pane'
  id: string
  /** The host this pane connects to, if one has been chosen. */
  hostId: string | null
  /** Set once a session is live. Cleared on disconnect; the pane survives. */
  sessionId: string | null
}

export interface SplitNode {
  kind: 'split'
  id: string
  /** `row` places children side by side, `col` stacks them. */
  dir: 'row' | 'col'
  /** Percentages, one per child, summing to 100. */
  sizes: number[]
  children: LayoutNode[]
}

export type LayoutNode = PaneNode | SplitNode

let counter = 0

/** Ids only need to be unique within a session; `crypto.randomUUID` is overkill here. */
export function nextId(prefix: string): string {
  counter += 1
  return `${prefix}-${Date.now().toString(36)}-${counter}`
}

export function createPane(hostId: string | null = null): PaneNode {
  return { kind: 'pane', id: nextId('pane'), hostId, sessionId: null }
}

export function isPane(node: LayoutNode): node is PaneNode {
  return node.kind === 'pane'
}

/** Every pane in the tree, left to right, depth first. */
export function listPanes(node: LayoutNode): PaneNode[] {
  return isPane(node) ? [node] : node.children.flatMap(listPanes)
}

export function findPane(node: LayoutNode, paneId: string): PaneNode | null {
  return listPanes(node).find(pane => pane.id === paneId) ?? null
}

export function findPaneBySession(node: LayoutNode, sessionId: string): PaneNode | null {
  return listPanes(node).find(pane => pane.sessionId === sessionId) ?? null
}

/** Even split across `count` children. */
function evenSizes(count: number): number[] {
  return Array.from({ length: count }, () => 100 / count)
}

/**
 * Split `paneId` in `dir`, putting `newPane` after it.
 *
 * A split in the same direction as its parent is flattened into that parent instead of
 * nesting, so three vertical splits give three equal columns rather than a lopsided
 * tree - the behaviour every terminal multiplexer has.
 */
export function splitPane(
  root: LayoutNode,
  paneId: string,
  dir: SplitNode['dir'],
  newPane: PaneNode,
): LayoutNode {
  if (isPane(root)) {
    if (root.id !== paneId) return root
    return {
      kind: 'split',
      id: nextId('split'),
      dir,
      sizes: evenSizes(2),
      children: [root, newPane],
    }
  }

  const index = root.children.findIndex(child => isPane(child) && child.id === paneId)

  if (index !== -1 && root.dir === dir) {
    const children = [...root.children]
    children.splice(index + 1, 0, newPane)
    return { ...root, children, sizes: evenSizes(children.length) }
  }

  return {
    ...root,
    children: root.children.map(child => splitPane(child, paneId, dir, newPane)),
  }
}

/**
 * Remove a pane.
 *
 * Splits left with one child collapse into that child, so the tree never accumulates
 * pointless wrappers. Returns `null` when the last pane was removed.
 */
export function closePane(root: LayoutNode, paneId: string): LayoutNode | null {
  if (isPane(root)) {
    return root.id === paneId ? null : root
  }

  const children = root.children
    .map(child => closePane(child, paneId))
    .filter((child): child is LayoutNode => child !== null)

  if (children.length === 0) return null
  if (children.length === 1) return children[0]

  const removed = children.length !== root.children.length
  return {
    ...root,
    children,
    sizes: removed ? evenSizes(children.length) : root.sizes,
  }
}

/** Apply a patch to one pane, leaving the rest of the tree untouched. */
export function updatePane(
  root: LayoutNode,
  paneId: string,
  patch: Partial<Omit<PaneNode, 'kind' | 'id'>>,
): LayoutNode {
  if (isPane(root)) {
    return root.id === paneId ? { ...root, ...patch } : root
  }
  return {
    ...root,
    children: root.children.map(child => updatePane(child, paneId, patch)),
  }
}

/** Record a drag of a split's dividers. */
export function setSizes(root: LayoutNode, splitId: string, sizes: number[]): LayoutNode {
  if (isPane(root)) return root
  if (root.id === splitId) {
    // Ignore a size list that does not match the children, rather than corrupting the tree.
    return sizes.length === root.children.length ? { ...root, sizes } : root
  }
  return {
    ...root,
    children: root.children.map(child => setSizes(child, splitId, sizes)),
  }
}

/** Next pane in document order, wrapping. Returns `null` for an empty tree. */
export function relativePane(root: LayoutNode, paneId: string, offset: 1 | -1): PaneNode | null {
  const panes = listPanes(root)
  if (panes.length === 0) return null

  const index = panes.findIndex(pane => pane.id === paneId)
  if (index === -1) return panes[0]

  // `+ panes.length` keeps the result positive when stepping back from the first pane.
  return panes[(index + offset + panes.length) % panes.length]
}

/** Live session ids in the tree, for cleanup when a tab closes. */
export function sessionIds(root: LayoutNode): string[] {
  return listPanes(root)
    .map(pane => pane.sessionId)
    .filter((id): id is string => id !== null)
}
