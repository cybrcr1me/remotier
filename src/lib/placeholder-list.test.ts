import { describe, expect, it } from 'vitest'
import { blockingCount, effectiveValue, GLOBAL_SCOPE_ID, groupPlaceholders } from './placeholder-list'
import type { Group, VarDef } from '@/lib/types'

function group(id: string, name: string): Group {
  return {
    id,
    parentId: null,
    name,
    sort: 0,
    defaultPort: null,
    defaultIdentityId: null,
    defaultJumpHostId: null,
    icon: null,
    color: null,
    createdAt: 0,
    updatedAt: 0,
  }
}

function def(scopeId: string, name: string, extra: Partial<VarDef> = {}): VarDef {
  return {
    id: `${scopeId}-${name}`,
    scope: 'group',
    scopeId,
    name,
    label: null,
    defaultValue: null,
    required: false,
    createdAt: 0,
    updatedAt: 0,
    ...extra,
  }
}

const groups = [group('g1', 'Warpgate'), group('g2', 'Alpha')]

describe('groupPlaceholders', () => {
  it('buckets declarations under the group that made them, by name', () => {
    const result = groupPlaceholders(
      [def('g1', 'wg_user'), def('g2', 'realm')],
      groups,
      () => undefined,
    )

    expect(result.map(g => g.name)).toEqual(['Alpha', 'Warpgate'])
    expect(result[1].rows.map(r => r.def.name)).toEqual(['wg_user'])
  })

  it('sorts declarations within a group', () => {
    const result = groupPlaceholders(
      [def('g1', 'zone'), def('g1', 'account')],
      groups,
      () => undefined,
    )
    expect(result[0].rows.map(r => r.def.name)).toEqual(['account', 'zone'])
  })

  it('pairs each declaration with this machine\'s answer', () => {
    const result = groupPlaceholders(
      [def('g1', 'wg_user')],
      groups,
      (scope, scopeId, name) =>
        scope === 'group' && scopeId === 'g1' && name === 'wg_user' ? 'lucas' : undefined,
    )
    expect(result[0].rows[0].value).toBe('lucas')
  })

  it('flags only a required declaration with no answer and no default', () => {
    const rows = groupPlaceholders(
      [
        def('g1', 'blocked', { required: true }),
        def('g1', 'has_default', { required: true, defaultValue: 'x' }),
        def('g1', 'optional'),
      ],
      groups,
      () => undefined,
    ).flatMap(g => g.rows)

    expect(rows.filter(r => r.blocking).map(r => r.def.name)).toEqual(['blocked'])
  })

  it('does not flag a required declaration once it has been answered', () => {
    const result = groupPlaceholders(
      [def('g1', 'wg_user', { required: true })],
      groups,
      () => 'lucas',
    )
    expect(result[0].rows[0].blocking).toBe(false)
  })

  it('keeps declarations whose group is gone rather than hiding them', () => {
    // The value is still there and still occupies a name; hiding it would leave something
    // the user can neither find nor clear.
    const result = groupPlaceholders([def('deleted', 'orphan')], groups, () => undefined)
    expect(result[0].name).toBe('Unknown group')
    expect(result[0].group).toBeNull()
  })

  it('ignores host-scoped declarations, which belong to a single host', () => {
    const result = groupPlaceholders(
      [def('h1', 'per_host', { scope: 'host' })],
      groups,
      () => undefined,
    )
    expect(result).toEqual([])
  })
})

describe('blockingCount', () => {
  it('counts across every group', () => {
    const result = groupPlaceholders(
      [def('g1', 'a', { required: true }), def('g2', 'b', { required: true })],
      groups,
      () => undefined,
    )
    expect(blockingCount(result)).toBe(2)
  })

  it('is zero when everything is answered', () => {
    expect(blockingCount([])).toBe(0)
  })
})

describe('effectiveValue', () => {
  const [bucket] = groupPlaceholders([def('g1', 'wg_user', { defaultValue: 'fallback' })], groups, () => 'mine')

  it('prefers this machine\'s answer over the default', () => {
    expect(effectiveValue(bucket.rows[0])).toBe('mine')
  })

  it('falls back to the declared default', () => {
    const [only] = groupPlaceholders(
      [def('g1', 'wg_user', { defaultValue: 'fallback' })],
      groups,
      () => undefined,
    )
    expect(effectiveValue(only.rows[0])).toBe('fallback')
  })

  it('is null when there is nothing to resolve to', () => {
    const [only] = groupPlaceholders([def('g1', 'wg_user')], groups, () => undefined)
    expect(effectiveValue(only.rows[0])).toBeNull()
  })
})

describe('the account scope', () => {
  function globalDef(name: string, extra: Partial<VarDef> = {}): VarDef {
    return def(GLOBAL_SCOPE_ID, name, { scope: 'global', ...extra })
  }

  it('leads the list, ahead of every group', () => {
    // It is the widest scope and the only one edited here, so it belongs at the top
    // regardless of how the groups happen to sort.
    const result = groupPlaceholders(
      [def('g2', 'realm'), globalDef('company'), def('g1', 'wg_user')],
      groups,
      () => undefined,
    )
    expect(result.map(g => g.name)).toEqual(['Account', 'Alpha', 'Warpgate'])
    expect(result[0].global).toBe(true)
  })

  it('collects account declarations into one bucket', () => {
    const result = groupPlaceholders(
      [globalDef('company'), globalDef('realm')],
      groups,
      () => undefined,
    )
    expect(result).toHaveLength(1)
    expect(result[0].rows.map(r => r.def.name)).toEqual(['company', 'realm'])
  })

  it('is not mistaken for a deleted group', () => {
    // Both have no `Group` behind them; only one is an orphan.
    const result = groupPlaceholders([globalDef('company')], groups, () => undefined)
    expect(result[0].name).toBe('Account')
    expect(result[0].group).toBeNull()
  })

  it('looks an account value up under the global scope', () => {
    // Its `scopeId` is empty, so a lookup keyed on the id alone matches a group's value
    // - or, keyed on `group`, matches nothing and reports every account row unanswered.
    const result = groupPlaceholders(
      [globalDef('company')],
      groups,
      (scope, scopeId, name) =>
        scope === 'global' && scopeId === GLOBAL_SCOPE_ID && name === 'company'
          ? 'acme'
          : undefined,
    )
    expect(result[0].rows[0].value).toBe('acme')
  })

  it('counts an unanswered required account placeholder as blocking', () => {
    const result = groupPlaceholders(
      [globalDef('company', { required: true })],
      groups,
      () => undefined,
    )
    expect(blockingCount(result)).toBe(1)
  })
})
