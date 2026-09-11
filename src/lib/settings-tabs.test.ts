import { describe, expect, it } from 'vitest'
import { SETTINGS_TABS, settingsRoute, settingsTabFrom } from './settings-tabs'

describe('settingsTabFrom', () => {
  it('opens the tab the query names', () => {
    expect(settingsTabFrom('sync')).toBe('sync')
    expect(settingsTabFrom('placeholders')).toBe('placeholders')
  })

  it('opens General when the query is missing or names no tab', () => {
    expect(settingsTabFrom(undefined)).toBe('general')
    expect(settingsTabFrom(null)).toBe('general')
    expect(settingsTabFrom('billing')).toBe('general')
  })

  it('reads the first value of a repeated query', () => {
    expect(settingsTabFrom(['sync', 'general'])).toBe('sync')
  })
})

describe('settingsRoute', () => {
  it('names the tab in the query', () => {
    expect(settingsRoute('sync')).toEqual({ path: '/settings', query: { tab: 'sync' } })
  })

  it('leaves the query off for General', () => {
    expect(settingsRoute('general')).toEqual({ path: '/settings' })
  })

  it('opens every tab it links to', () => {
    // The link and the view must agree, or a link lands on the wrong tab without a word.
    for (const tab of SETTINGS_TABS) {
      expect(settingsTabFrom(settingsRoute(tab).query?.tab)).toBe(tab)
    }
  })
})
