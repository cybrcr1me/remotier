/**
 * Which tab of the settings view is open.
 *
 * Carried in the route as `?tab=` rather than held by the view, so a link elsewhere in the
 * app can open a particular tab - the sync status in the sidebar opens Sync. State kept in
 * the view would only be read when it mounts, so the same link would do nothing while
 * settings was already on screen with another tab open.
 */

import type { LocationQueryValue } from 'vue-router'

export const SETTINGS_TABS = ['general', 'placeholders', 'sync'] as const

export type SettingsTab = (typeof SETTINGS_TABS)[number]

/** The tab a `tab` query value names. Missing or unrecognised opens General. */
export function settingsTabFrom(
  value: LocationQueryValue | LocationQueryValue[] | undefined,
): SettingsTab {
  const raw = Array.isArray(value) ? value[0] : value
  return SETTINGS_TABS.find(tab => tab === raw) ?? 'general'
}

/**
 * A link to the settings view with `tab` open. General carries no query, since it is the
 * tab settings opens on anyway.
 */
export function settingsRoute(tab: SettingsTab): { path: string, query?: { tab: SettingsTab } } {
  return tab === 'general' ? { path: '/settings' } : { path: '/settings', query: { tab } }
}
