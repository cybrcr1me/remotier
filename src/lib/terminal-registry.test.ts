/**
 * These exercise the bookkeeping, not xterm: `acquire` is stubbed out at the module
 * boundary so the tests can run without a canvas. What matters here is that a terminal
 * outlives its component and dies with its pane, which is pure map arithmetic.
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    options = {}
    unicode = { activeVersion: '' }
    loadAddon() {}
    open() {}
    onData() {}
    onBinary() {}
    dispose() {}
  },
}))
vi.mock('@xterm/addon-fit', () => ({ FitAddon: class { fit() {} } }))
vi.mock('@xterm/addon-search', () => ({ SearchAddon: class {} }))
vi.mock('@xterm/addon-unicode11', () => ({ Unicode11Addon: class {} }))
vi.mock('@xterm/addon-web-links', () => ({ WebLinksAddon: class {} }))

const { acquire, peek, release, releaseMissing, reset, size } = await import('./terminal-registry')

const options = { fontFamily: 'mono', fontSize: 13, theme: {} }
const handlers = { write: () => {} }

function host() {
  const element = document.createElement('div')
  document.body.appendChild(element)
  return element
}

afterEach(() => {
  reset()
  document.body.innerHTML = ''
})

describe('acquire', () => {
  it('creates one terminal per pane and hands the same one back', () => {
    const first = acquire('pane-1', host(), options, handlers)
    const again = acquire('pane-1', host(), options, handlers)

    expect(again).toBe(first)
    expect(size()).toBe(1)
  })

  it('re-parents the container instead of building a new terminal', () => {
    const before = host()
    const after = host()

    const entry = acquire('pane-1', before, options, handlers)
    expect(before.contains(entry.container)).toBe(true)

    acquire('pane-1', after, options, handlers)
    expect(after.contains(entry.container)).toBe(true)
    expect(before.contains(entry.container)).toBe(false)
  })

  it('keeps the connection state across a move', () => {
    const entry = acquire('pane-1', host(), options, handlers)
    entry.status.value = 'connected'
    entry.sessionId = 'session-1'

    const moved = acquire('pane-1', host(), options, handlers)
    expect(moved.status.value).toBe('connected')
    expect(moved.sessionId).toBe('session-1')
  })

  it('keeps panes apart', () => {
    const a = acquire('pane-1', host(), options, handlers)
    const b = acquire('pane-2', host(), options, handlers)

    expect(a).not.toBe(b)
    expect(size()).toBe(2)
  })
})

describe('releaseMissing', () => {
  it('drops only the panes that are no longer in the layout', () => {
    acquire('pane-1', host(), options, handlers)
    acquire('pane-2', host(), options, handlers)
    acquire('pane-3', host(), options, handlers)

    const dropped = releaseMissing(['pane-1', 'pane-3'])

    expect(dropped).toEqual(['pane-2'])
    expect(peek('pane-1')).toBeDefined()
    expect(peek('pane-2')).toBeUndefined()
    expect(peek('pane-3')).toBeDefined()
  })

  it('keeps a pane that merely moved to another tab', () => {
    const entry = acquire('pane-1', host(), options, handlers)

    // The pane is still somewhere in the app, so nothing is released.
    expect(releaseMissing(['pane-1'])).toEqual([])
    expect(peek('pane-1')).toBe(entry)
  })

  it('clears everything when the last tab closes', () => {
    acquire('pane-1', host(), options, handlers)
    acquire('pane-2', host(), options, handlers)

    expect(releaseMissing([]).sort()).toEqual(['pane-1', 'pane-2'])
    expect(size()).toBe(0)
  })

  it('takes the container out of the DOM with it', () => {
    const element = host()
    const entry = acquire('pane-1', element, options, handlers)

    releaseMissing([])
    expect(element.contains(entry.container)).toBe(false)
  })
})

describe('release', () => {
  it('is a no-op for a pane that never had a terminal', () => {
    expect(() => release('never-existed')).not.toThrow()
  })
})
