import { describe, expect, it } from 'vitest'
import {
  closePane,
  createPane,
  findPane,
  findPaneBySession,
  insertNode,
  listPanes,
  movePane,
  relativePane,
  sessionIds,
  setSizes,
  splitPane,
  updatePane,
  type LayoutNode,
  type SplitNode,
} from './layout'

function asSplit(node: LayoutNode | null): SplitNode {
  if (!node || node.kind !== 'split') throw new Error('expected a split node')
  return node
}

describe('createPane', () => {
  it('starts disconnected', () => {
    const pane = createPane('host-1')
    expect(pane.hostId).toBe('host-1')
    expect(pane.sessionId).toBeNull()
  })

  it('gives every pane a unique id', () => {
    const ids = new Set(Array.from({ length: 50 }, () => createPane().id))
    expect(ids.size).toBe(50)
  })
})

describe('splitPane', () => {
  it('turns a lone pane into a split of two', () => {
    const first = createPane()
    const second = createPane()

    const split = asSplit(splitPane(first, first.id, 'row', second))

    expect(split.dir).toBe('row')
    expect(split.children).toEqual([first, second])
    expect(split.sizes).toEqual([50, 50])
  })

  it('inserts the new pane directly after the one being split', () => {
    const [a, b, c] = [createPane(), createPane(), createPane()]
    let tree = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, a.id, 'row', c)

    expect(listPanes(tree).map(p => p.id)).toEqual([a.id, c.id, b.id])
  })

  it('flattens a split that matches its parent direction', () => {
    const [a, b, c] = [createPane(), createPane(), createPane()]
    let tree = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, b.id, 'row', c)

    const split = asSplit(tree)
    // Three columns side by side, not a split nested inside a split.
    expect(split.children).toHaveLength(3)
    expect(split.children.every(child => child.kind === 'pane')).toBe(true)
    expect(split.sizes).toEqual([100 / 3, 100 / 3, 100 / 3])
  })

  it('nests when the direction differs from the parent', () => {
    const [a, b, c] = [createPane(), createPane(), createPane()]
    let tree = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, b.id, 'col', c)

    const outer = asSplit(tree)
    expect(outer.dir).toBe('row')
    expect(outer.children).toHaveLength(2)
    expect(asSplit(outer.children[1]).dir).toBe('col')
  })

  it('supports the four-pane grid from splitting twice each way', () => {
    const a = createPane()
    const b = createPane()
    let tree: LayoutNode = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, a.id, 'col', createPane())
    tree = splitPane(tree, b.id, 'col', createPane())

    expect(listPanes(tree)).toHaveLength(4)
  })

  it('leaves the tree alone for an unknown pane id', () => {
    const a = createPane()
    const tree = splitPane(a, 'nope', 'row', createPane())
    expect(tree).toEqual(a)
  })

  it('does not mutate the input tree', () => {
    const a = createPane()
    const snapshot = structuredClone(a)
    splitPane(a, a.id, 'row', createPane())
    expect(a).toEqual(snapshot)
  })
})

describe('closePane', () => {
  it('returns null when the last pane goes', () => {
    const a = createPane()
    expect(closePane(a, a.id)).toBeNull()
  })

  it('collapses a split down to its surviving child', () => {
    const [a, b] = [createPane(), createPane()]
    const tree = splitPane(a, a.id, 'row', b)

    const remaining = closePane(tree, a.id)

    // The wrapper split is pointless with one child, so it must disappear.
    expect(remaining).toEqual(b)
  })

  it('keeps the split and rebalances when more than one child remains', () => {
    const [a, b, c] = [createPane(), createPane(), createPane()]
    let tree = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, b.id, 'row', c)

    const split = asSplit(closePane(tree, b.id))

    expect(listPanes(split).map(p => p.id)).toEqual([a.id, c.id])
    expect(split.sizes).toEqual([50, 50])
  })

  it('removes a nested pane without disturbing the rest', () => {
    const [a, b, c] = [createPane(), createPane(), createPane()]
    let tree: LayoutNode = splitPane(a, a.id, 'row', b)
    tree = splitPane(tree, b.id, 'col', c)

    const remaining = closePane(tree, c.id)

    expect(listPanes(remaining!).map(p => p.id)).toEqual([a.id, b.id])
  })

  it('is a no-op for an unknown pane id', () => {
    const a = createPane()
    const tree = splitPane(a, a.id, 'row', createPane())
    expect(closePane(tree, 'nope')).toEqual(tree)
  })
})

