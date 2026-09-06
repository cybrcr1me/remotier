/**
 * Tabs, panes, and which pane owns which live SSH session.
 *
 * The store deliberately holds no terminal objects - xterm instances live in the
 * component that renders a pane. Everything here stays serialisable so a tab can be
 * saved as a workspace or restored on next launch.
 */

import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { errorMessage, ipc } from '@/lib/ipc'
import { deserialize, serialize } from '@/lib/session-state'
import {
  closePane as closePaneInTree,
  createPane,
  findPane,
  findPaneBySession,
  listPanes,
  nextId,
  relativePane,
  sessionIds,
  setSizes as setSizesInTree,
  splitPane,
  updatePane,
  type LayoutNode,
  type SplitNode,
} from '@/lib/layout'

export interface Tab {
  id: string
  name: string
  layout: LayoutNode
  activePaneId: string
}

function createTab(name: string, hostId: string | null = null): Tab {
  const pane = createPane(hostId)
  return { id: nextId('tab'), name, layout: pane, activePaneId: pane.id }
}

export const useSessionsStore = defineStore('sessions', () => {
  const tabs = ref<Tab[]>([])
  const activeTabId = ref<string | null>(null)

  /**
   * Panes restored from disk. Their hosts are known but nothing is connected, so they
   * wait for the user rather than dialling out on launch.
   */
  const awaitingReconnect = ref(new Set<string>())
  const restoring = ref(false)

  /** Tabs that produced output while they were not the visible one. */
  const unread = ref(new Set<string>())

  const activeTab = computed(() => tabs.value.find(tab => tab.id === activeTabId.value) ?? null)
  const activePane = computed(() => {
    const tab = activeTab.value
    return tab ? findPane(tab.layout, tab.activePaneId) : null
  })

  function tabById(tabId: string) {
    return tabs.value.find(tab => tab.id === tabId) ?? null
  }

  /** False for a pane restored from a previous run until the user connects it. */
  function shouldAutoConnect(paneId: string) {
    return !awaitingReconnect.value.has(paneId)
  }

  function markConnected(paneId: string) {
    if (!awaitingReconnect.value.has(paneId)) return
    const next = new Set(awaitingReconnect.value)
    next.delete(paneId)
    awaitingReconnect.value = next
  }

  function openTab(name = 'New tab', hostId: string | null = null) {
    const tab = createTab(name, hostId)
    tabs.value.push(tab)
    activeTabId.value = tab.id
    return tab
  }

  function focusTab(tabId: string) {
    if (!tabById(tabId)) return
    activeTabId.value = tabId
    clearUnread(tabId)
  }

  /**
   * Record output from a tab. Ignored for the visible tab, which the user is watching
   * anyway.
   */
  function noteOutput(tabId: string) {
    if (tabId === activeTabId.value || unread.value.has(tabId)) return
    const next = new Set(unread.value)
    next.add(tabId)
    unread.value = next
  }

  function clearUnread(tabId: string) {
    if (!unread.value.has(tabId)) return
    const next = new Set(unread.value)
    next.delete(tabId)
    unread.value = next
  }

  function hasUnread(tabId: string) {
    return unread.value.has(tabId)
  }

  /** Disconnect everything in the tab, then drop it. */
  async function closeTab(tabId: string) {
    const tab = tabById(tabId)
    if (!tab) return

    const live = sessionIds(tab.layout)
    const index = tabs.value.findIndex(t => t.id === tabId)
    tabs.value = tabs.value.filter(t => t.id !== tabId)
    clearUnread(tabId)

    if (activeTabId.value === tabId) {
      // Focus the neighbour that took its place, or the new last tab.
      const next = tabs.value[Math.min(index, tabs.value.length - 1)]
      activeTabId.value = next?.id ?? null
    }

    await disconnectAll(live)
  }

  async function disconnectAll(ids: string[]) {
    await Promise.all(
      ids.map(async (sessionId) => {
        try {
          await ipc.sshDisconnect(sessionId)
        } catch (e) {
          // The session may already be gone; losing the tab anyway is correct.
          console.warn(`could not disconnect ${sessionId}: ${errorMessage(e)}`)
        }
      }),
    )
  }

  function focusPane(paneId: string) {
    const tab = activeTab.value
    if (tab && findPane(tab.layout, paneId)) tab.activePaneId = paneId
  }

  function focusRelativePane(offset: 1 | -1) {
    const tab = activeTab.value
    if (!tab) return
    const pane = relativePane(tab.layout, tab.activePaneId, offset)
    if (pane) tab.activePaneId = pane.id
  }

  /** Split the focused pane and move focus into the new one. */
  function splitActivePane(dir: SplitNode['dir'], hostId: string | null = null) {
    const tab = activeTab.value
    if (!tab) return null

    const pane = createPane(hostId)
    tab.layout = splitPane(tab.layout, tab.activePaneId, dir, pane)
    tab.activePaneId = pane.id
    return pane
  }

  /**
   * Close one pane. Closing the last pane in a tab closes the tab, which is what every
   * terminal does and avoids leaving an empty shell behind.
   */
  async function closePane(tabId: string, paneId: string) {
    const tab = tabById(tabId)
    if (!tab) return

    const pane = findPane(tab.layout, paneId)
    const live = pane?.sessionId ? [pane.sessionId] : []

    const remaining = closePaneInTree(tab.layout, paneId)
    if (!remaining) {
      await closeTab(tabId)
      return
    }

    tab.layout = remaining
    if (tab.activePaneId === paneId) {
      tab.activePaneId = listPanes(remaining)[0]?.id ?? tab.activePaneId
    }
    await disconnectAll(live)
  }

  function setPaneHost(tabId: string, paneId: string, hostId: string | null) {
    const tab = tabById(tabId)
    if (tab) tab.layout = updatePane(tab.layout, paneId, { hostId })
  }

  /** Record that a pane now owns a live session. */
  function attachSession(tabId: string, paneId: string, sessionId: string) {
    const tab = tabById(tabId)
    if (tab) tab.layout = updatePane(tab.layout, paneId, { sessionId })
  }

  /**
   * Clear a session that has ended. Called from the `ssh://session` listener, so it has
   * to find the pane by session id rather than being told where it is.
   */
  function detachSession(sessionId: string) {
    for (const tab of tabs.value) {
      const pane = findPaneBySession(tab.layout, sessionId)
      if (pane) {
        tab.layout = updatePane(tab.layout, pane.id, { sessionId: null })
        return pane.id
      }
    }
    return null
  }

  function setSizes(tabId: string, splitId: string, sizes: number[]) {
    const tab = tabById(tabId)
    if (tab) tab.layout = setSizesInTree(tab.layout, splitId, sizes)
  }

  /** Persist the current tabs. Debounced by the watcher below. */
  async function persist() {
    // Restoring assigns tabs, which would otherwise write the snapshot straight back.
    if (restoring.value) return
    try {
      await ipc.setSessionState(serialize(tabs.value, activeTabId.value))
    } catch (e) {
      console.warn(`could not save the session layout: ${errorMessage(e)}`)
    }
  }

  /**
   * Restore the tabs from the last run.
   *
   * Panes come back disconnected. Auto-reconnecting on launch would fire a burst of
   * connections the user did not ask for, so it is opt-in via settings.
   */
  async function restore(autoReconnect = false) {
    let payload: string | null = null
    try {
      payload = await ipc.getSessionState()
    } catch (e) {
      console.warn(`could not read the saved session layout: ${errorMessage(e)}`)
      return
    }

    const snapshot = deserialize(payload)
    if (!snapshot) return

    restoring.value = true
    try {
      tabs.value = snapshot.tabs.map(tab => ({ ...tab }))
      activeTabId.value = snapshot.activeTabId

      if (!autoReconnect) {
        awaitingReconnect.value = new Set(
          tabs.value.flatMap(tab => listPanes(tab.layout).map(pane => pane.id)),
        )
      }
    } finally {
      restoring.value = false
    }
  }

  /** Replace the open tabs with a saved workspace. */
  async function applyWorkspace(layoutJson: string) {
    const snapshot = deserialize(layoutJson)
    if (!snapshot) {
      throw new Error('That workspace could not be read.')
    }

    const live = tabs.value.flatMap(tab => sessionIds(tab.layout))

    restoring.value = true
    try {
      // Fresh ids: applying the same workspace twice must not collide with itself.
      tabs.value = snapshot.tabs.map(tab => ({ ...tab, id: nextId('tab') }))
      activeTabId.value = tabs.value[0]?.id ?? null
      awaitingReconnect.value = new Set(
        tabs.value.flatMap(tab => listPanes(tab.layout).map(pane => pane.id)),
      )
    } finally {
      restoring.value = false
    }

    await disconnectAll(live)
    await persist()
  }

  /** The current tabs, ready to be stored as a workspace. */
  function currentLayoutJson() {
    return serialize(tabs.value, activeTabId.value)
  }

  function renameTab(tabId: string, name: string) {
    const tab = tabById(tabId)
    if (tab && name.trim()) tab.name = name.trim()
  }

  // Debounced so a burst of resizes or splits results in one write.
  let persistTimer: ReturnType<typeof setTimeout> | undefined
  watch(
    [tabs, activeTabId],
    () => {
      clearTimeout(persistTimer)
      persistTimer = setTimeout(() => void persist(), 400)
    },
    { deep: true },
  )

  return {
    tabs,
    activeTabId,
    awaitingReconnect,
    unread,
    noteOutput,
    clearUnread,
    hasUnread,
    activeTab,
    activePane,
    tabById,
    openTab,
    focusTab,
    closeTab,
    focusPane,
    focusRelativePane,
    splitActivePane,
    closePane,
    setPaneHost,
    attachSession,
    detachSession,
    setSizes,
    renameTab,
    shouldAutoConnect,
    markConnected,
    persist,
    restore,
    applyWorkspace,
    currentLayoutJson,
  }
})
