import { describe, expect, it } from 'vitest'
import { beginDrag, canDropOnPane, dragging, endDrag } from './drag'

const TAB_A = { tabId: 'tab-a', paneId: 'pane-a' }

describe('canDropOnPane', () => {
  it('accepts a tab dropped on a pane belonging to another tab', () => {
    expect(canDropOnPane({ kind: 'tab', tabId: 'tab-b' }, TAB_A)).toBe(true)
  })

  it('refuses a tab dropped on a pane it already owns', () => {
    // A tab's layout cannot be inserted into itself. This is the case that made every
    // drop a no-op while tabs still focused on mousedown: pressing a tab put its own
    // panes on screen, so whatever the pointer then crossed belonged to the dragged tab.
    expect(canDropOnPane({ kind: 'tab', tabId: 'tab-a' }, TAB_A)).toBe(false)
  })

  it('accepts a pane dropped on a different pane, in any tab', () => {
    expect(canDropOnPane({ kind: 'pane', paneId: 'pane-b' }, TAB_A)).toBe(true)
  })

  it('refuses a pane dropped on itself', () => {
    expect(canDropOnPane({ kind: 'pane', paneId: 'pane-a' }, TAB_A)).toBe(false)
  })

  it('refuses everything when nothing is being dragged', () => {
    expect(canDropOnPane(null, TAB_A)).toBe(false)
  })
})

describe('drag state', () => {
  it('reports what is being dragged and forgets it afterwards', () => {
    beginDrag({ kind: 'tab', tabId: 'tab-a' })
    expect(dragging.value).toEqual({ kind: 'tab', tabId: 'tab-a' })

    endDrag()
    expect(dragging.value).toBeNull()
  })
})
