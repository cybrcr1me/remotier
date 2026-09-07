import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { listPanes } from '@/lib/layout'
import { useSessionsStore } from './sessions'

const sshDisconnect = vi.fn(async (_sessionId: string) => {})
const getSessionState = vi.fn(async (): Promise<string | null> => null)
const setSessionState = vi.fn(async (_payload: string) => {})

vi.mock('@/lib/ipc', () => ({
  ipc: {
    sshDisconnect: (id: string) => sshDisconnect(id),
    getSessionState: () => getSessionState(),
    setSessionState: (payload: string) => setSessionState(payload),
  },
  errorMessage: (e: unknown) => String(e),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  sshDisconnect.mockClear()
  getSessionState.mockClear()
  getSessionState.mockResolvedValue(null)
  setSessionState.mockClear()
})

describe('tabs', () => {
  it('opens a tab with one focused pane', () => {
    const store = useSessionsStore()
    const tab = store.openTab('web-01')

    expect(store.tabs).toHaveLength(1)
    expect(store.activeTabId).toBe(tab.id)
    expect(store.activePane?.id).toBe(tab.activePaneId)
  })

  it('focuses the neighbour when the active tab closes', async () => {
    const store = useSessionsStore()
    const first = store.openTab('one')
    const second = store.openTab('two')
    const third = store.openTab('three')

    store.focusTab(second.id)
    await store.closeTab(second.id)

    expect(store.tabs.map(t => t.id)).toEqual([first.id, third.id])
    expect(store.activeTabId).toBe(third.id)
  })

  it('leaves no active tab once the last one closes', async () => {
    const store = useSessionsStore()
    const tab = store.openTab()

    await store.closeTab(tab.id)

    expect(store.tabs).toHaveLength(0)
    expect(store.activeTabId).toBeNull()
  })

  it('disconnects every live session in a closing tab', async () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    const first = tab.activePaneId
    store.attachSession(tab.id, first, 'session-1')

    const second = store.splitActivePane('row')!
    store.attachSession(tab.id, second.id, 'session-2')

    await store.closeTab(tab.id)

    expect(sshDisconnect).toHaveBeenCalledTimes(2)
    expect(sshDisconnect.mock.calls.flat()).toEqual(
      expect.arrayContaining(['session-1', 'session-2']),
    )
  })

  it('still closes the tab when disconnecting throws', async () => {
    sshDisconnect.mockRejectedValueOnce(new Error('already gone'))
    const store = useSessionsStore()
    const tab = store.openTab()
    store.attachSession(tab.id, tab.activePaneId, 'session-1')

    await store.closeTab(tab.id)

    expect(store.tabs).toHaveLength(0)
  })

  it('ignores renaming to blank', () => {
    const store = useSessionsStore()
    const tab = store.openTab('original')

    store.renameTab(tab.id, '   ')

    expect(store.tabs[0].name).toBe('original')
  })
})

describe('panes', () => {
  it('moves focus into the pane just created', () => {
    const store = useSessionsStore()
    store.openTab()

    const pane = store.splitActivePane('row')!

    expect(store.activePane?.id).toBe(pane.id)
  })

  it('builds a four pane grid', () => {
    const store = useSessionsStore()
    const tab = store.openTab()

    store.splitActivePane('row')
    store.splitActivePane('col')
    store.focusPane(tab.activePaneId)
    store.splitActivePane('col')

    expect(listPanes(store.tabs[0].layout)).toHaveLength(4)
  })

  it('disconnects the session belonging to a closed pane', async () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    const second = store.splitActivePane('row')!
    store.attachSession(tab.id, second.id, 'session-2')

    await store.closePane(tab.id, second.id)

    expect(sshDisconnect).toHaveBeenCalledExactlyOnceWith('session-2')
    expect(listPanes(store.tabs[0].layout)).toHaveLength(1)
  })

  it('closes the tab when its last pane closes', async () => {
    const store = useSessionsStore()
    const tab = store.openTab()

    await store.closePane(tab.id, tab.activePaneId)

    expect(store.tabs).toHaveLength(0)
  })

  it('moves focus off a pane that was closed', async () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    const first = tab.activePaneId
    const second = store.splitActivePane('row')!

    await store.closePane(tab.id, second.id)

    expect(store.activePane?.id).toBe(first)
  })

  it('cycles focus and wraps', () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    const first = tab.activePaneId
    const second = store.splitActivePane('row')!

    store.focusRelativePane(1)
    expect(store.activePane?.id).toBe(first)

    store.focusRelativePane(-1)
    expect(store.activePane?.id).toBe(second.id)
  })
})

