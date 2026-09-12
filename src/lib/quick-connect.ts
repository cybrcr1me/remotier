/**
 * Whether the quick connect dialog is open.
 *
 * Shared rather than owned by `TerminalsView`, because the tab bar lives in the window
 * header and is therefore on screen on every page: `+` has to open the dialog from a page
 * whose view does not exist yet, and have it appear once the terminals are back.
 *
 * Deliberately outside the sessions store, for the same reason as `lib/drag.ts`: that store
 * is watched deeply and written to disk, and an open dialog is not session state.
 */

import { ref } from 'vue'

export const quickConnectOpen = ref(false)

export function openQuickConnect() {
  quickConnectOpen.value = true
}
