/** Loads everything the app needs before the first paint of real content. */

import { toast } from 'vue-sonner'
import { errorMessage } from '@/lib/ipc'
import { useCredentialsStore } from '@/stores/credentials'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { useSettingsStore } from '@/stores/settings'
import { useVarsStore } from '@/stores/vars'

export async function bootstrap() {
  const inventory = useInventoryStore()
  const credentials = useCredentialsStore()
  const settings = useSettingsStore()
  const vars = useVarsStore()
  const sessions = useSessionsStore()

  try {
    await Promise.all([inventory.load(), credentials.load(), settings.load(), vars.load()])
  } catch (e) {
    toast.error('Could not load your data', { description: errorMessage(e) })
  }

  // After settings, so the auto-reconnect preference is known.
  try {
    await sessions.restore(settings.get('session.autoReconnect') === 'true')
  } catch (e) {
    toast.error('Could not restore your tabs', { description: errorMessage(e) })
  }
}
