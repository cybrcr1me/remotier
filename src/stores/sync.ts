import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { platform, hostname } from '@tauri-apps/plugin-os'

import { ipc, errorMessage } from '@/lib/ipc'
import type { DeviceLayout, InstanceInfo, SyncStatus } from '@/lib/types'

/**
 * The instance shipped for people who do not want to run their own. The sign-in screen
 * shows it as text with a Change control rather than an empty field, because most users
 * will never touch it and an empty required field reads as work to do.
 */
export const DEFAULT_INSTANCE = 'https://sync.remotier.app'

/** The backend emits this after every cycle and every session change. */
const STATUS_EVENT = 'sync://status'

const EMPTY: SyncStatus = {
  signedIn: false,
  instanceUrl: null,
  email: null,
  deviceId: '',
  lastSyncAt: null,
  pending: 0,
  syncing: false,
  applied: 0,
  error: null,
}

export const useSyncStore = defineStore('sync', () => {
  const status = ref<SyncStatus>({ ...EMPTY })
  const busy = ref(false)
  /** Shown once after registering, then dropped. Never persisted anywhere. */
  const recoveryCode = ref<string | null>(null)

  const signedIn = computed(() => status.value.signedIn)

  /**
   * What the status square shows. Four states, matching the connection indicator
   * convention: lime connected, amber working, red failed, grey off.
   */
  const indicator = computed<'synced' | 'syncing' | 'error' | 'off'>(() => {
    if (!status.value.signedIn) return 'off'
    if (status.value.syncing) return 'syncing'
    if (status.value.error) return 'error'
    return 'synced'
  })

  async function load() {
    status.value = await ipc.syncStatus()
  }

  /**
   * Subscribe to backend status. Returns the unlisten handle.
   *
   * The frontend has no timer of its own: the engine decides when to sync and says what
   * happened, the same way session liveness is reported rather than polled.
   */
  async function watch(onApplied: () => void | Promise<void>) {
    return listen<SyncStatus | null>(STATUS_EVENT, async event => {
      if (!event.payload) return
      status.value = event.payload
      // The engine wrote rows underneath the stores. Reload only when it actually did,
      // rather than on every tick.
      if (event.payload.applied > 0) await onApplied()
    })
  }

  /** A name for this machine, so the device list is readable. */
  async function deviceName(): Promise<string> {
    try {
      return (await hostname()) ?? platform()
    } catch {
      return platform()
    }
  }

  /** Other machines that have saved tabs. Empty while signed out. */
  async function devices(): Promise<DeviceLayout[]> {
    if (!status.value.signedIn) return []
    return ipc.syncDevices()
  }

  async function deviceLayout(deviceId: string): Promise<string> {
    return ipc.syncDeviceLayout(deviceId)
  }

  async function probe(url: string): Promise<InstanceInfo> {
    return ipc.syncProbeInstance(url)
  }

  async function register(url: string, email: string, password: string) {
    busy.value = true
    try {
      recoveryCode.value = await ipc.syncRegister(url, email, password, await deviceName())
      await load()
    } finally {
      busy.value = false
    }
  }

  async function login(url: string, email: string, password: string) {
    busy.value = true
    try {
      status.value = await ipc.syncLogin(url, email, password, await deviceName())
    } finally {
      busy.value = false
    }
  }

  async function recover(url: string, email: string, code: string) {
    busy.value = true
    try {
      status.value = await ipc.syncRecover(url, email, code, await deviceName())
    } finally {
      busy.value = false
    }
  }

  async function logout() {
    busy.value = true
    try {
      status.value = await ipc.syncLogout()
    } finally {
      busy.value = false
    }
  }

  async function syncNow() {
    busy.value = true
    try {
      status.value = await ipc.syncNow()
      return status.value.applied > 0
    } catch (e) {
      // A failed sync is reported on the status, not thrown at the caller: the panel
      // shows it beside the last successful sync rather than losing both.
      status.value = { ...status.value, error: errorMessage(e) }
      return false
    } finally {
      busy.value = false
    }
  }

  function dismissRecoveryCode() {
    recoveryCode.value = null
  }

  return {
    status,
    busy,
    recoveryCode,
    signedIn,
    indicator,
    load,
    watch,
    devices,
    deviceLayout,
    probe,
    register,
    login,
    recover,
    logout,
    syncNow,
    dismissRecoveryCode,
  }
})
