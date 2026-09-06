import { describe, expect, it, vi } from 'vitest'
import {
  buildTerminalTheme,
  domColorResolver,
  isDarkTheme,
  withAlpha,
  type ColorResolver,
} from './terminal-theme'

const resolved: Record<string, string> = {
  'var(--background)': 'rgb(10, 10, 10)',
  'var(--foreground)': 'rgb(250, 250, 250)',
  'var(--primary)': 'rgb(120, 180, 255)',
  'var(--accent)': 'rgb(60, 60, 60)',
}

const resolver: ColorResolver = expression => resolved[expression] ?? null
const nullResolver: ColorResolver = () => null

describe('buildTerminalTheme', () => {
  it('takes its chrome colours from the design tokens', () => {
    const theme = buildTerminalTheme(resolver, true)

    expect(theme.background).toBe('rgb(10, 10, 10)')
    expect(theme.foreground).toBe('rgb(250, 250, 250)')
    expect(theme.cursor).toBe('rgb(120, 180, 255)')
    // The cursor glyph sits on the cursor block, so it has to invert against it.
    expect(theme.cursorAccent).toBe('rgb(10, 10, 10)')
  })

  it('makes the selection translucent so text stays readable', () => {
    const theme = buildTerminalTheme(resolver, true)

    expect(theme.selectionBackground).toBe('rgba(60, 60, 60, 0.35)')
    expect(theme.selectionForeground).toBeUndefined()
  })

  it('supplies a full sixteen colour ANSI palette', () => {
    const theme = buildTerminalTheme(resolver, true) as Record<string, unknown>

    for (const slot of ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white']) {
      expect(theme[slot], slot).toMatch(/^#[0-9a-f]{6}$/i)
      const bright = `bright${slot[0].toUpperCase()}${slot.slice(1)}`
      expect(theme[bright], bright).toMatch(/^#[0-9a-f]{6}$/i)
    }
  })

  it('uses different ANSI palettes for light and dark', () => {
    const dark = buildTerminalTheme(resolver, true)
    const light = buildTerminalTheme(resolver, false)

    expect(dark.red).not.toBe(light.red)
  })

  it('falls back to readable colours when tokens cannot be resolved', () => {
    const dark = buildTerminalTheme(nullResolver, true)
    const light = buildTerminalTheme(nullResolver, false)

    // A theme without a background would render the terminal transparent.
    expect(dark.background).toBe('#0a0a0a')
    expect(dark.foreground).toBe('#fafafa')
    expect(light.background).toBe('#ffffff')
    expect(light.foreground).toBe('#0a0a0a')
  })
})

describe('withAlpha', () => {
  it('adds alpha to an rgb colour', () => {
    expect(withAlpha('rgb(1, 2, 3)', 0.5)).toBe('rgba(1, 2, 3, 0.5)')
  })

  it('adds alpha to a hex colour', () => {
    expect(withAlpha('#0a141e', 0.25)).toBe('rgba(10, 20, 30, 0.25)')
  })

  it('passes through anything it does not understand', () => {
    // Better a solid colour than an invalid one xterm would reject outright.
    expect(withAlpha('oklch(0.2 0 0)', 0.5)).toBe('oklch(0.2 0 0)')
  })
})

describe('domColorResolver', () => {
  it('returns null when the environment cannot evaluate the colour', () => {
    // happy-dom does not compute oklch, which is exactly the fallback path.
    expect(domColorResolver()('var(--nope)')).toBeNull()
  })

  it('cleans up its probe element', () => {
    const before = document.body.childElementCount
    domColorResolver()('var(--background)')
    expect(document.body.childElementCount).toBe(before)
  })

  it('removes the probe even when reading the colour throws', () => {
    const before = document.body.childElementCount
    const spy = vi.spyOn(globalThis, 'getComputedStyle').mockImplementation(() => {
      throw new Error('boom')
    })

    expect(() => domColorResolver()('var(--background)')).toThrow('boom')
    expect(document.body.childElementCount).toBe(before)

    spy.mockRestore()
  })
})

describe('isDarkTheme', () => {
  it('follows the dark class on the root element', () => {
    const root = document.createElement('html')
    expect(isDarkTheme(root)).toBe(false)

    root.classList.add('dark')
    expect(isDarkTheme(root)).toBe(true)
  })
})
