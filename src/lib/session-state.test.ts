import { describe, expect, it } from 'vitest'
import { createPane, splitPane, updatePane, type LayoutNode } from './layout'
import {
  SNAPSHOT_VERSION,
  deserialize,
  detachSessions,
  serialize,
  snapshot,
  type TabSnapshot,
} from './session-state'

function tab(layout: LayoutNode, name = 'tab'): TabSnapshot {
  return { id: 'tab-1', name, layout, activePaneId: 'pane-x' }
}

describe('detachSessions', () => {
  it('clears session ids but keeps hosts', () => {
    const pane = createPane('host-1')
    const connected = updatePane(pane, pane.id, { sessionId: 'session-1' })

    const detached = detachSessions(connected)

    expect(detached).toMatchObject({ hostId: 'host-1', sessionId: null })
  })

  it('reaches panes nested inside splits', () => {
    const a = createPane('host-1')
    const b = createPane('host-2')
    let tree: LayoutNode = splitPane(a, a.id, 'row', b)
    tree = updatePane(tree, b.id, { sessionId: 'session-2' })

    const detached = detachSessions(tree) as { children: { sessionId: string | null }[] }

    expect(detached.children[1].sessionId).toBeNull()
  })
})

describe('snapshot', () => {
  it('stamps the current version', () => {
    expect(snapshot([], null).version).toBe(SNAPSHOT_VERSION)
  })

  it('never stores a live session id', () => {
    const pane = createPane('host-1')
    const connected = updatePane(pane, pane.id, { sessionId: 'session-1' })

    const stored = snapshot([tab(connected)], 'tab-1')

    expect(JSON.stringify(stored)).not.toContain('session-1')
  })
})

describe('deserialize', () => {
  it('round trips a layout', () => {
    const a = createPane('host-1')
    const b = createPane('host-2')
    const tree = splitPane(a, a.id, 'row', b)

    const restored = deserialize(serialize([tab(tree)], 'tab-1'))

    expect(restored?.tabs).toHaveLength(1)
    expect(restored?.tabs[0].layout).toEqual(tree)
    expect(restored?.activeTabId).toBe('tab-1')
  })

  it('returns null for empty input', () => {
    expect(deserialize(null)).toBeNull()
    expect(deserialize('')).toBeNull()
  })

  it('returns null for invalid JSON rather than throwing', () => {
    // A corrupt snapshot should cost the layout, not stop the app starting.
    expect(deserialize('{not json')).toBeNull()
  })

  it('ignores a snapshot from a different version', () => {
    const stored = JSON.stringify({ version: 99, tabs: [tab(createPane())], activeTabId: 'tab-1' })
    expect(deserialize(stored)).toBeNull()
  })

  it('returns null when there are no tabs', () => {
    expect(deserialize(serialize([], null))).toBeNull()
  })

  it('drops malformed tabs but keeps the rest', () => {
    const good = tab(createPane('host-1'))
    const stored = JSON.stringify({
      version: SNAPSHOT_VERSION,
      tabs: [good, { id: 'broken' }, { name: 'no layout', id: 'x', activePaneId: 'y' }],
      activeTabId: 'tab-1',
    })

    const restored = deserialize(stored)

    expect(restored?.tabs).toHaveLength(1)
    expect(restored?.tabs[0].id).toBe('tab-1')
  })

  it('rejects a split with no children', () => {
    const stored = JSON.stringify({
      version: SNAPSHOT_VERSION,
      tabs: [{
        id: 'tab-1',
        name: 'tab',
        activePaneId: 'pane-x',
        layout: { kind: 'split', id: 's1', dir: 'row', sizes: [], children: [] },
      }],
      activeTabId: 'tab-1',
    })

    expect(deserialize(stored)).toBeNull()
  })

  it('falls back to the first tab when the active id is stale', () => {
    const stored = JSON.stringify({
      version: SNAPSHOT_VERSION,
      tabs: [tab(createPane())],
      activeTabId: 'tab-that-is-gone',
    })

    expect(deserialize(stored)?.activeTabId).toBe('tab-1')
  })

  it('strips session ids that were stored by an older build', () => {
    const pane = { kind: 'pane', id: 'p1', hostId: 'host-1', sessionId: 'stale-session' }
    const stored = JSON.stringify({
      version: SNAPSHOT_VERSION,
      tabs: [{ id: 'tab-1', name: 'tab', activePaneId: 'p1', layout: pane }],
      activeTabId: 'tab-1',
    })

    const restored = deserialize(stored)

    // A stale id would render the pane as connected to a session that no longer exists.
    expect(restored?.tabs[0].layout).toMatchObject({ sessionId: null, hostId: 'host-1' })
  })
})
