/**
 * A record of what happened during a connection attempt.
 *
 * Kept so a failure leaves something to read afterwards: without it a connection that
 * dies during authentication just vanishes, with no indication of how far it got.
 */

import type { ConnectStage } from '@/lib/types'

export interface LogEntry {
  at: number
  /** `action` is waiting on the user, not on us, and is called out as such. */
  level: 'info' | 'action' | 'error'
  message: string
}

/** Turn a stage into the line shown in the panel. */
export function describeStage(stage: ConnectStage): string {
  switch (stage.stage) {
    case 'connecting':
      return `Connecting to ${stage.host}:${stage.port}`
    case 'hostKeyAccepted':
      return `Host key verified (${stage.fingerprint})`
    case 'authenticating':
      return `Authenticating as ${stage.username} using ${stage.method}`
    case 'touchRequired':
      return 'Touch your security key'
    case 'authenticated':
      return `Authenticated using ${stage.method}`
    case 'openingShell':
      return `Opening shell (TERM=${stage.term})`
    case 'ready':
      return 'Connected'
    default:
      return 'Working…'
  }
}

/** Newest last, capped so a reconnect loop cannot grow without bound. */
export const MAX_ENTRIES = 50

export function appendEntry(entries: LogEntry[], entry: LogEntry): LogEntry[] {
  const next = [...entries, entry]
  return next.length > MAX_ENTRIES ? next.slice(next.length - MAX_ENTRIES) : next
}

export function infoEntry(message: string, at = Date.now()): LogEntry {
  return { at, level: 'info', message }
}

export function errorEntry(message: string, at = Date.now()): LogEntry {
  return { at, level: 'error', message }
}

/** Something the user has to do before anything else can happen. */
export function actionEntry(message: string, at = Date.now()): LogEntry {
  return { at, level: 'action', message }
}

/** Stages that are waiting on the user rather than on the connection. */
export function isAction(stage: ConnectStage): boolean {
  return stage.stage === 'touchRequired'
}

/** `14:03:22`, which is enough to correlate with a server log. */
export function formatTime(at: number): string {
  return new Date(at).toTimeString().slice(0, 8)
}
