import { describe, expect, it } from 'vitest'
import {
  COLORS,
  HOST_ICONS,
  colorBorder,
  colorSwatch,
  groupIcon,
  hasColor,
  hostIcon,
} from './appearance'

describe('icons', () => {
  it('offers a set worth choosing from', () => {
    expect(HOST_ICONS.length).toBeGreaterThanOrEqual(8)
  })

  it('has unique keys', () => {
    expect(new Set(HOST_ICONS.map(o => o.key)).size).toBe(HOST_ICONS.length)
  })

  it('resolves every offered key to a component', () => {
    for (const option of HOST_ICONS) {
      expect(hostIcon(option.key), option.key).toBe(option.icon)
    }
  })

  it('falls back for an unset or unknown key', () => {
    // A key written by a future version must not render nothing.
    const fallback = hostIcon(null)
    expect(hostIcon(undefined)).toBe(fallback)
    expect(hostIcon('not-a-real-icon')).toBe(fallback)
    expect(hostIcon('')).toBe(fallback)
  })

  it('gives groups a folder by default but honours an explicit choice', () => {
    expect(groupIcon(null)).not.toBe(hostIcon(null))
    expect(groupIcon('database')).toBe(hostIcon('database'))
  })
})

describe('colors', () => {
  it('has unique keys and a default', () => {
    expect(new Set(COLORS.map(o => o.key)).size).toBe(COLORS.length)
    expect(COLORS.some(o => o.key === 'default')).toBe(true)
  })

  it('only ever styles a border', () => {
    for (const option of COLORS) {
      // A filled colour would fight the terminal and the semantic tokens.
      expect(option.border, option.key).toMatch(/^border-/)
    }
  })

  it('resolves every offered key', () => {
    for (const option of COLORS) {
      expect(colorBorder(option.key), option.key).toBe(option.border)
      expect(colorSwatch(option.key), option.key).toBe(option.swatch)
    }
  })

  it('falls back to the neutral border', () => {
    expect(colorBorder(null)).toBe('border-border')
    expect(colorBorder('chartreuse')).toBe('border-border')
    expect(colorBorder('default')).toBe('border-border')
  })

  it('knows when a colour was actually chosen', () => {
    expect(hasColor('red')).toBe(true)
    expect(hasColor('default')).toBe(false)
    expect(hasColor(null)).toBe(false)
    expect(hasColor('nonsense')).toBe(false)
  })
})
