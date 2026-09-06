import { describe, expect, it } from 'vitest'
import { INHERIT, resolveInherited, toFormValue } from './inherit'

describe('inherit sentinel', () => {
  it('maps the sentinel back to null', () => {
    expect(resolveInherited(INHERIT)).toBeNull()
  })

  it('passes a real id through untouched', () => {
    expect(resolveInherited('group-1')).toBe('group-1')
  })

  it('maps null and undefined to the sentinel', () => {
    expect(toFormValue(null)).toBe(INHERIT)
    expect(toFormValue(undefined)).toBe(INHERIT)
  })

  it('round trips a real id', () => {
    expect(resolveInherited(toFormValue('group-1'))).toBe('group-1')
  })

  it('round trips inherit', () => {
    expect(resolveInherited(toFormValue(null))).toBeNull()
  })
})
