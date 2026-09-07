/**
 * Builds the hosts sidebar tree from the flat group and host tables.
 *
 * Kept as pure functions so the nesting and search rules can be tested without mounting
 * anything.
 */

import type { Group, Host } from '@/lib/types'

export interface GroupNode {
  kind: 'group'
  id: string
  name: string
  group: Group
  children: TreeNode[]
}

export interface HostNode {
  kind: 'host'
  id: string
  host: Host
}

export type TreeNode = GroupNode | HostNode

function byName(a: { name: string }, b: { name: string }) {
  return a.name.localeCompare(b.name)
}

function bySort(a: Host, b: Host) {
  return a.sort - b.sort || a.label.localeCompare(b.label)
}

/**
 * Assemble the tree. Groups nest by `parentId`; hosts hang off their group, and hosts
 * with no group are returned after the groups so they are still reachable.
 *
 * A group whose parent is missing is treated as a root rather than being dropped - losing
 * hosts because of a dangling reference would be worse than showing it in the wrong place.
 */
export function buildTree(groups: Group[], hosts: Host[]): TreeNode[] {
  const byParent = new Map<string | null, Group[]>()
  const ids = new Set(groups.map(group => group.id))

  for (const group of groups) {
    const parent = group.parentId && ids.has(group.parentId) ? group.parentId : null
    const siblings = byParent.get(parent) ?? []
    siblings.push(group)
    byParent.set(parent, siblings)
  }

  const hostsByGroup = new Map<string | null, Host[]>()
  for (const host of hosts) {
    const key = host.groupId && ids.has(host.groupId) ? host.groupId : null
    const siblings = hostsByGroup.get(key) ?? []
    siblings.push(host)
    hostsByGroup.set(key, siblings)
  }

  function childrenOf(parentId: string | null, seen: Set<string>): TreeNode[] {
    const groupNodes = [...(byParent.get(parentId) ?? [])]
      .sort(byName)
      // A cycle from a bad edit would otherwise recurse forever.
      .filter(group => !seen.has(group.id))
      .map<TreeNode>((group) => {
        seen.add(group.id)
        return {
          kind: 'group',
          id: group.id,
          name: group.name,
          group,
          children: childrenOf(group.id, seen),
        }
      })

    const hostNodes = [...(hostsByGroup.get(parentId) ?? [])]
      .sort(bySort)
      .map<TreeNode>(host => ({ kind: 'host', id: host.id, host }))

    return [...groupNodes, ...hostNodes]
  }

  return childrenOf(null, new Set())
}

function hostMatches(host: Host, query: string): boolean {
  const haystack = [host.label, host.hostname, ...host.tags].join(' ').toLowerCase()
  return haystack.includes(query)
}

/**
 * Filter the tree by a search string.
 *
 * A matching host is kept along with its ancestors, so the result is still a valid tree.
 * A matching group keeps all of its contents, which is what makes searching for a group
 * name a useful way to see everything inside it.
 */
export function filterTree(nodes: TreeNode[], search: string): TreeNode[] {
  const query = search.trim().toLowerCase()
  if (!query) return nodes

  return nodes.flatMap<TreeNode>((node) => {
    if (node.kind === 'host') {
      return hostMatches(node.host, query) ? [node] : []
    }

    if (node.name.toLowerCase().includes(query)) {
      return [node]
    }

    const children = filterTree(node.children, query)
    return children.length > 0 ? [{ ...node, children }] : []
  })
}

/** Every host in the tree, in display order. */
export function treeHosts(nodes: TreeNode[]): Host[] {
  return nodes.flatMap(node => (node.kind === 'host' ? [node.host] : treeHosts(node.children)))
}

/** Ids of every group in the tree, for expand-all. */
export function groupIds(nodes: TreeNode[]): string[] {
  return nodes.flatMap(node => (node.kind === 'group' ? [node.id, ...groupIds(node.children)] : []))
}

