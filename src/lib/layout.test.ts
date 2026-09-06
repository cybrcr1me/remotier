import { describe, expect, it } from 'vitest'
import {
  closePane,
  createPane,
  findPane,
  findPaneBySession,
  listPanes,
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
