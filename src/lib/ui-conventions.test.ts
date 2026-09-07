/**
 * Conventions that are easy to break and annoying to notice by eye.
 *
 * These read the .vue sources rather than mounting anything: the point is to catch a
 * missing attribute across every form at once.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

const SRC = join(import.meta.dirname, '..')

function vueFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry)
    if (statSync(path).isDirectory()) {
      // Generated shadcn-vue components are not ours to hold to these rules.
      return entry === 'ui' ? [] : vueFiles(path)
    }
    return path.endsWith('.vue') ? [path] : []
  })
}

function tagsOf(source: string, tag: string): string[] {
  return source.match(new RegExp(`<${tag}\\b[^>]*?/?>`, 'gs')) ?? []
}

describe('form inputs', () => {
  const files = vueFiles(SRC)

  it('finds the components to check', () => {
    expect(files.length).toBeGreaterThan(5)
  })

  it('every text field has a placeholder', () => {
    const offenders: string[] = []

    for (const file of files) {
      const source = readFileSync(file, 'utf8')
      for (const tag of [...tagsOf(source, 'Input'), ...tagsOf(source, 'Textarea')]) {
        if (!tag.includes('placeholder')) {
          const id = tag.match(/id="([^"]+)"/)?.[1] ?? tag.slice(0, 40)
          offenders.push(`${file.replace(SRC, 'src')}: ${id}`)
        }
      }
    }

    // An empty field with no placeholder gives the user nothing to go on.
    expect(offenders).toEqual([])
  })

  it('every icon-only button has an accessible label', () => {
    const offenders: string[] = []

    for (const file of files) {
      const source = readFileSync(file, 'utf8')
      for (const tag of tagsOf(source, 'Button')) {
        const iconOnly = /size="icon"/.test(tag)
        const labelled = /aria-label|:aria-label/.test(tag)
        if (iconOnly && !labelled) {
          offenders.push(`${file.replace(SRC, 'src')}: ${tag.slice(0, 60)}`)
        }
      }
    }

    expect(offenders).toEqual([])
  })
})

describe('chrome bars', () => {
  const BARS = [
    'components/layout/ViewToolbar.vue',
    'components/layout/AppHeader.vue',
    'components/layout/AppSidebar.vue',
    'components/terminal/TabBar.vue',
  ]

  it('every horizontal bar takes its height from the shared constant', () => {
    const offenders = BARS.filter((file) => {
      const source = readFileSync(join(SRC, file), 'utf8')
      return !source.includes('BAR_HEIGHT')
    })

    // Mismatched heights make the seam between the sidebar and the content visibly
    // crooked, and it is very easy to change one and forget the others.
    expect(offenders).toEqual([])
  })

  it('no bar hardcodes a height alongside the shared constant', () => {
    const offenders: string[] = []

    for (const file of BARS) {
      const source = readFileSync(join(SRC, file), 'utf8')
      // A literal h-<n> in a class list would silently win over the shared value.
      const literals = source.match(/class="[^"]*\bh-\d+\b[^"]*"/g) ?? []
      if (literals.length > 0) offenders.push(`${file}: ${literals[0]}`)
    }

    expect(offenders).toEqual([])
  })
})

describe('terminal stacking', () => {
  it('the terminal host creates its own stacking context', () => {
    const source = readFileSync(join(SRC, 'components/terminal/TerminalPane.vue'), 'utf8')

    // `.xterm` is position:relative with z-index:auto, so it creates no stacking context
    // of its own and its internal layers (up to z-index 10, carrying cursor:text) would
    // otherwise paint over sibling overlays and swallow their clicks.
    expect(source).toMatch(/ref="host"[^>]*class="[^"]*\bisolate\b/)
  })

  it('the connection panel sits above the terminal', () => {
    const source = readFileSync(join(SRC, 'components/terminal/ConnectionPanel.vue'), 'utf8')
    expect(source).toMatch(/class="[^"]*\babsolute\b[^"]*\bz-\d+\b/)
  })
})

describe('stylesheet', () => {
  const css = readFileSync(join(SRC, 'assets/index.css'), 'utf8')

  it('pulls no stylesheet or font over the network', () => {
    // `shadcn-vue add` re-injects a Google Fonts `@import url(...)` into this file every
    // time it runs. The app's CSP is `default-src 'self'`, so the request is refused and
    // the face silently falls back - the faces are installed from npm instead.
    expect(css.match(/@import\s+url\(/g) ?? []).toEqual([])
  })

  it('defines the heading hook the nova preset leaves empty', () => {
    // Seven generated titles carry `cn-font-heading`; nothing in the registry defines it.
    expect(css).toMatch(/@utility\s+cn-font-heading\s*\{/)
  })
})

describe('pane lifetime', () => {
  const pane = readFileSync(join(SRC, 'components/terminal/TerminalPane.vue'), 'utf8')

  it('does not dispose the terminal when the component unmounts', () => {
    // The component unmounts both when a pane is closed and when it is dragged to another
    // split or tab, and cannot tell those apart. Disposing here would drop the IPC channel
    // and end the SSH session behind a pane that the user only moved.
    const teardown = pane.match(/onBeforeUnmount\(\(\) => \{[\s\S]*?\n\}\)/)?.[0] ?? ''
    expect(teardown).not.toMatch(/dispose\(/)
  })

  it('leaves disposal to the view that knows which panes are gone', () => {
    const view = readFileSync(join(SRC, 'views/TerminalsView.vue'), 'utf8')
    expect(view).toMatch(/releaseMissing\(/)
  })
})

describe('tab dragging', () => {
  it('does not make a tab active on mousedown', () => {
    // Focusing on mousedown puts the tab's own panes on screen before its drag begins,
    // so every pane the pointer then crosses belongs to the tab being dragged - and a tab
    // cannot be dropped into itself. Focus on click, which a drag never produces.
    const source = readFileSync(join(SRC, 'components/terminal/TabBar.vue'), 'utf8')
    expect(source).not.toMatch(/@mousedown/)
    expect(source).toMatch(/@click="sessions\.focusTab/)
  })
})

describe('hosts breadcrumb', () => {
  const view = readFileSync(join(SRC, 'views/HostsView.vue'), 'utf8')

  it('shows the trail whenever the grid is on, not only inside a group', () => {
    // A bar that appears only once you are inside a group shifts everything under it by
    // its own height on the way in, and takes the way out with it on the way back.
    expect(view).toMatch(/<div v-if="layout === 'grid'" class="[^"]*"\s*>/)
    expect(view).not.toMatch(/v-if="layout === 'grid' && inFolder"/)
  })

  it('offers a way back to the root from inside a group', () => {
    expect(view).toMatch(/goUpTo\(-1\)/)
  })
})
