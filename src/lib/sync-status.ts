/**
 * Pure formatting for the sync panel, kept out of the component so it can be tested
 * without a DOM.
 */

import type { InstanceInfo, SyncHistoryEntry, SyncStatus } from './types'

/** A URL is usable if it parses and speaks http(s). */
export function normaliseInstanceUrl(raw: string): string | null {
  const trimmed = raw.trim()
  if (!trimmed) return null

  // A scheme that is present and wrong must be rejected, not repaired: prepending
  // `https://` to `ftp://a.example` yields something `new URL` parses happily as the host
  // `ftp`, which would then be probed.
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(trimmed)
  if (scheme && !/^https?$/i.test(scheme[1])) return null

  // A bare host is what people type. Assume https rather than rejecting it - and https
  // rather than http, so a typo never quietly downgrades the transport.
  const withScheme = scheme ? trimmed : `https://${trimmed}`

  try {
    const url = new URL(withScheme)
    if (url.protocol !== 'https:' && url.protocol !== 'http:') return null
    if (!url.hostname) return null
    return url.origin + url.pathname.replace(/\/+$/, '')
  } catch {
    return null
  }
}

/** Is this the instance the app ships with, rather than one the user typed? */
export function isDefaultInstance(url: string | null, fallback: string): boolean {
  if (!url) return true
  return normaliseInstanceUrl(url) === normaliseInstanceUrl(fallback)
}

/** Just the host, for the sidebar, where the scheme is noise. */
export function instanceLabel(url: string | null): string {
  if (!url) return ''
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

/**
 * "just now" / "4m ago" / "yesterday". Coarse on purpose: the exact second a sync
 * happened is never what the reader wants to know.
 */
export function lastSyncLabel(at: number | null, now = Date.now()): string {
  if (at === null) return 'never'

  const seconds = Math.floor((now - at) / 1000)
  // A clock nudged backwards between the sync and the render would otherwise read as a
  // sync in the future.
  if (seconds < 45) return 'just now'
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`
  if (seconds < 86_400) return `${Math.round(seconds / 3600)}h ago`
  if (seconds < 172_800) return 'yesterday'
  return `${Math.round(seconds / 86_400)}d ago`
}

/** One line under the status square. State first, then what to do about it. */
export function summarise(status: SyncStatus): string {
  if (!status.signedIn) return 'Not signed in.'
  if (status.error) return status.error
  if (status.syncing) return 'Syncing.'
  if (status.pending > 0) {
    return `${status.pending} change${status.pending === 1 ? '' : 's'} waiting.`
  }
  return `Synced ${lastSyncLabel(status.lastSyncAt)}.`
}

/** What a record kind is called in the activity list. */
const KIND_LABELS: Record<string, string> = {
  host: 'Host',
  group: 'Group',
  identity: 'Identity',
  var_def: 'Placeholder',
  workspace: 'Workspace',
  setting: 'Setting',
  device_layout: 'Tabs',
}

/**
 * A kind a newer build sent falls back to the raw key rather than being hidden: the row
 * still happened, and "var_def" tells the reader more than an empty cell.
 */
export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? kind
}

/**
 * What to show for a record whose name is not known.
 *
 * Only a tombstone this machine pushed gets here - the row was deleted before the cycle
 * that sent it. The id is truncated because the whole uuid is never recognisable anyway
 * and a full one pushes the timestamp off the row.
 */
export function entryLabel(entry: SyncHistoryEntry): string {
  return entry.label ?? entry.recordId.slice(0, 8)
}

/** Past tense, from this device's point of view. */
export function entryVerb(entry: SyncHistoryEntry): string {
  if (entry.action === 'deleted') return entry.direction === 'push' ? 'Deleted' : 'Removed here'
  return entry.direction === 'push' ? 'Sent' : 'Received'
}

/** Why an instance cannot be used, or `null` if it can. */
export function instanceProblem(info: InstanceInfo, formatVersion: number): string | null {
  if (!info.formatVersions.includes(formatVersion)) {
    return `That instance speaks format ${info.formatVersions.join(', ')}; this build speaks ${formatVersion}.`
  }
  return null
}

/** The envelope format this build writes. Must match the Rust `FORMAT_VERSION`. */
export const FORMAT_VERSION = 1
