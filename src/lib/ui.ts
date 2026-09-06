/**
 * Layout constants shared between the chrome components.
 *
 * The window header, the terminal tab bar and every view toolbar have to be exactly the
 * same height or the seam between the sidebar and the content is visibly crooked. Keeping
 * the class here means they cannot drift apart; `ui-conventions.test.ts` checks that each
 * bar actually uses it.
 */

/** Height of every horizontal bar in the app chrome. */
export const BAR_HEIGHT = 'h-11'
