/**
 * The arithmetic behind the update panel, kept out of the store so it can be tested
 * without a backend or a clock.
 *
 * Which version is newer is deliberately not decided here: the updater compares the
 * manifest against the running binary in Rust, and a second opinion in TypeScript would
 * be one that could disagree.
 */

import type { UpdateProgress } from '@/lib/types'

/**
 * How long a check holds before the next automatic one.
 *
 * Long, because this app is opened and left running for days: a check every few minutes
 * would be a request to GitHub for every one of those, and a release nobody installs for
 * six hours has cost nothing.
 */
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000

/** True when the last automatic check is old enough to repeat. Never checked counts. */
export function dueForCheck(lastCheckedAt: number | null, now: number): boolean {
  if (lastCheckedAt === null) return true
  // A clock moved backwards - an NTP correction, a time zone change - would otherwise
  // park the next check arbitrarily far into the future.
  if (lastCheckedAt > now) return true
  return now - lastCheckedAt >= CHECK_INTERVAL_MS
}

/**
 * How much of the download is done, 0 to 1, or `null` while the server has not said how
 * big it is. A bar drawn from a guessed total is worse than no bar.
 */
export function downloadFraction(progress: UpdateProgress | null): number | null {
  if (!progress || progress.total === null || progress.total <= 0) return null
  return Math.min(1, progress.downloaded / progress.total)
}

/** Sizes as the user would say them: one decimal, binary units, no trailing ".0". */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—'

  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }

  // Bytes are whole things; anything larger reads better rounded.
  const rounded = unit === 0 ? String(Math.round(value)) : value.toFixed(1).replace(/\.0$/, '')
  return `${rounded} ${units[unit]}`
}

/** "3.2 MB of 48.1 MB", or just what has arrived while the total is unknown. */
export function describeProgress(progress: UpdateProgress | null): string {
  if (!progress) return ''
  if (progress.total === null) return formatBytes(progress.downloaded)
  return `${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)}`
}