describe('sessions', () => {
  it('attaches and detaches a session by id', () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    store.attachSession(tab.id, tab.activePaneId, 'session-1')

    expect(store.activePane?.sessionId).toBe('session-1')

    const paneId = store.detachSession('session-1')

    expect(paneId).toBe(tab.activePaneId)
    // The pane survives a disconnect so the user can reconnect in place.
    expect(store.activePane?.sessionId).toBeNull()
    expect(listPanes(store.tabs[0].layout)).toHaveLength(1)
  })

  it('finds the right pane across several tabs', () => {
    const store = useSessionsStore()
    store.openTab('one')
    const second = store.openTab('two')
    store.attachSession(second.id, second.activePaneId, 'session-9')

    expect(store.detachSession('session-9')).toBe(second.activePaneId)
  })

  it('reports nothing for an unknown session', () => {
    const store = useSessionsStore()
    store.openTab()

    expect(store.detachSession('missing')).toBeNull()
  })

  it('records a divider drag', () => {
    const store = useSessionsStore()
    const tab = store.openTab()
    store.splitActivePane('row')

    const splitId = (store.tabs[0].layout as { id: string }).id
    store.setSizes(tab.id, splitId, [70, 30])

    expect((store.tabs[0].layout as { sizes: number[] }).sizes).toEqual([70, 30])
  })
})

describe('persistence', () => {
  function storedSnapshot(hostId = 'host-1') {
    return JSON.stringify({
      version: 1,
      tabs: [{
        id: 'tab-saved',
        name: 'web-01',
        activePaneId: 'pane-saved',
        layout: { kind: 'pane', id: 'pane-saved', hostId, sessionId: null },
      }],
      activeTabId: 'tab-saved',
    })
  }

  it('restores tabs from the last run', async () => {
    getSessionState.mockResolvedValue(storedSnapshot())
    const store = useSessionsStore()

    await store.restore()

    expect(store.tabs).toHaveLength(1)
    expect(store.tabs[0].name).toBe('web-01')
    expect(store.activeTabId).toBe('tab-saved')
  })

  it('leaves restored panes waiting instead of dialling out on launch', async () => {
    getSessionState.mockResolvedValue(storedSnapshot())
    const store = useSessionsStore()

    await store.restore(false)

    // A burst of connections the user did not ask for would be the wrong default.
    expect(store.shouldAutoConnect('pane-saved')).toBe(false)
  })

  it('connects immediately when auto-reconnect is on', async () => {
    getSessionState.mockResolvedValue(storedSnapshot())
    const store = useSessionsStore()

    await store.restore(true)

    expect(store.shouldAutoConnect('pane-saved')).toBe(true)
  })

  it('stops waiting once a pane has connected', async () => {
    getSessionState.mockResolvedValue(storedSnapshot())
    const store = useSessionsStore()
    await store.restore(false)

    store.markConnected('pane-saved')

    expect(store.shouldAutoConnect('pane-saved')).toBe(true)
  })

  it('starts empty when nothing was saved', async () => {
    const store = useSessionsStore()

    await store.restore()

    expect(store.tabs).toHaveLength(0)
  })

  it('starts empty rather than throwing on a corrupt snapshot', async () => {
    getSessionState.mockResolvedValue('{not json')
    const store = useSessionsStore()

    await store.restore()

    expect(store.tabs).toHaveLength(0)
  })

  it('survives the backend failing to return a snapshot', async () => {
    getSessionState.mockRejectedValue(new Error('db is gone'))
    const store = useSessionsStore()

    await expect(store.restore()).resolves.toBeUndefined()
    expect(store.tabs).toHaveLength(0)
  })

  it('writes a snapshot with no live session ids', async () => {
    const store = useSessionsStore()
    const tab = store.openTab('web')
    store.attachSession(tab.id, tab.activePaneId, 'session-1')

    await store.persist()

    expect(setSessionState).toHaveBeenCalledOnce()
    expect(setSessionState.mock.calls[0][0]).not.toContain('session-1')
  })
})

