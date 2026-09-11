import { describe, expect, it } from 'vitest'
import {
  COLORS,
  HOST_ICONS,
  catalogKey,
  catalogReference,
  colorBorder,
  colorSwatch,
  colorText,
  colorTint,
  groupIcon,
  hasColor,
  hostIcon,
  tintGradient,
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

describe('catalog icons', () => {
  it('round-trips a reference through its key', () => {
    expect(catalogReference(catalogKey('portainer'))).toBe('portainer')
  })

  it('is no catalog reference for a built-in, unset or empty key', () => {
    expect(catalogReference('database')).toBeNull()
    expect(catalogReference(null)).toBeNull()
    expect(catalogReference(undefined)).toBeNull()
    expect(catalogReference('selfhst:')).toBeNull()
  })

  it('has the default built-in icon to show until the image arrives', () => {
    expect(hostIcon(catalogKey('portainer'))).toBe(hostIcon(null))
    expect(groupIcon(catalogKey('portainer'))).toBe(groupIcon(null))
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
      expect(colorTint(option.key), option.key).toBe(option.tint)
      expect(colorText(option.key), option.key).toBe(option.text)
    }
  })

  it('falls back to the neutral tint and a muted icon', () => {
    expect(colorTint('chartreuse')).toBe(colorTint('default'))
    expect(colorText(null)).toBe(colorText('default'))
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

describe('tint gradient', () => {
  it('gives every colour the value its swatch paints', () => {
    for (const option of COLORS.filter(o => o.key !== 'default')) {
      // `bg-red-500` paints `var(--color-red-500)`; a gradient stop has to be that colour.
      expect(option.value, option.key).toBe(`var(--color-${option.swatch.replace(/^bg-/, '')})`)
    }
  })

  it('blends the colours in order, with no colour as the neutral surface', () => {
    const { backgroundImage } = tintGradient(['red', null, 'blue'])
    const fill = backgroundImage.split('), linear-gradient')[0] ?? ''

    const red = fill.indexOf('var(--color-red-500) 15%')
    const neutral = fill.indexOf('var(--accent)')
    const blue = fill.indexOf('var(--color-blue-500) 15%')
    expect(red).toBeGreaterThan(-1)
    expect(neutral).toBeGreaterThan(red)
    expect(blue).toBeGreaterThan(neutral)
    expect(backgroundImage).toContain('var(--border)')
  })

  it('paints the border gradient in the border only', () => {
    // The fill is opaque and clipped to the padding box, so the border layer beneath it
    // shows through nowhere else.
    const style = tintGradient(['red', 'blue'])
    expect(style.backgroundClip).toBe('padding-box, border-box')
    expect(style.backgroundOrigin).toBe('border-box')
    expect(style.backgroundImage).toContain('var(--color-red-500) 15%, var(--background)')
  })

  it('treats a default or unknown colour as none', () => {
    expect(tintGradient(['default', 'chartreuse']).backgroundImage).not.toContain('--color-')
  })
})
