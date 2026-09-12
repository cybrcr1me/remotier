import { describe, expect, it } from 'vitest'
import { dropZone, insertionIndex, reorder, zoneStyle, type Rect } from './dnd'

const PANE: Rect = { left: 0, top: 0, width: 400, height: 200 }

describe('dropZone', () => {
  it('reads the middle as a drop into the pane', () => {
    expect(dropZone(PANE, 200, 100)).toBe('center')
  })

  it('reads each edge band', () => {
    expect(dropZone(PANE, 10, 100)).toBe('left')
    expect(dropZone(PANE, 390, 100)).toBe('right')
    expect(dropZone(PANE, 200, 5)).toBe('top')
    expect(dropZone(PANE, 200, 195)).toBe('bottom')
  })

  it('picks the nearest edge in a corner', () => {
    // Four pixels from the top, ten from the left: the top is nearer in absolute terms,
    // but the pane is twice as wide as it is tall, so proportionally the left wins.
    expect(dropZone(PANE, 10, 4)).toBe('top')
    expect(dropZone(PANE, 4, 20)).toBe('left')
  })

  it('scales the bands to the pane rather than using fixed pixels', () => {
    const wide: Rect = { left: 0, top: 0, width: 1000, height: 200 }
    const narrow: Rect = { left: 0, top: 0, width: 100, height: 200 }

    // The same 60px from the left is inside the wide pane's 250px band and well outside
    // the narrow pane's 25px one.
    expect(dropZone(wide, 60, 100)).toBe('left')
    expect(dropZone(narrow, 60, 100)).toBe('center')
  })

  it('honours the pane offset', () => {
    const offset: Rect = { left: 500, top: 300, width: 400, height: 200 }
    expect(dropZone(offset, 700, 400)).toBe('center')
    expect(dropZone(offset, 510, 400)).toBe('left')
  })

  it('treats a collapsed pane as a plain drop rather than dividing by zero', () => {
    expect(dropZone({ left: 0, top: 0, width: 0, height: 0 }, 0, 0)).toBe('center')
  })
})

describe('zoneStyle', () => {
  it('covers the whole pane for a centre drop', () => {
    expect(zoneStyle('center')).toMatchObject({ width: '100%', height: '100%' })
  })

  it('puts the trailing bands at the far edge', () => {
    expect(zoneStyle('right')).toMatchObject({ left: '75%', width: '25%' })
    expect(zoneStyle('bottom')).toMatchObject({ top: '75%', height: '25%' })
  })
})

describe('insertionIndex', () => {
  const tabs: Rect[] = [
    { left: 0, top: 0, width: 100, height: 30 },
    { left: 100, top: 0, width: 100, height: 30 },
    { left: 200, top: 0, width: 100, height: 30 },
  ]

  it('lands before the first tab when dragged to the far left', () => {
    expect(insertionIndex(tabs, 10)).toBe(0)
  })

  it('lands after the last tab when dragged past the end', () => {
    expect(insertionIndex(tabs, 400)).toBe(3)
  })

  it('hands over at each midpoint', () => {
    expect(insertionIndex(tabs, 49)).toBe(0)
    expect(insertionIndex(tabs, 51)).toBe(1)
    expect(insertionIndex(tabs, 149)).toBe(1)
    expect(insertionIndex(tabs, 151)).toBe(2)
  })

  it('is 0 for an empty bar', () => {
    expect(insertionIndex([], 42)).toBe(0)
  })
})

describe('reorder', () => {
  const items = ['a', 'b', 'c', 'd']

  it('moves an item later, accounting for its own removal', () => {
    // Insertion index 3 is the gap before 'd', so 'a' ends up between 'c' and 'd'.
    expect(reorder(items, 0, 3)).toEqual(['b', 'c', 'a', 'd'])
  })

  it('moves an item earlier', () => {
    expect(reorder(items, 3, 1)).toEqual(['a', 'd', 'b', 'c'])
  })

  it('moves an item to the end', () => {
    expect(reorder(items, 0, 4)).toEqual(['b', 'c', 'd', 'a'])
  })

  it('leaves the list alone when the item does not move', () => {
    expect(reorder(items, 1, 1)).toEqual(items)
    expect(reorder(items, 1, 2)).toEqual(items)
  })

  it('ignores an index that is not in the list', () => {
    expect(reorder(items, 9, 0)).toEqual(items)
    expect(reorder(items, 0, -1)).toEqual(items)
  })

  it('does not mutate the input', () => {
    const original = [...items]
    reorder(items, 0, 3)
    expect(items).toEqual(original)
  })
})
