/**
 * Select components cannot bind an empty string or null to an option, so "inherit" needs
 * a sentinel value in forms. Keeping it in one place avoids each form inventing its own.
 */

export const INHERIT = '__inherit__'

/** Turn a form value back into what the backend expects, where null means inherit. */
export function resolveInherited(value: string): string | null {
  return value === INHERIT ? null : value
}

/** Turn a stored value into something a Select can bind to. */
export function toFormValue(value: string | null | undefined): string {
  return value ?? INHERIT
}
