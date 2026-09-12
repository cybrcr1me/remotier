/** Account profiles and the key repository. */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { errorMessage, ipc } from '@/lib/ipc'
import type { Identity, IdentityInput, KeyAlgorithm, KeyMetaInput, SshKey } from '@/lib/types'

export const useCredentialsStore = defineStore('credentials', () => {
  const identities = ref<Identity[]>([])
  const keys = ref<SshKey[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)

  const identityById = computed(() => new Map(identities.value.map(i => [i.id, i])))
  const keyById = computed(() => new Map(keys.value.map(k => [k.id, k])))

  async function load() {
    loading.value = true
    error.value = null
    try {
      const [loadedIdentities, loadedKeys] = await Promise.all([
        ipc.listIdentities(),
        ipc.listKeys(),
      ])
      identities.value = loadedIdentities
      keys.value = loadedKeys
    } catch (e) {
      error.value = errorMessage(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function createIdentity(input: IdentityInput) {
    const identity = await ipc.createIdentity(input)
    identities.value.push(identity)
    return identity
  }

  async function updateIdentity(id: string, input: IdentityInput) {
    const identity = await ipc.updateIdentity(id, input)
    const index = identities.value.findIndex(i => i.id === id)
    if (index !== -1) identities.value[index] = identity
    return identity
  }

  async function deleteIdentity(id: string) {
    await ipc.deleteIdentity(id)
    identities.value = identities.value.filter(i => i.id !== id)
  }

  async function generateKey(
    label: string,
    algorithm: KeyAlgorithm,
    comment?: string,
    passphrase?: string,
  ) {
    const key = await ipc.generateKey(label, algorithm, comment, passphrase)
    keys.value.push(key)
    return key
  }

  async function importKey(label: string, pem: string, passphrase?: string) {
    const key = await ipc.importKey(label, pem, passphrase)
    keys.value.push(key)
    return key
  }

  async function updateKey(id: string, input: KeyMetaInput) {
    const key = await ipc.updateKey(id, input)
    const index = keys.value.findIndex(k => k.id === id)
    if (index !== -1) keys.value[index] = key
    return key
  }

  async function deleteKey(id: string) {
    await ipc.deleteKey(id)
    keys.value = keys.value.filter(k => k.id !== id)
  }

  return {
    identities,
    keys,
    loading,
    error,
    identityById,
    keyById,
    load,
    generateKey,
    importKey,
    createIdentity,
    updateIdentity,
    deleteIdentity,
    updateKey,
    deleteKey,
  }
})
