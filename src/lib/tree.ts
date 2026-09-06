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
