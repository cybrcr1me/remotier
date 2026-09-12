/**
 * Layout constants shared between the chrome components.
 *
 * The window header and the sidebar header form the top row and have to be exactly the same
 * height, or the seam between the sidebar and the content is visibly crooked. The view
 * toolbars beneath the header share a height of their own. Keeping the classes here means
 * neither can drift; `ui-conventions.test.ts` checks that each bar actually uses its constant.
 *
 * The terminal tabs are not a bar at all: they live in the window header, which `AppHeader`
 * renders directly.
 */

/** Height of the top row: the window header and the sidebar header. */
export const BAR_HEIGHT = 'h-11'

/**
 * Height of the view toolbars beneath the header. Shorter than the header, which has to
 * leave room for the macOS window buttons, while these hold only a row of small controls.
 */
export const TOOLBAR_HEIGHT = 'h-10'
