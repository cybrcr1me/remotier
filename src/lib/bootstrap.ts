/** Loads everything the app needs before the first paint of real content. */

import { toast } from 'vue-sonner'
import { errorMessage } from '@/lib/ipc'
import { router } from '@/router'
import { settingsRoute } from '@/lib/settings-tabs'
import { useUpdatesStore } from '@/stores/updates'
import { useCredentialsStore } from '@/stores/credentials'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { useSettingsStore } from '@/stores/settings'
import { useSyncStore } from '@/stores/sync'
import { useVarsStore } from '@/stores/vars'

/** Re-read everything sync could have changed underneath the stores. */
export async function reloadSynced() {
  const inventory = useInventoryStore()
  const credentials = useCredentialsStore()
  const settings = useSettingsStore()
  const vars = useVarsStore()

  await Promise.all([inventory.load(), credentials.load(), settings.load(), vars.load()])
}

export async function bootstrap() {
  const inventory = useInventoryStore()
  const credentials = useCredentialsStore()
  const settings = useSettingsStore()
  const vars = useVarsStore()
  const sessions = useSessionsStore()
  const sync = useSyncStore()

  try {
    await Promise.all([inventory.load(), credentials.load(), settings.load(), vars.load()])
  } catch (e) {
    toast.error('Could not load your data', { description: errorMessage(e) })
  }

  // Sync is optional and must never block startup: a failure here leaves the app exactly
  // as it was before sync existed.
  try {
    await sync.load()
    await sync.watch(() => reloadSynced())
  } catch (e) {
    console.warn('sync unavailable', errorMessage(e))
  }

  // After settings, so the auto-reconnect preference is known.
  try {
    await sessions.restore(settings.get('session.autoReconnect') === 'true')
  } catch (e) {
    toast.error('Could not restore your tabs', { description: errorMessage(e) })
  }

  startUpdateChecks(settings.get('updates.autoCheck') === 'true')
}

/**
 * Look for a new version, and keep looking.
 *
 * Announced, never installed: an update restarts the app and takes every SSH session with
 * it, so the toast leads to the panel with the button rather than being the button. A
 * development build is left alone - its version is whatever the working tree says, and
 * replacing it with a release would throw the build away.
 */
function startUpdateChecks(enabled: boolean) {
  if (!enabled || import.meta.env.DEV) return

  const updates = useUpdatesStore()

  void updates.watch().catch(e => console.warn('update progress unavailable', errorMessage(e)))

  updates.startAutoChecks((info) => {
    toast('Remotier ' + info.version + ' is available', {
      description: 'Installing restarts the app and ends every open session.',
      action: {
        label: 'Show',
        onClick: () => void router.push(settingsRoute('updates')),
      },
    })
  })
}