describe('workspaces', () => {
  const workspace = JSON.stringify({
    version: 1,
    tabs: [{
      id: 'ws-tab',
      name: 'Morning checks',
      activePaneId: 'ws-pane',
      layout: { kind: 'pane', id: 'ws-pane', hostId: 'host-9', sessionId: null },
    }],
    activeTabId: 'ws-tab',
  })

  it('replaces the open tabs', async () => {
    const store = useSessionsStore()
    store.openTab('scratch')

    await store.applyWorkspace(workspace)

    expect(store.tabs).toHaveLength(1)
    expect(store.tabs[0].name).toBe('Morning checks')
  })

  it('gives the applied tabs fresh ids so it can be opened twice', async () => {
    const store = useSessionsStore()
    await store.applyWorkspace(workspace)
    const first = store.tabs[0].id

    await store.applyWorkspace(workspace)

    expect(store.tabs[0].id).not.toBe(first)
  })

  it('disconnects sessions belonging to the tabs it replaces', async () => {
    const store = useSessionsStore()
    const tab = store.openTab('scratch')
    store.attachSession(tab.id, tab.activePaneId, 'session-1')

    await store.applyWorkspace(workspace)

    expect(sshDisconnect).toHaveBeenCalledExactlyOnceWith('session-1')
  })

  it('leaves applied panes waiting to connect', async () => {
    const store = useSessionsStore()

    await store.applyWorkspace(workspace)

    expect(store.shouldAutoConnect(store.tabs[0].activePaneId)).toBe(false)
  })

  it('reports an unreadable workspace instead of clearing the tabs', async () => {
    const store = useSessionsStore()
    store.openTab('scratch')

    await expect(store.applyWorkspace('{not json')).rejects.toThrow()

    expect(store.tabs).toHaveLength(1)
    expect(store.tabs[0].name).toBe('scratch')
  })

  it('serialises the current tabs for saving', () => {
    const store = useSessionsStore()
    store.openTab('web')

    const json = store.currentLayoutJson()

    expect(JSON.parse(json)).toMatchObject({ version: 1, tabs: [{ name: 'web' }] })
  })
})

describe('unread activity', () => {
  it('marks a background tab that produced output', () => {
    const store = useSessionsStore()
    const background = store.openTab('background')
    const visible = store.openTab('visible')

    store.noteOutput(background.id)

    expect(store.hasUnread(background.id)).toBe(true)
    expect(store.hasUnread(visible.id)).toBe(false)
  })

  it('ignores output from the tab already on screen', () => {
    const store = useSessionsStore()
    const tab = store.openTab('visible')

    store.noteOutput(tab.id)

    // The user is watching it; a dot would be noise.
    expect(store.hasUnread(tab.id)).toBe(false)
  })

  it('clears the marker when the tab is opened', () => {
    const store = useSessionsStore()
    const background = store.openTab('background')
    store.openTab('visible')
    store.noteOutput(background.id)

    store.focusTab(background.id)

    expect(store.hasUnread(background.id)).toBe(false)
  })

  it('stays marked while a different tab is focused', () => {
    const store = useSessionsStore()
    const background = store.openTab('background')
    const other = store.openTab('other')
    store.openTab('visible')
    store.noteOutput(background.id)

    store.focusTab(other.id)

    expect(store.hasUnread(background.id)).toBe(true)
  })

  it('does not accumulate duplicates', () => {
    const store = useSessionsStore()
    const background = store.openTab('background')
    store.openTab('visible')

    store.noteOutput(background.id)
    store.noteOutput(background.id)

    expect(store.unread.size).toBe(1)
  })

  it('forgets a closed tab', async () => {
    const store = useSessionsStore()
    const background = store.openTab('background')
    store.openTab('visible')
    store.noteOutput(background.id)

    await store.closeTab(background.id)

    expect(store.hasUnread(background.id)).toBe(false)
    expect(store.unread.size).toBe(0)
  })
})