/** A host together with the group names above it, for the grid view. */
export interface FlatHost {
  host: Host
  /** Outermost group first. Empty for an ungrouped host. */
  groupPath: string[]
}

/**
 * Flatten the tree to a list of hosts, keeping the group each one sits under.
 *
 * The grid has no nesting to show structure with, so the path is what tells the user
 * where a host actually lives.
 */
export function flattenHosts(nodes: TreeNode[], groupPath: string[] = []): FlatHost[] {
  return nodes.flatMap<FlatHost>((node) => {
    if (node.kind === 'host') {
      return [{ host: node.host, groupPath }]
    }
    return flattenHosts(node.children, [...groupPath, node.name])
  })
}

/**
 * The children at a path of group ids, walking down from the roots.
 *
 * Returns `null` when the path no longer resolves - a group deleted or moved while the
 * user was inside it. The caller is expected to fall back to the root rather than show an
 * empty folder that cannot be explained.
 */
export function nodesAt(nodes: TreeNode[], path: string[]): TreeNode[] | null {
  let level = nodes

  for (const id of path) {
    const next = level.find((node): node is GroupNode => node.kind === 'group' && node.id === id)
    if (!next) return null
    level = next.children
  }

  return level
}

/**
 * The groups named along a path, outermost first.
 *
 * Stops at the first id that does not resolve, so a stale path still produces a usable
 * trail as far as it goes.
 */
export function breadcrumb(nodes: TreeNode[], path: string[]): GroupNode[] {
  const trail: GroupNode[] = []
  let level = nodes

  for (const id of path) {
    const next = level.find((node): node is GroupNode => node.kind === 'group' && node.id === id)
    if (!next) break
    trail.push(next)
    level = next.children
  }

  return trail
}

/** What a group card reports about its contents. */
export interface GroupSummary {
  /** Every host inside, at any depth: the number that says how much is in there. */
  hosts: number
  /** Immediate subgroups only, which is what the user would see on opening it. */
  groups: number
}

export function summarise(node: GroupNode): GroupSummary {
  return {
    hosts: treeHosts(node.children).length,
    groups: node.children.filter(child => child.kind === 'group').length,
  }
}

/** The group nodes among `nodes`, in order. */
export function groupNodes(nodes: TreeNode[]): GroupNode[] {
  return nodes.filter((node): node is GroupNode => node.kind === 'group')
}

/** The hosts directly among `nodes`, without descending into groups. */
export function directHosts(nodes: TreeNode[]): FlatHost[] {
  return nodes
    .filter((node): node is HostNode => node.kind === 'host')
    .map(node => ({ host: node.host, groupPath: [] }))
}

/** A group as an option in a picker, with the nesting flattened onto it. */
export interface GroupOption {
  group: Group
  /** 0 for a root group; one more for each level down. */
  depth: number
  /** Ancestor names, outermost first. Empty for a root group. */
  path: string[]
}

/**
 * Every group, depth first, carrying how deep it sits and what is above it.
 *
 * A picker cannot nest the way a tree does, so the depth drives indentation and the path
 * both labels the option and gives search something to match: typing a parent's name
 * should find the children under it.
 */
export function flattenGroups(nodes: TreeNode[], depth = 0, path: string[] = []): GroupOption[] {
  return groupNodes(nodes).flatMap<GroupOption>(node => [
    { group: node.group, depth, path },
    ...flattenGroups(node.children, depth + 1, [...path, node.name]),
  ])
}

/**
 * A group and everything under it.
 *
 * Used to keep a group from being reparented into its own subtree. `buildTree` already
 * refuses to follow a cycle, but it does so by dropping the group - so the data would be
 * corrupt and the group would simply vanish from the list.
 */
export function descendantIds(nodes: TreeNode[], groupId: string): string[] {
  for (const node of groupNodes(nodes)) {
    if (node.id === groupId) return [node.id, ...groupIds(node.children)]

    const found = descendantIds(node.children, groupId)
    if (found.length > 0) return found
  }
  return []
}
