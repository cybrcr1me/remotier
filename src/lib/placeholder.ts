/**
 * Literal `{{name}}` examples for help text.
 *
 * They cannot be written inline in a template: Vue's tokenizer closes the interpolation
 * on the inner `}}`, so the braces have to come from script.
 */
export function placeholderExample(name: string): string {
  return `{{${name}}}`
}

export const WG_USER_EXAMPLE = placeholderExample('wg_user')
export const NAME_EXAMPLE = placeholderExample('name')
