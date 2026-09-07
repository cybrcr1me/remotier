/**
 * Terminal colours derived from the app's own design tokens, so the terminal follows the
 * shadcn theme (including a light/dark switch) instead of being a differently-coloured
 * rectangle in the middle of the window.
 *
 * The tokens are `oklch(...)`, which xterm's colour parser does not understand, so values
 * are round-tripped through a probe element and read back as `rgb(...)`.
 */

import type { ITheme } from '@xterm/xterm'

/**
 * ANSI palettes. These are data, not design tokens - the sixteen ANSI slots have fixed
 * meanings that a semantic token set does not express.
 */
const ANSI_DARK = {
  black: '#1e1e2e',
  red: '#f38ba8',
  green: '#a6e3a1',
  yellow: '#f9e2af',
  blue: '#89b4fa',
  magenta: '#f5c2e7',
  cyan: '#94e2d5',
  white: '#bac2de',
  brightBlack: '#585b70',
  brightRed: '#f38ba8',
  brightGreen: '#a6e3a1',
  brightYellow: '#f9e2af',
  brightBlue: '#89b4fa',
  brightMagenta: '#f5c2e7',
  brightCyan: '#94e2d5',
  brightWhite: '#e6edf3',
} as const

const ANSI_LIGHT = {
  black: '#4c4f69',
  red: '#d20f39',
  green: '#40a02b',
  yellow: '#df8e1d',
  blue: '#1e66f5',
  magenta: '#ea76cb',
  cyan: '#179299',
  white: '#5c5f77',
  brightBlack: '#8c8fa1',
  brightRed: '#d20f39',
  brightGreen: '#40a02b',
  brightYellow: '#df8e1d',
  brightBlue: '#1e66f5',
  brightMagenta: '#ea76cb',
  brightCyan: '#179299',
  brightWhite: '#1e1e2e',
} as const

/** Used when a token cannot be resolved, e.g. during a headless render. */
const FALLBACK_DARK = { background: '#0a0a0a', foreground: '#fafafa' }
const FALLBACK_LIGHT = { background: '#ffffff', foreground: '#0a0a0a' }

export type ColorResolver = (cssExpression: string) => string | null

/**
 * Build the xterm theme.
 *
 * `resolve` turns a CSS expression such as `var(--background)` into a concrete colour;
 * it returns `null` when the value cannot be resolved, in which case a fallback is used.
 */
export function buildTerminalTheme(resolve: ColorResolver, dark: boolean): ITheme {
  const ansi = dark ? ANSI_DARK : ANSI_LIGHT
  const fallback = dark ? FALLBACK_DARK : FALLBACK_LIGHT

  const background = resolve('var(--background)') ?? fallback.background
  const foreground = resolve('var(--foreground)') ?? fallback.foreground
  const accent = resolve('var(--primary)') ?? foreground
  // Not `--accent`: that is the hover surface, one step off the canvas, so a translucent
  // selection drawn from it would be invisible on a near-black terminal.
  const selection = resolve('var(--muted-foreground)') ?? (dark ? '#ffffff40' : '#00000030')

  return {
    background,
    foreground,
    cursor: accent,
    cursorAccent: background,
    // A translucent selection keeps the glyphs underneath readable.
    selectionBackground: withAlpha(selection, dark ? 0.35 : 0.28),
    selectionForeground: undefined,
    ...ansi,
  }
}

/** Add alpha to an `rgb(...)` / `#rrggbb` colour. Unrecognised input is passed through. */
export function withAlpha(color: string, alpha: number): string {
  const rgb = color.match(/^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i)
  if (rgb) {
    return `rgba(${rgb[1]}, ${rgb[2]}, ${rgb[3]}, ${alpha})`
  }

  const hex = color.match(/^#([0-9a-f]{6})$/i)
  if (hex) {
    const value = Number.parseInt(hex[1], 16)
    // eslint-disable-next-line no-bitwise
    return `rgba(${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}, ${alpha})`
  }

  return color
}

/**
 * Resolve a CSS colour expression against the live document.
 *
 * Custom properties come back from `getComputedStyle` unevaluated (`oklch(...)`), so the
 * value is assigned to a probe element and read back from `color`, which the browser has
 * already converted to `rgb(...)`.
 */
export function domColorResolver(host: HTMLElement = document.body): ColorResolver {
  return (expression) => {
    const probe = document.createElement('span')
    probe.style.color = expression
    probe.style.position = 'absolute'
    probe.style.pointerEvents = 'none'
    probe.style.opacity = '0'
    host.appendChild(probe)

    try {
      const resolved = getComputedStyle(probe).color
      // happy-dom and friends hand back the expression unevaluated; treat that as a miss.
      return resolved && resolved.startsWith('rgb') ? resolved : null
    } finally {
      probe.remove()
    }
  }
}

export function isDarkTheme(root: HTMLElement = document.documentElement): boolean {
  return root.classList.contains('dark')
}

/** Read the current theme straight off the document. */
export function readTerminalTheme(): ITheme {
  return buildTerminalTheme(domColorResolver(), isDarkTheme())
}

/** The font stack the terminal should use, taken from the app's mono token. */
export function terminalFontFamily(root: HTMLElement = document.documentElement): string {
  const value = getComputedStyle(root).getPropertyValue('--font-mono').trim()
  return value || 'ui-monospace, SFMono-Regular, Menlo, monospace'
}
