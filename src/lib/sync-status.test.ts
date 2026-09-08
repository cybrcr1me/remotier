import { describe, expect, it } from 'vitest'

import {
  FORMAT_VERSION,
  instanceLabel,
  instanceProblem,
  isDefaultInstance,
  lastSyncLabel,
  normaliseInstanceUrl,
  summarise,
} from './sync-status'
import type { SyncStatus } from './types'

const base: SyncStatus = {
  signedIn: true,
  instanceUrl: 'https://sync.remotier.app',
  email: 'ada@example.com',
  deviceId: 'dev-a',
  lastSyncAt: null,
  pending: 0,
  syncing: false,
  applied: 0,
  error: null,
}

describe('normaliseInstanceUrl', () => {
  it('assumes https for a bare host', () => {
    // What people type. Assuming http instead would quietly downgrade the transport.
    expect(normaliseInstanceUrl('sync.example.com')).toBe('https://sync.example.com')
  })

  it('keeps an explicit scheme, including http for a local instance', () => {
    expect(normaliseInstanceUrl('http://localhost:8787')).toBe('http://localhost:8787')
  })

  it('drops trailing slashes so two spellings are one instance', () => {
    expect(normaliseInstanceUrl('https://a.example/')).toBe('https://a.example')
    expect(normaliseInstanceUrl('https://a.example///')).toBe('https://a.example')
  })

  it('keeps a path, for an instance behind a prefix', () => {
    expect(normaliseInstanceUrl('https://a.example/sync')).toBe('https://a.example/sync')
  })

  it('refuses anything that is not http', () => {
    expect(normaliseInstanceUrl('ftp://a.example')).toBeNull()
    expect(normaliseInstanceUrl('javascript:alert(1)')).toBeNull()
    expect(normaliseInstanceUrl('')).toBeNull()
    expect(normaliseInstanceUrl('   ')).toBeNull()
  })
})

describe('isDefaultInstance', () => {
  const fallback = 'https://sync.remotier.app'

  it('treats no instance as the default, so a fresh install shows the simple form', () => {
    expect(isDefaultInstance(null, fallback)).toBe(true)
  })

  it('ignores a trailing slash', () => {
    expect(isDefaultInstance('https://sync.remotier.app/', fallback)).toBe(true)
  })

  it('spots a self-hosted one', () => {
    expect(isDefaultInstance('https://sync.example.com', fallback)).toBe(false)
  })
})

describe('instanceLabel', () => {
  it('shows the host, because the scheme is noise in a sidebar', () => {
    expect(instanceLabel('https://sync.example.com/x')).toBe('sync.example.com')
    expect(instanceLabel('http://localhost:8787')).toBe('localhost:8787')
  })

  it('falls back to the raw string rather than throwing', () => {
    expect(instanceLabel('not a url')).toBe('not a url')
    expect(instanceLabel(null)).toBe('')
  })
})

describe('lastSyncLabel', () => {
  const now = 1_700_000_000_000

  it('is coarse on purpose', () => {
    expect(lastSyncLabel(now - 5_000, now)).toBe('just now')
    expect(lastSyncLabel(now - 240_000, now)).toBe('4m ago')
    expect(lastSyncLabel(now - 7_200_000, now)).toBe('2h ago')
    expect(lastSyncLabel(now - 90_000_000, now)).toBe('yesterday')
    expect(lastSyncLabel(now - 600_000_000, now)).toBe('7d ago')
  })

  it('says never rather than showing an epoch', () => {
    expect(lastSyncLabel(null, now)).toBe('never')
  })

  it('does not report a sync in the future when the clock moves back', () => {
    // NTP corrections happen; "in -3s" would be nonsense on screen.
    expect(lastSyncLabel(now + 3_000, now)).toBe('just now')
  })
})

describe('summarise', () => {
  it('leads with the error, because that is the actionable part', () => {
    expect(summarise({ ...base, error: 'could not reach the sync server' }))
      .toBe('could not reach the sync server')
  })

  it('reports pending work over a stale success', () => {
    expect(summarise({ ...base, pending: 1, lastSyncAt: 0 })).toBe('1 change waiting.')
    expect(summarise({ ...base, pending: 4, lastSyncAt: 0 })).toBe('4 changes waiting.')
  })

  it('says so plainly when signed out', () => {
    expect(summarise({ ...base, signedIn: false })).toBe('Not signed in.')
  })

  it('prefers the in-flight state to a count', () => {
    expect(summarise({ ...base, syncing: true, pending: 3 })).toBe('Syncing.')
  })
})

describe('instanceProblem', () => {
  const info = { name: 'x', version: '0.1.0', formatVersions: [1], registrationOpen: true }

  it('passes an instance that speaks this format', () => {
    expect(instanceProblem(info, FORMAT_VERSION)).toBeNull()
  })

  it('explains a mismatch instead of letting it fail as a decryption error', () => {
    const problem = instanceProblem({ ...info, formatVersions: [2, 3] }, 1)
    expect(problem).toContain('2, 3')
    expect(problem).toContain('1')
  })
})
