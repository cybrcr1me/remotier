import { describe, expect, it } from 'vitest'
import {
  CHECK_INTERVAL_MS,
  describeProgress,
  downloadFraction,
  dueForCheck,
  formatBytes,
} from './updates'

describe('dueForCheck', () => {
  it('checks when nothing has been checked yet', () => {
    expect(dueForCheck(null, 1_000)).toBe(true)
  })

  it('holds until the interval has passed', () => {
    const now = 10 * CHECK_INTERVAL_MS
    expect(dueForCheck(now - CHECK_INTERVAL_MS + 1, now)).toBe(false)
    expect(dueForCheck(now - CHECK_INTERVAL_MS, now)).toBe(true)
  })

  it('checks when the clock has moved backwards', () => {
    // An NTP correction or a time zone change would otherwise park the next check
    // however far into the future the clock had jumped.
    expect(dueForCheck(5_000, 1_000)).toBe(true)
  })
})

describe('downloadFraction', () => {
  it('is null while the size is unknown', () => {
    expect(downloadFraction(null)).toBeNull()
    expect(downloadFraction({ downloaded: 10, total: null })).toBeNull()
  })

  it('is the share of the total', () => {
    expect(downloadFraction({ downloaded: 25, total: 100 })).toBe(0.25)
  })

  it('never exceeds one', () => {
    // The last chunk can overshoot a total that was only ever a header.
    expect(downloadFraction({ downloaded: 120, total: 100 })).toBe(1)
  })
})

describe('formatBytes', () => {
  it('keeps bytes whole and scales the rest', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2 KB')
    expect(formatBytes(1024 * 1024 * 3.25)).toBe('3.3 MB')
  })

  it('says nothing useful rather than something wrong', () => {
    expect(formatBytes(Number.NaN)).toBe('—')
  })
})

describe('describeProgress', () => {
  it('names both ends when the total is known', () => {
    expect(describeProgress({ downloaded: 1024 * 1024, total: 4 * 1024 * 1024 }))
      .toBe('1 MB of 4 MB')
  })

  it('reports only what has arrived otherwise', () => {
    expect(describeProgress({ downloaded: 1024, total: null })).toBe('1 KB')
  })
})
