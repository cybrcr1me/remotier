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

/** Which side of a pane a dragged thing was dropped on. */
export type DropEdge = 'left' | 'right' | 'top' | 'bottom'

function dirOfEdge(edge: DropEdge): SplitNode['dir'] {
  return edge === 'left' || edge === 'right' ? 'row' : 'col'
}

function insertsBefore(edge: DropEdge): boolean {
  return edge === 'left' || edge === 'top'
}

/**
 * Put `node` next to `targetPaneId`, on the given side.
 *
 * This is the general form of `splitPane`: the incoming node can be a whole subtree, and
 * it can land on either side of the target. Same-direction splits flatten into the parent
 * for the same reason they do there - dropping three times on the right should give three
 * columns, not a staircase.
 */
export function insertNode(
  root: LayoutNode,
  targetPaneId: string,
  node: LayoutNode,
  edge: DropEdge,
): LayoutNode {
  const dir = dirOfEdge(edge)
  const before = insertsBefore(edge)

  if (isPane(root)) {
    if (root.id !== targetPaneId) return root
    return {
      kind: 'split',
      id: nextId('split'),
      dir,
      sizes: evenSizes(2),
      children: before ? [node, root] : [root, node],
    }
  }

  const index = root.children.findIndex(child => isPane(child) && child.id === targetPaneId)

  if (index !== -1 && root.dir === dir) {
    const children = [...root.children]
    children.splice(before ? index : index + 1, 0, node)
    return { ...root, children, sizes: evenSizes(children.length) }
  }

  return {
    ...root,
    children: root.children.map(child => insertNode(child, targetPaneId, node, edge)),
  }
}

/**
 * Move a pane that is already in the tree to a new position.
 *
 * The pane is lifted out first, so the target is located in the tree the pane has already
 * left - otherwise dropping a pane next to its own sibling could reference a split that
 * collapses the moment the pane is removed. Anything that would be a no-op or would lose
 * the pane returns the tree untouched.
 */
export function movePane(
  root: LayoutNode,
  paneId: string,
  targetPaneId: string,
  edge: DropEdge,
): LayoutNode {
  if (paneId === targetPaneId) return root

  const pane = findPane(root, paneId)
  if (!pane) return root

  const without = closePane(root, paneId)
  // The pane was the only one there; moving it anywhere is a no-op.
  if (!without) return root
  if (!findPane(without, targetPaneId)) return root

  return insertNode(without, targetPaneId, pane, edge)
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
