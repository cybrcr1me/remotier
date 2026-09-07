import { describe, expect, it } from 'vitest'
import { matchShortcut } from './shortcuts'

function key(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return new KeyboardEvent('keydown', {
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    ...init,
  })
}

describe('matchShortcut', () => {
  it('ignores plain keystrokes so they reach the shell', () => {
    expect(matchShortcut(key({ key: 'd' }))).toBeNull()
    expect(matchShortcut(key({ key: 'k' }))).toBeNull()
  })

  it('accepts Command on macOS and Control elsewhere', () => {
    expect(matchShortcut(key({ key: 't', metaKey: true }))).toBe('newTab')
    expect(matchShortcut(key({ key: 't', ctrlKey: true }))).toBe('newTab')
  })

  it('maps the split shortcuts to opposite axes', () => {
    expect(matchShortcut(key({ key: 'd', metaKey: true }))).toBe('splitRow')
    expect(matchShortcut(key({ key: 'd', metaKey: true, shiftKey: true }))).toBe('splitCol')
  })

  it('maps pane cycling to the bracket keys', () => {
    expect(matchShortcut(key({ key: ']', metaKey: true }))).toBe('nextPane')
    expect(matchShortcut(key({ key: '[', metaKey: true }))).toBe('previousPane')
  })

  it('maps quick connect and close pane', () => {
    expect(matchShortcut(key({ key: 'k', metaKey: true }))).toBe('quickConnect')
    expect(matchShortcut(key({ key: 'w', metaKey: true }))).toBe('closePane')
  })

  it('is case insensitive, so caps lock does not break it', () => {
    expect(matchShortcut(key({ key: 'K', metaKey: true }))).toBe('quickConnect')
  })

  it('leaves Alt combinations alone', () => {
    // Alt+key produces characters the shell needs, and is meta in readline.
    expect(matchShortcut(key({ key: 'd', metaKey: true, altKey: true }))).toBeNull()
  })

  it('does not claim shifted variants it has no action for', () => {
    expect(matchShortcut(key({ key: 't', metaKey: true, shiftKey: true }))).toBeNull()
    expect(matchShortcut(key({ key: 'w', metaKey: true, shiftKey: true }))).toBeNull()
  })

  it('ignores unrelated modified keys such as copy and paste', () => {
    expect(matchShortcut(key({ key: 'c', metaKey: true }))).toBeNull()
    expect(matchShortcut(key({ key: 'v', metaKey: true }))).toBeNull()
  })
})

describe('moving the active tab', () => {
  it('maps shifted brackets to a tab move, not a pane focus', () => {
    expect(matchShortcut(key({ key: ']', metaKey: true, shiftKey: true }))).toBe('moveTabRight')
    expect(matchShortcut(key({ key: '[', metaKey: true, shiftKey: true }))).toBe('moveTabLeft')
  })

  it('accepts the shifted characters the keyboard actually sends', () => {
    // Most layouts report `}` rather than `]` when shift is held.
    expect(matchShortcut(key({ key: '}', metaKey: true, shiftKey: true }))).toBe('moveTabRight')
    expect(matchShortcut(key({ key: '{', metaKey: true, shiftKey: true }))).toBe('moveTabLeft')
  })

  it('leaves the unshifted brackets on pane focus', () => {
    expect(matchShortcut(key({ key: ']', metaKey: true }))).toBe('nextPane')
    expect(matchShortcut(key({ key: '[', metaKey: true }))).toBe('previousPane')
  })
})
