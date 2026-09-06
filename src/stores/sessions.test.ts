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
