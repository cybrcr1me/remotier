import { describe, expect, it } from 'vitest'
import {
  actionEntry,
  isAction,
  MAX_ENTRIES,
  appendEntry,
  describeStage,
  errorEntry,
  formatTime,
  infoEntry,
  type LogEntry,
} from './connection-log'

describe('describeStage', () => {
  it('names the host and port while connecting', () => {
    expect(describeStage({ stage: 'connecting', host: 'example.com', port: 2222 }))
      .toBe('Connecting to example.com:2222')
  })

  it('shows the fingerprint that was verified', () => {
    expect(describeStage({ stage: 'hostKeyAccepted', fingerprint: 'SHA256:abc' }))
      .toBe('Host key verified (SHA256:abc)')
  })

  it('names the user and method being offered', () => {
    // Which method was tried is the first thing you need when auth fails.
    expect(describeStage({ stage: 'authenticating', method: 'ssh-agent', username: 'deploy' }))
      .toBe('Authenticating as deploy using ssh-agent')
  })

  it('describes the remaining stages', () => {
    expect(describeStage({ stage: 'authenticated', method: 'password' }))
      .toBe('Authenticated using password')
    expect(describeStage({ stage: 'openingShell', term: 'xterm-256color' }))
      .toBe('Opening shell (TERM=xterm-256color)')
    expect(describeStage({ stage: 'ready' })).toBe('Connected')
  })
})

describe('appendEntry', () => {
  it('keeps entries in order, newest last', () => {
    const entries = appendEntry(appendEntry([], infoEntry('first', 1)), infoEntry('second', 2))
    expect(entries.map(e => e.message)).toEqual(['first', 'second'])
  })

  it('caps the log so a reconnect loop cannot grow it forever', () => {
    let entries: LogEntry[] = []
    for (let i = 0; i < MAX_ENTRIES + 20; i += 1) {
      entries = appendEntry(entries, infoEntry(`entry ${i}`, i))
    }

    expect(entries).toHaveLength(MAX_ENTRIES)
    // The oldest are dropped, not the newest.
    expect(entries.at(-1)?.message).toBe(`entry ${MAX_ENTRIES + 19}`)
  })

  it('does not mutate the array it is given', () => {
    const original = [infoEntry('one', 1)]
    appendEntry(original, infoEntry('two', 2))
    expect(original).toHaveLength(1)
  })

  it('marks errors so they can be styled apart from progress', () => {
    expect(errorEntry('nope').level).toBe('error')
    expect(infoEntry('fine').level).toBe('info')
  })
})

describe('formatTime', () => {
  it('renders a wall clock time', () => {
    expect(formatTime(new Date(2026, 0, 2, 14, 3, 22).getTime())).toBe('14:03:22')
  })
})

describe('a step that waits on the user', () => {
  it('describes the touch stage', () => {
    expect(describeStage({ stage: 'touchRequired' })).toBe('Touch your security key')
  })

  it('marks only the stages the user has to act on', () => {
    expect(isAction({ stage: 'touchRequired' })).toBe(true)
    expect(isAction({ stage: 'connecting', host: 'h', port: 22 })).toBe(false)
    expect(isAction({ stage: 'ready' })).toBe(false)
  })

  it('records an action at its own level', () => {
    const entry = actionEntry('Touch your security key', 1000)
    expect(entry).toEqual({ at: 1000, level: 'action', message: 'Touch your security key' })
  })
})
