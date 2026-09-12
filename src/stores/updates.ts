import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'

import { errorMessage, ipc } from '@/lib/ipc'
import type { UpdateInfo, UpdateProgress } from '@/lib/types'
import { dueForCheck } from '@/lib/updates'

/** Emitted by `commands::updates` while the new version downloads. */
const PROGRESS_EVENT = 'update://progress'

/**
 * How often the timer looks, which is not how often it checks: `dueForCheck` decides
 * that. A laptop that slept through the interval fires no timer at all, so a tick short
 * enough to catch the wake-up asks the question and usually answers "not yet".
 */
const TICK_MS = 15 * 60 * 1000

export type UpdateState = 'idle' | 'checking' | 'available' | 'downloading' | 'failed'

export const useUpdatesStore = defineStore('updates', () => {
  const state = ref<UpdateState>('idle')
  /** The version on offer, or null when this is the newest. */
  const available = ref<UpdateInfo | null>(null)
  const progress = ref<UpdateProgress | null>(null)
  const error = ref<string | null>(null)
  const lastCheckedAt = ref<number | null>(null)
  const version = ref('')

  const busy = computed(() => state.value === 'checking' || state.value === 'downloading')

  async function loadVersion() {
    try {
      version.value = await getVersion()
    } catch (e) {
      // Only ever shown as a label; nothing depends on knowing it.
      console.warn('could not read the app version', errorMessage(e))
    }
  }

  /**
   * Ask whether there is a newer version.
   *
   * A failure is recorded rather than thrown: the machine being offline is the usual
   * reason, and it is not something the user has to act on. The caller decides whether
   * anyone is told - a check they asked for should say so, one on a timer should not.
   */
  async function check(): Promise<UpdateInfo | null> {
    if (state.value === 'downloading') return available.value

    state.value = 'checking'
    error.value = null

    try {
      const found = await ipc.updateCheck()
      lastCheckedAt.value = Date.now()
      available.value = found
      state.value = found ? 'available' : 'idle'
      return found
    } catch (e) {
      error.value = errorMessage(e)
      state.value = 'failed'
      return null
    }
  }

  /**
   * Download the new version and restart into it.
   *
   * This ends every SSH session the app holds, so nothing calls it on its own: the button
   * that does says as much. On macOS and Linux it never resolves - the process is
   * replaced - and on Windows the installer closes the app itself.
   */
  async function install() {
    if (state.value === 'downloading') return

    state.value = 'downloading'
    progress.value = { downloaded: 0, total: null }
    error.value = null

    try {
      await ipc.updateInstall()
    } catch (e) {
      error.value = errorMessage(e)
      state.value = 'failed'
      progress.value = null
    }
  }

  /** Follow the download. Returns the unlisten, though nothing outlives the window. */
  async function watch() {
    return listen<UpdateProgress>(PROGRESS_EVENT, ({ payload }) => {
      progress.value = payload
    })
  }

  /**
   * Check now, then keep checking. `onFound` is called only when a check that the user
   * did not ask for turns something up, so the announcement is made once.
   */
  function startAutoChecks(onFound: (info: UpdateInfo) => void) {
    const run = async () => {
      if (!dueForCheck(lastCheckedAt.value, Date.now())) return
      const found = await check()
      if (found) onFound(found)
    }

    void run()
    return window.setInterval(() => void run(), TICK_MS)
  }

  return {
    state,
    available,
    progress,
    error,
    lastCheckedAt,
    version,
    busy,
    loadVersion,
    check,
    install,
    watch,
    startAutoChecks,
  }
})
