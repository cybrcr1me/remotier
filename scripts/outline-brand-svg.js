/**
 * Convert the brand SVGs in `design/assets/` into self-contained artwork.
 *
 * Those files set `>_` and REMOTIER as live `<text>` in Martian Mono, which the brand
 * guide calls layout reference only: nothing that renders them - resvg behind
 * `tauri icon`, a browser on a machine without the font, an installer - is guaranteed to
 * have the face, and every one of them silently falls back to some other monospace. The
 * guide's answer is to outline the type before release, which is what this does.
 *
 * Martian Mono ships as a variable woff2 in node_modules. fontkit cannot instance a
 * variable axis while the tables are still woff2-compressed - `getVariation` hands back
 * an object whose `cmap` is gone - so the font is expanded to TTF in memory first.
 *
 *   bun run scripts/outline-brand-svg.js
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import * as fontkit from 'fontkit'
import * as woff2 from 'wawoff2'

const ROOT = join(dirname(new URL(import.meta.url).pathname), '..')
const WOFF2 = join(
  ROOT,
  'node_modules/@fontsource-variable/martian-mono/files/martian-mono-latin-wght-normal.woff2',
)

/** The brand sets the mark and the wordmark in Martian Mono 700 and nothing else. */
const WEIGHT = 700

/** `design/assets` source -> destination. */
const TARGETS = [
  { from: 'design/assets/app-icon.svg', to: 'design/icon.svg' },
  { from: 'design/assets/favicon.svg', to: 'public/favicon.svg' },
  { from: 'design/assets/mark.svg', to: 'design/mark.svg' },
]

/*
 * `wordmark.svg` and `logo-lockup.svg` are deliberately not built. Both position the lime
 * block cursor with a hard-coded `x` - 694 on a 760-wide canvas for eight characters at
 * 64px - which implies an advance of about 1355 per 1000 units. Martian Mono advances 750
 * at its widest, so outlining the type leaves the cursor floating far to its right: those
 * files were laid out against a fallback face, exactly what the brand guide warns about.
 * Nothing consumes them anyway; the wordmark in the sidebar is live text with the real
 * font loaded. Fix the source layout first if an SVG wordmark is ever needed.
 */

async function loadFont() {
  const ttf = await woff2.decompress(readFileSync(WOFF2))
  return fontkit.create(Buffer.from(ttf)).getVariation({ wght: WEIGHT })
}

function attr(tag, name) {
  return tag.match(new RegExp(`${name}="([^"]*)"`))?.[1]
}

function num(tag, name, fallback = 0) {
  const raw = attr(tag, name)
  return raw === undefined ? fallback : Number.parseFloat(raw)
}

/** Trim to 3 decimals without leaving `1.000`. */
function round(value) {
  return Number.parseFloat(value.toFixed(3)).toString()
}

/**
 * Lay `text` out and return its glyph paths plus the bounding box of the ink, both in
 * font units, with the pen starting at the origin.
 */
function runOf(font, text, letterSpacingUnits) {
  const glyphs = []
  const box = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity }
  let pen = 0

  for (const char of text) {
    const glyph = font.glyphsForString(char)[0]
    const { bbox } = glyph

    glyphs.push({ path: glyph.path.toSVG(), x: pen })
    box.minX = Math.min(box.minX, pen + bbox.minX)
    box.maxX = Math.max(box.maxX, pen + bbox.maxX)
    box.minY = Math.min(box.minY, bbox.minY)
    box.maxY = Math.max(box.maxY, bbox.maxY)

    pen += glyph.advanceWidth + letterSpacingUnits
  }

  // The trailing letter-spacing is between characters only, never after the last one.
  return { glyphs, box, advance: pen - letterSpacingUnits }
}

/**
 * Replace one `<text>` element with an equivalent `<g>` of outlined glyphs.
 *
 * `dominant-baseline="central"` is honoured by centring the ink rather than the em box:
 * these are logos, where what has to sit in the middle of the tile is the mark you can
 * see, not the metrics box around it.
 */
function outlineText(font, tag, text) {
  const size = num(tag, 'font-size', 16)
  const fill = attr(tag, 'fill') ?? '#000000'
  const anchor = attr(tag, 'text-anchor') ?? 'start'
  const x = num(tag, 'x')
  const y = num(tag, 'y')

  const scale = size / font.unitsPerEm
  const { glyphs, box, advance } = runOf(font, text, num(tag, 'letter-spacing') / scale)

  let originX = x
  if (anchor === 'middle') originX = x - (advance * scale) / 2
  else if (anchor === 'end') originX = x - advance * scale

  // Font space is y-up, SVG is y-down, hence the negative vertical scale.
  const baseline = attr(tag, 'dominant-baseline') === 'central'
    ? y + ((box.minY + box.maxY) / 2) * scale
    : y

  const body = glyphs
    .map(g => `    <path d="${g.path}" transform="translate(${round(g.x)} 0)"/>`)
    .join('\n')

  return `<g fill="${fill}" transform="translate(${round(originX)} ${round(baseline)}) scale(${round(scale)} ${round(-scale)})">\n${body}\n  </g>`
}

/** Strip the C2PA manifest and the namespace declaration that only it needs. */
function clean(source) {
  return source
    .replace(/<metadata>[\s\S]*?<\/metadata>/g, '')
    .replace(/ xmlns:c2pa="[^"]*"/g, '')
}

const font = await loadFont()
const banner = '<!-- Generated by scripts/outline-brand-svg.js - edit design/assets/, not this file. -->'

for (const { from, to } of TARGETS) {
  const source = clean(readFileSync(join(ROOT, from), 'utf8'))
  let count = 0

  const outlined = source.replace(/<text\b([^>]*)>([\s\S]*?)<\/text>/g, (_, attrs, body) => {
    count += 1
    const text = body.replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&amp;/g, '&').trim()
    return outlineText(font, `<text${attrs}>`, text)
  })

  const result = outlined.replace(/^(<svg[^>]*>)/, `$1\n  ${banner}`)
  writeFileSync(join(ROOT, to), `${result.trim()}\n`)
  console.log(`${to} <- ${from} (${count} text element${count === 1 ? '' : 's'} outlined)`)
}
