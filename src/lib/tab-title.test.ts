import { describe, expect, it } from 'vitest'
import { createPane, listPanes, splitPane } from './layout'
import { tabColors, tabHostId, tabIsSplit, tabTitle, titleParts, UNTITLED } from './tab-title'

const LABELS: Record<string, string> = {
  'host-1': 'terminal.shop',
  'host-2': 'web-01',
  'host-3': 'db-primary',
}

const labelOf = (id: string) => LABELS[id] ?? null

describe('titleParts', () => {
  it('lists one host once', () => {
    expect(titleParts(createPane('host-1'), labelOf)).toEqual(['terminal.shop'])
  })

  it('counts repeats instead of repeating them', () => {
    const pane = createPane('host-1')
    const twice = splitPane(pane, pane.id, 'row', createPane('host-1'))
    const thrice = splitPane(twice, pane.id, 'row', createPane('host-1'))

    expect(titleParts(twice, labelOf)).toEqual(['terminal.shop ×2'])
    expect(titleParts(thrice, labelOf)).toEqual(['terminal.shop ×3'])
  })

  it('lists distinct hosts in pane order', () => {
    const first = createPane('host-1')
    const tree = splitPane(first, first.id, 'row', createPane('host-2'))
    expect(titleParts(tree, labelOf)).toEqual(['terminal.shop', 'web-01'])
  })

  it('mixes counted repeats with single hosts', () => {
    const first = createPane('host-1')
    const withSecond = splitPane(first, first.id, 'row', createPane('host-2'))
    const tree = splitPane(withSecond, first.id, 'row', createPane('host-1'))

    expect(titleParts(tree, labelOf)).toEqual(['terminal.shop ×2', 'web-01'])
  })

  it('ignores panes with no host and hosts that no longer exist', () => {
    const first = createPane(null)
    const withGhost = splitPane(first, first.id, 'row', createPane('deleted-host'))
    const tree = splitPane(withGhost, first.id, 'row', createPane('host-2'))

    expect(titleParts(tree, labelOf)).toEqual(['web-01'])
  })

  it('is empty for a tab that has connected to nothing', () => {
    expect(titleParts(createPane(null), labelOf)).toEqual([])
  })
})

describe('tabTitle', () => {
  it('joins the parts', () => {
    const first = createPane('host-1')
    const tree = splitPane(first, first.id, 'row', createPane('host-3'))
    expect(tabTitle(tree, labelOf)).toBe('terminal.shop · db-primary')
  })

  it('falls back to the stored name while the tab is empty', () => {
    expect(tabTitle(createPane(null), labelOf, 'Scratch')).toBe('Scratch')
    expect(tabTitle(createPane(null), labelOf)).toBe(UNTITLED)
  })

  it('reports both hosts once two tabs have been merged', () => {
    // The case that made this necessary: merging used to leave the tab advertising only
    // whichever of the two happened to be the drop target.
    const target = createPane('host-1')
    const merged = splitPane(target, target.id, 'row', createPane('host-2'))
    expect(tabTitle(merged, labelOf)).toBe('terminal.shop · web-01')
  })
})

describe('tabHostId', () => {
  it('takes the host of the active pane', () => {
    const first = createPane('host-1')
    const tree = splitPane(first, first.id, 'row', createPane('host-2'))
    const second = listPanes(tree)[1]

    expect(tabHostId(tree, first.id)).toBe('host-1')
    expect(tabHostId(tree, second.id)).toBe('host-2')
  })

  it('falls back to the first pane when the active one is gone', () => {
    const pane = createPane('host-1')
    expect(tabHostId(pane, 'stale-pane-id')).toBe('host-1')
  })

  it('has no host for a pane that has connected to nothing', () => {
    const pane = createPane(null)
    expect(tabHostId(pane, pane.id)).toBeNull()
  })
})

describe('tabIsSplit', () => {
  it('is false for one pane and true once the tab splits', () => {
    const pane = createPane('host-1')
    expect(tabIsSplit(pane)).toBe(false)

    // Two panes on the same host still count: the tab is a group either way.
    expect(tabIsSplit(splitPane(pane, pane.id, 'row', createPane('host-1')))).toBe(true)
  })
})

describe('tabColors', () => {
  const COLORS: Record<string, string> = { 'host-1': 'red', 'host-2': 'blue' }
  const colorOf = (id: string) => COLORS[id] ?? null

  it('is one colour for a tab on one host', () => {
    expect(tabColors(createPane('host-1'), colorOf)).toEqual(['red'])
  })

  it('lists each colour once, in pane order', () => {
    const first = createPane('host-2')
    const withRed = splitPane(first, first.id, 'row', createPane('host-1'))
    const tree = splitPane(withRed, first.id, 'row', createPane('host-2'))

    expect(tabColors(tree, colorOf)).toEqual(['blue', 'red'])
  })

  it('keeps a host with no colour as a stop, so the tint cannot claim the whole tab', () => {
    const first = createPane('host-1')
    const tree = splitPane(first, first.id, 'row', createPane('host-3'))
    expect(tabColors(tree, colorOf)).toEqual(['red', null])
  })

  it('is empty when no host in the tab has a colour', () => {
    const first = createPane('host-3')
    const tree = splitPane(first, first.id, 'row', createPane(null))
    expect(tabColors(tree, colorOf)).toEqual([])
  })

  it('ignores panes with no host', () => {
    const first = createPane(null)
    const tree = splitPane(first, first.id, 'row', createPane('host-2'))
    expect(tabColors(tree, colorOf)).toEqual(['blue'])
  })
})
