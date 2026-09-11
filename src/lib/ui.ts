/**
 * Layout constants shared between the chrome components.
 *
 * The chrome has two rows of bars. The window header and the sidebar header form the top
 * row and have to be exactly the same height, or the seam between the sidebar and the
 * content is visibly crooked. Beneath the header, the terminal tab bar and every view
 * toolbar take the same place in their views, so they share a height of their own and the
 * content does not jump when switching views. Keeping the classes here means neither row
 * can drift apart; `ui-conventions.test.ts` checks that each bar actually uses its constant.
 */

/** Height of the top row: the window header and the sidebar header. */
export const BAR_HEIGHT = 'h-11'

/**
 * Height of the row beneath it: the terminal tab bar and the view toolbars. Shorter than
 * the header, which has to leave room for the macOS window buttons, while these hold only a
 * line of text or a row of small controls.
 */
export const TOOLBAR_HEIGHT = 'h-10'
