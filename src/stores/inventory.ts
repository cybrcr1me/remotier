/** Groups and hosts: the sidebar tree and everything that hangs off it. */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { errorMessage, ipc } from '@/lib/ipc'
import type { Group, GroupInput, Host, HostInput } from '@/lib/types'

export const useInventoryStore = defineStore('inventory', () => {
  const groups = ref<Group[]>([])
  const hosts = ref<Host[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  const groupById = computed(() => new Map(groups.value.map(g => [g.id, g])))
  const hostById = computed(() => new Map(hosts.value.map(h => [h.id, h])))

  const rootGroups = computed(() => groups.value.filter(g => g.parentId === null))
  const ungroupedHosts = computed(() => hosts.value.filter(h => h.groupId === null))

  function childGroups(parentId: string) {
    return groups.value.filter(g => g.parentId === parentId)
  }

  function hostsInGroup(groupId: string) {
    return hosts.value.filter(h => h.groupId === groupId)
  }

  /** Walks group -> parent -> ... -> root. Used for inherited defaults and variables. */
  function groupChain(groupId: string | null): Group[] {
    const chain: Group[] = []
    const seen = new Set<string>()
    let current = groupId
    while (current && !seen.has(current)) {
      seen.add(current)
      const group = groupById.value.get(current)
      if (!group) break
      chain.push(group)
      current = group.parentId
    }
    return chain
  }

  async function load() {
    loading.value = true
    error.value = null
    try {
      const [loadedGroups, loadedHosts] = await Promise.all([ipc.listGroups(), ipc.listHosts()])
      groups.value = loadedGroups
      hosts.value = loadedHosts
    } catch (e) {
      error.value = errorMessage(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function createGroup(input: GroupInput) {
    const group = await ipc.createGroup(input)
    groups.value.push(group)
    return group
  }

  async function updateGroup(id: string, input: GroupInput) {
    const group = await ipc.updateGroup(id, input)
    const index = groups.value.findIndex(g => g.id === id)
    if (index !== -1) groups.value[index] = group
    return group
  }

  async function deleteGroup(id: string) {
    await ipc.deleteGroup(id)
    // Subgroups cascade and hosts fall back to ungrouped, so reload rather than guess.
    await load()
  }

  async function createHost(input: HostInput) {
    const host = await ipc.createHost(input)
    hosts.value.push(host)
    return host
  }

  async function updateHost(id: string, input: HostInput) {
    const host = await ipc.updateHost(id, input)
    const index = hosts.value.findIndex(h => h.id === id)
    if (index !== -1) hosts.value[index] = host
    return host
  }

  /**
   * Store a password answered at a prompt against the host.
   *
   * Switches the host to its own password credentials, since that is what the user just
   * supplied - leaving it pointing at an identity would discard the password next time.
   */
  async function updateHostPassword(id: string, password: string) {
    const host = hostById.value.get(id)
    if (!host) return

    const updated = await ipc.updateHost(id, {
      label: host.label,
      hostname: host.hostname,
      groupId: host.groupId,
      port: host.port,
      jumpHostId: host.jumpHostId,
      color: host.color,
      tags: host.tags,
      sort: host.sort,
      identityId: null,
      username: host.username ?? null,
      authKind: 'password',
      password,
      keyId: host.keyId,
    })

    const index = hosts.value.findIndex(h => h.id === id)
    if (index !== -1) hosts.value[index] = updated
    return updated
  }

  async function deleteHost(id: string) {
    await ipc.deleteHost(id)
    hosts.value = hosts.value.filter(h => h.id !== id)
  }

  async function moveHosts(ids: string[], groupId: string | null) {
    await ipc.moveHosts(ids, groupId)
    await load()
  }

  return {
    groups,
    hosts,
    loading,
    error,
    groupById,
    hostById,
    rootGroups,
    ungroupedHosts,
    childGroups,
    hostsInGroup,
    groupChain,
    load,
    createGroup,
    updateGroup,
    deleteGroup,
    createHost,
    updateHost,
    updateHostPassword,
    deleteHost,
    moveHosts,
  }
})
