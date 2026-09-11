/**
 * Layout constants shared between the chrome components.
 *
 * The window header and the sidebar header form the top row and have to be exactly the same
 * height, or the seam between the sidebar and the content is visibly crooked. The view
 * toolbars beneath the header share a height of their own. Keeping the classes here means
 * neither can drift; `ui-conventions.test.ts` checks that each bar actually uses its constant.
 *
 * The terminal tabs are not a bar at all: they live in the window header, in `HEADER_SLOT`.
 */

/** Height of the top row: the window header and the sidebar header. */
export const BAR_HEIGHT = 'h-11'

/**
 * Height of the view toolbars beneath the header. Shorter than the header, which has to
 * leave room for the macOS window buttons, while these hold only a row of small controls.
 */
export const TOOLBAR_HEIGHT = 'h-10'

/**
 * Where a view renders content into the window header, by `<Teleport>`: the terminal tabs.
 * The header row otherwise holds only the sidebar toggle, and a tab bar of its own beneath
 * it cost every terminal a row of height.
 */
export const HEADER_SLOT_ID = 'app-header-slot'
export const HEADER_SLOT = `#${HEADER_SLOT_ID}`