describe('updatePane', () => {
  it('patches only the target pane', () => {
    const [a, b] = [createPane(), createPane()]
    const tree = splitPane(a, a.id, 'row', b)

    const updated = updatePane(tree, a.id, { sessionId: 'session-1' })

    expect(findPane(updated, a.id)?.sessionId).toBe('session-1')
    expect(findPane(updated, b.id)?.sessionId).toBeNull()
  })

  it('can clear a session without removing the pane', () => {
    const a = createPane()
    const connected = updatePane(a, a.id, { sessionId: 'session-1' })

    const disconnected = updatePane(connected, a.id, { sessionId: null })

    expect(findPane(disconnected, a.id)).not.toBeNull()
    expect(findPane(disconnected, a.id)?.sessionId).toBeNull()
  })
})

describe('setSizes', () => {
  it('records a divider drag', () => {
    const a = createPane()
    const split = asSplit(splitPane(a, a.id, 'row', createPane()))

    const resized = asSplit(setSizes(split, split.id, [70, 30]))

    expect(resized.sizes).toEqual([70, 30])
  })

  it('ignores a size list that does not match the child count', () => {
    const a = createPane()
    const split = asSplit(splitPane(a, a.id, 'row', createPane()))

    const resized = asSplit(setSizes(split, split.id, [50, 30, 20]))

    expect(resized.sizes).toEqual([50, 50])
  })
})

describe('relativePane', () => {
  it('wraps forward past the last pane', () => {
    const [a, b] = [createPane(), createPane()]
    const tree = splitPane(a, a.id, 'row', b)

    expect(relativePane(tree, b.id, 1)?.id).toBe(a.id)
  })

  it('wraps backward past the first pane', () => {
    const [a, b] = [createPane(), createPane()]
    const tree = splitPane(a, a.id, 'row', b)

    expect(relativePane(tree, a.id, -1)?.id).toBe(b.id)
  })

  it('falls back to the first pane when the current one is gone', () => {
    const a = createPane()
    expect(relativePane(a, 'stale', 1)?.id).toBe(a.id)
  })
})

describe('sessionIds', () => {
  it('collects only connected panes', () => {
    const [a, b] = [createPane(), createPane()]
    let tree: LayoutNode = splitPane(a, a.id, 'row', b)
    tree = updatePane(tree, a.id, { sessionId: 'session-1' })

    expect(sessionIds(tree)).toEqual(['session-1'])
  })
})

describe('findPaneBySession', () => {
  it('maps a session back to its pane', () => {
    const [a, b] = [createPane(), createPane()]
    let tree: LayoutNode = splitPane(a, a.id, 'row', b)
    tree = updatePane(tree, b.id, { sessionId: 'session-2' })

    expect(findPaneBySession(tree, 'session-2')?.id).toBe(b.id)
    expect(findPaneBySession(tree, 'missing')).toBeNull()
  })
})