describe('rearranging tabs and panes', () => {
  it('moves a tab to a new position in the bar', () => {
    const store = useSessionsStore()
    const a = store.openTab('a')
    store.openTab('b')
    const c = store.openTab('c')

    store.moveTab(a.id, 3)
    expect(store.tabs.map(t => t.name)).toEqual(['b', 'c', 'a'])

    store.moveTab(c.id, 0)
    expect(store.tabs.map(t => t.name)).toEqual(['c', 'b', 'a'])
  })

  it('ignores a move of a tab that is not open', () => {
    const store = useSessionsStore()
    store.openTab('a')
    store.moveTab('missing', 0)
    expect(store.tabs).toHaveLength(1)
  })

  it('merges one tab into another as a split', () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    const sourcePane = source.activePaneId

    store.mergeTabInto(source.id, target.id, target.activePaneId, 'right')

    expect(store.tabs.map(t => t.id)).toEqual([target.id])
    expect(listPanes(store.tabs[0].layout).map(p => p.id)).toContain(sourcePane)
    expect(store.activeTabId).toBe(target.id)
  })

  it('does not disconnect anything when a tab is merged away', async () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    store.attachSession(source.id, source.activePaneId, 'session-1')

    store.mergeTabInto(source.id, target.id, target.activePaneId, 'bottom')
    await Promise.resolve()

    // The pane is still open, just somewhere else. Disconnecting it would be a bug.
    expect(sshDisconnect).not.toHaveBeenCalled()
    expect(listPanes(store.tabs[0].layout).some(p => p.sessionId === 'session-1')).toBe(true)
  })

  it('carries a whole split across rather than flattening it', () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    store.splitActivePane('row')

    const carried = listPanes(source.layout).map(p => p.id)
    expect(carried).toHaveLength(2)

    store.mergeTabInto(source.id, target.id, target.activePaneId, 'left')
    const panes = listPanes(store.tabs[0].layout).map(p => p.id)
    expect(panes).toEqual(expect.arrayContaining(carried))
    expect(panes).toHaveLength(3)
  })

  it('refuses to merge a tab into itself', () => {
    const store = useSessionsStore()
    const tab = store.openTab('a')
    store.mergeTabInto(tab.id, tab.id, tab.activePaneId, 'right')
    expect(store.tabs).toHaveLength(1)
  })

  it('moves a pane between tabs and closes the tab it emptied', () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    const moved = source.activePaneId

    store.movePane(moved, target.activePaneId, 'bottom')

    expect(store.tabs.map(t => t.id)).toEqual([target.id])
    expect(listPanes(store.tabs[0].layout).map(p => p.id)).toContain(moved)
  })

  it('leaves the source tab open when it still has panes', () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    const extra = store.splitActivePane('row')!

    store.movePane(extra.id, target.activePaneId, 'right')

    expect(store.tabs).toHaveLength(2)
    expect(listPanes(source.layout)).toHaveLength(1)
    expect(listPanes(target.layout).map(p => p.id)).toContain(extra.id)
  })

  it('reports which tab holds a pane, and every live pane', () => {
    const store = useSessionsStore()
    const a = store.openTab('a')
    const b = store.openTab('b')

    expect(store.tabIdForPane(a.activePaneId)).toBe(a.id)
    expect(store.tabIdForPane('missing')).toBeNull()
    expect(store.allPaneIds().sort()).toEqual([a.activePaneId, b.activePaneId].sort())
  })

  it('marks output against the tab a pane currently sits in', () => {
    const store = useSessionsStore()
    const target = store.openTab('target')
    const source = store.openTab('source')
    const moved = source.activePaneId

    store.focusTab(target.id)
    store.movePane(moved, target.activePaneId, 'right')

    // The pane is in the visible tab now, so its output is not unread anywhere.
    store.noteOutputFromPane(moved)
    expect(store.hasUnread(target.id)).toBe(false)
  })
})
