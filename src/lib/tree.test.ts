import { describe, expect, it } from 'vitest'
import { buildTree, filterTree, flattenHosts, groupIds, treeHosts, type GroupNode } from './tree'
import type { Group, Host } from '@/lib/types'

function group(id: string, name: string, parentId: string | null = null): Group {
  return {
    id,
    parentId,
    name,
    sort: 0,
    defaultPort: null,
    defaultIdentityId: null,
    defaultJumpHostId: null,
    createdAt: 0,
    updatedAt: 0,
  }
}

function host(id: string, label: string, groupId: string | null = null, extra: Partial<Host> = {}): Host {
  return {
    id,
    groupId,
    label,
    hostname: `${label}.example.com`,
    port: null,
    identityId: null,
    jumpHostId: null,
    color: null,
    tags: [],
    sort: 0,
    createdAt: 0,
    updatedAt: 0,
    ...extra,
  }
}

function asGroup(node: unknown): GroupNode {
  const candidate = node as GroupNode
  if (candidate?.kind !== 'group') throw new Error('expected a group node')
  return candidate
}

describe('buildTree', () => {
  it('nests groups by parent', () => {
    const tree = buildTree([group('a', 'A'), group('b', 'B', 'a')], [])

    expect(tree).toHaveLength(1)
    expect(asGroup(tree[0]).children.map(n => n.id)).toEqual(['b'])
  })

  it('puts hosts under their group', () => {
    const tree = buildTree([group('a', 'A')], [host('h1', 'web', 'a')])

    expect(asGroup(tree[0]).children.map(n => n.id)).toEqual(['h1'])
  })

  it('keeps ungrouped hosts at the top level', () => {
    const tree = buildTree([group('a', 'A')], [host('h1', 'loose')])

    expect(tree.map(n => n.id)).toEqual(['a', 'h1'])
  })

  it('lists groups before hosts at the same level', () => {
    const tree = buildTree([group('a', 'Zed')], [host('h1', 'alpha')])

    expect(tree.map(n => n.kind)).toEqual(['group', 'host'])
  })

  it('sorts groups by name and hosts by sort order', () => {
    const tree = buildTree(
      [group('b', 'Beta'), group('a', 'Alpha')],
      [host('h2', 'second', null, { sort: 2 }), host('h1', 'first', null, { sort: 1 })],
    )

    expect(tree.map(n => n.id)).toEqual(['a', 'b', 'h1', 'h2'])
  })

  it('treats a group with a dangling parent as a root', () => {
    // Losing the group, and every host inside it, would be worse than misplacing it.
    const tree = buildTree([group('a', 'Orphan', 'missing')], [host('h1', 'web', 'a')])

    expect(tree.map(n => n.id)).toEqual(['a'])
    expect(treeHosts(tree)).toHaveLength(1)
  })

  it('shows a host whose group no longer exists as ungrouped', () => {
    const tree = buildTree([], [host('h1', 'web', 'gone')])

    expect(tree.map(n => n.id)).toEqual(['h1'])
  })

  it('does not recurse forever on a cyclic parent chain', () => {
    const tree = buildTree([group('a', 'A', 'b'), group('b', 'B', 'a')], [])

    // Whatever it renders, it must terminate and not duplicate a group.
    expect(groupIds(tree).length).toBeLessThanOrEqual(2)
  })

  it('handles an empty inventory', () => {
    expect(buildTree([], [])).toEqual([])
  })
})

describe('filterTree', () => {
  const groups = [group('a', 'Production'), group('b', 'Databases', 'a')]
  const hosts = [
    host('h1', 'web-01', 'a', { tags: ['eu'] }),
    host('h2', 'db-01', 'b'),
    host('h3', 'laptop'),
  ]
  const tree = buildTree(groups, hosts)

  it('returns everything for an empty query', () => {
    expect(filterTree(tree, '   ')).toEqual(tree)
  })

  it('keeps a matching host and its ancestors', () => {
    const filtered = filterTree(tree, 'db-01')

    // The result has to stay a valid tree, so Production and Databases come along.
    expect(filtered.map(n => n.id)).toEqual(['a'])
    const production = asGroup(filtered[0])
    expect(production.children.map(n => n.id)).toEqual(['b'])
    expect(treeHosts(filtered).map(h => h.id)).toEqual(['h2'])
  })

  it('keeps everything inside a matching group', () => {
    const filtered = filterTree(tree, 'production')

    // Display order, not insertion order: the nested Databases group renders above
    // Production's own hosts, so its host comes first.
    expect(treeHosts(filtered).map(h => h.id)).toEqual(['h2', 'h1'])
  })

  it('matches on hostname as well as label', () => {
    expect(treeHosts(filterTree(tree, 'laptop.example.com')).map(h => h.id)).toEqual(['h3'])
  })

  it('matches on tags', () => {
    expect(treeHosts(filterTree(tree, 'eu')).map(h => h.id)).toEqual(['h1'])
  })

  it('is case insensitive', () => {
    expect(treeHosts(filterTree(tree, 'WEB-01')).map(h => h.id)).toEqual(['h1'])
  })

  it('drops groups with no matches', () => {
    expect(filterTree(tree, 'nothing-matches-this')).toEqual([])
  })
})

describe('flattenHosts', () => {
  it('returns hosts with the groups above them', () => {
    const tree = buildTree(
      [group('a', 'Production'), group('b', 'Databases', 'a')],
      [host('h1', 'web-01', 'a'), host('h2', 'db-01', 'b')],
    )

    expect(flattenHosts(tree)).toEqual([
      { host: expect.objectContaining({ id: 'h2' }), groupPath: ['Production', 'Databases'] },
      { host: expect.objectContaining({ id: 'h1' }), groupPath: ['Production'] },
    ])
  })

  it('gives ungrouped hosts an empty path', () => {
    const tree = buildTree([], [host('h1', 'loose')])

    expect(flattenHosts(tree)).toEqual([
      { host: expect.objectContaining({ id: 'h1' }), groupPath: [] },
    ])
  })

  it('includes every host in the tree', () => {
    const tree = buildTree(
      [group('a', 'A'), group('b', 'B', 'a')],
      [host('h1', 'one', 'a'), host('h2', 'two', 'b'), host('h3', 'three')],
    )

    expect(flattenHosts(tree)).toHaveLength(3)
  })

  it('is empty for an empty tree', () => {
    expect(flattenHosts([])).toEqual([])
  })

  it('reflects a filtered tree', () => {
    const tree = buildTree([group('a', 'Production')], [
      host('h1', 'web-01', 'a'),
      host('h2', 'db-01', 'a'),
    ])

    expect(flattenHosts(filterTree(tree, 'web')).map(f => f.host.id)).toEqual(['h1'])
  })
})