describe('insertNode', () => {
  it('splits a lone pane and honours the side', () => {
    const target = createPane('a')
    const incoming = createPane('b')

    const left = asSplit(insertNode(target, target.id, incoming, 'left'))
    expect(left.dir).toBe('row')
    expect(left.children.map(c => c.id)).toEqual([incoming.id, target.id])

    const bottom = asSplit(insertNode(target, target.id, incoming, 'bottom'))
    expect(bottom.dir).toBe('col')
    expect(bottom.children.map(c => c.id)).toEqual([target.id, incoming.id])
  })

  it('flattens into a parent that already runs the same way', () => {
    const a = createPane('a')
    const b = createPane('b')
    const first = asSplit(insertNode(a, a.id, b, 'right'))

    const c = createPane('c')
    const flat = asSplit(insertNode(first, b.id, c, 'right'))

    expect(flat.children).toHaveLength(3)
    expect(flat.children.map(n => n.id)).toEqual([a.id, b.id, c.id])
    expect(flat.sizes).toEqual([100 / 3, 100 / 3, 100 / 3])
  })

  it('nests when the direction differs from the parent', () => {
    const a = createPane('a')
    const b = createPane('b')
    const row = asSplit(insertNode(a, a.id, b, 'right'))

    const c = createPane('c')
    const mixed = asSplit(insertNode(row, b.id, c, 'bottom'))

    expect(mixed.dir).toBe('row')
    const nested = asSplit(mixed.children[1])
    expect(nested.dir).toBe('col')
    expect(nested.children.map(n => n.id)).toEqual([b.id, c.id])
  })

  it('takes a whole subtree, not just a pane', () => {
    const a = createPane('a')
    const b = createPane('b')
    const c = createPane('c')
    const incoming = splitPane(b, b.id, 'col', c)

    const merged = insertNode(a, a.id, incoming, 'right')
    expect(listPanes(merged).map(p => p.id)).toEqual([a.id, b.id, c.id])
  })

  it('leaves the tree alone when the target is not in it', () => {
    const a = createPane('a')
    expect(insertNode(a, 'missing', createPane('b'), 'right')).toBe(a)
  })
})

describe('movePane', () => {
  /** a | b | c, left to right. */
  function threeAcross() {
    const a = createPane('a')
    const b = createPane('b')
    const c = createPane('c')
    const tree = splitPane(splitPane(a, a.id, 'row', b), b.id, 'row', c)
    return { a, b, c, tree }
  }

  it('reorders panes within one split', () => {
    const { a, c, tree } = threeAcross()
    const moved = movePane(tree, c.id, a.id, 'left')
    expect(listPanes(moved).map(p => p.id)).toEqual([c.id, a.id, expect.any(String)])
  })

  it('keeps every pane, and only those panes', () => {
    const { b, c, tree } = threeAcross()
    const moved = movePane(tree, b.id, c.id, 'bottom')
    expect(listPanes(moved).map(p => p.id).sort()).toEqual(listPanes(tree).map(p => p.id).sort())
  })

  it('changes the orientation when dropped on a horizontal edge', () => {
    const { b, c, tree } = threeAcross()
    const moved = asSplit(movePane(tree, b.id, c.id, 'bottom'))

    // a and c stay side by side; b now sits under c.
    expect(moved.dir).toBe('row')
    const nested = asSplit(moved.children[1])
    expect(nested.dir).toBe('col')
    expect(nested.children.map(n => n.id)).toEqual([c.id, b.id])
  })

  it('collapses the split a pane leaves behind', () => {
    const a = createPane('a')
    const b = createPane('b')
    const c = createPane('c')
    // a on the left, b over c on the right.
    const tree = splitPane(splitPane(a, a.id, 'row', b), b.id, 'col', c)

    const moved = asSplit(movePane(tree, c.id, a.id, 'left'))
    // The col split held only b once c left, so it is gone.
    expect(moved.children.map(n => n.kind)).toEqual(['pane', 'pane', 'pane'])
    expect(moved.dir).toBe('row')
  })

  it('refuses to move a pane onto itself', () => {
    const { a, tree } = threeAcross()
    expect(movePane(tree, a.id, a.id, 'left')).toBe(tree)
  })

  it('refuses to move the only pane in a tab', () => {
    const only = createPane('a')
    expect(movePane(only, only.id, 'anything', 'right')).toBe(only)
  })

  it('ignores an unknown pane or target', () => {
    const { a, tree } = threeAcross()
    expect(movePane(tree, 'missing', a.id, 'right')).toBe(tree)
    expect(movePane(tree, a.id, 'missing', 'right')).toBe(tree)
  })
})
