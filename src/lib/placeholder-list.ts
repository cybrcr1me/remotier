/**
 * Every placeholder this machine has been asked to fill, and whether it has been.
 *
 * A declaration (`VarDef`) is shared and belongs to the group that made it; the answer
 * (`VarValue`) is local and never leaves this machine. Those two halves live in separate
 * tables and separate scopes, which is right for the data and useless for a user trying to
 * find out what still needs filling in - so this pairs them up.
 */

import type { Group, VarDef, VarScope } from '@/lib/types'

export interface PlaceholderRow {
  def: VarDef
  /** This machine's answer, or `undefined` when it has never given one. */
  value: string | undefined
  /**
   * True when a connection through this group would be blocked: the declaration is
   * required, nothing has been filled in, and there is no default to fall back on.
   */
  blocking: boolean
}

export interface PlaceholderGroup {
  /** The group that declares them; `null` for the account scope and for a missing group. */
  group: Group | null
  /** Shown when the group has been deleted but its declarations linger. */
  name: string
  /** True for the account-wide bucket, which is editable here rather than in a group. */
  global: boolean
  rows: PlaceholderRow[]
}

/** The account scope. One account, so there is nothing to point at. */
export const GLOBAL_SCOPE_ID = ''

/**
 * This machine's answer to a declaration.
 *
 * The scope is part of the key, not just the id: an account declaration has an empty
 * `scopeId`, so looking one up under `group` finds nothing and silently reports every
 * account placeholder as unanswered.
 */
export type ValueLookup = (
  scope: VarScope,
  scopeId: string,
  name: string,
) => string | undefined

/**
 * Group declarations by the group that made them, in group name order.
 *
 * Declarations whose group no longer exists are kept, under an "Unknown group" heading:
 * they still occupy a name, and silently hiding them would leave a value the user cannot
 * find and cannot clear.
 */
export function groupPlaceholders(
  defs: VarDef[],
  groups: Group[],
  valueFor: ValueLookup,
): PlaceholderGroup[] {
  const byId = new Map(groups.map(group => [group.id, group]))
  const buckets = new Map<string, PlaceholderGroup>()

  for (const def of defs) {
    if (def.scope !== 'group' && def.scope !== 'global') continue

    const global = def.scope === 'global'
    const group = global ? null : byId.get(def.scopeId) ?? null
    const key = global ? 'global' : def.scopeId

    const bucket = buckets.get(key) ?? {
      group,
      name: global ? 'Account' : group?.name ?? 'Unknown group',
      global,
      rows: [],
    }

    const value = valueFor(def.scope, def.scopeId, def.name)
    bucket.rows.push({
      def,
      value,
      blocking: def.required && !value && !def.defaultValue,
    })

    buckets.set(key, bucket)
  }

  for (const bucket of buckets.values()) {
    bucket.rows.sort((a, b) => a.def.name.localeCompare(b.def.name))
  }

  // The account bucket leads: it is the widest scope, and the one edited here.
  return [...buckets.values()].sort((a, b) => {
    if (a.global !== b.global) return a.global ? -1 : 1
    return a.name.localeCompare(b.name)
  })
}

/** How many declarations would block a connection as things stand. */
export function blockingCount(groups: PlaceholderGroup[]): number {
  return groups.reduce(
    (total, group) => total + group.rows.filter(row => row.blocking).length,
    0,
  )
}

/**
 * What a row resolves to right now, for the preview column.
 *
 * Mirrors the first two steps of `vars.rs`: the local value wins, the declaration's default
 * stands in for it, and neither means unresolved. Built-ins are not covered - they are not
 * declared, so they never appear in this list.
 */
export function effectiveValue(row: PlaceholderRow): string | null {
  return row.value ?? row.def.defaultValue ?? null
}
