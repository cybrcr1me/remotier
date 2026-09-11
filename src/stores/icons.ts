/**
 * Catalog icon images, and the catalog itself, fetched through Rust and kept for the session.
 *
 * Every card, tree row and tab showing the same icon asks for it; the store turns that into
 * one request. A failed fetch is remembered as `null`, so an offline session does not retry
 * on every render - the next launch tries again.
 */

import { errorMessage, ipc } from '@/lib/ipc'
import type { CatalogIcon } from '@/lib/types'
import { defineStore } from 'pinia'
import { reactive, shallowRef } from 'vue'

export const useIconsStore = defineStore('icons', () => {
  /** Reference to `data:` URL, or `null` once fetching it failed. Absent: not asked for yet. */
  const images = reactive(new Map<string, string | null>())
  const pending = new Map<string, Promise<void>>()

  function image(reference: string): string | null | undefined {
    return images.get(reference)
  }

  function load(reference: string): Promise<void> {
    if (images.has(reference)) return Promise.resolve()

    let request = pending.get(reference)
    if (!request) {
      request = ipc
        .iconImage(reference)
        .then((url) => {
          images.set(reference, url)
        })
        .catch((e) => {
          console.warn(`could not load the ${reference} icon: ${errorMessage(e)}`)
          images.set(reference, null)
        })
        .finally(() => pending.delete(reference))
      pending.set(reference, request)
    }
    return request
  }

  /** Shallow: thousands of entries that are only ever replaced whole. */
  const catalog = shallowRef<CatalogIcon[] | null>(null)
  let catalogRequest: Promise<CatalogIcon[]> | null = null

  /** The catalog, fetched on first use. Rejects when it is neither cached nor reachable. */
  function loadCatalog(): Promise<CatalogIcon[]> {
    if (catalog.value) return Promise.resolve(catalog.value)

    catalogRequest ??= ipc
      .iconCatalog()
      .then((entries) => {
        catalog.value = entries
        return entries
      })
      .finally(() => {
        catalogRequest = null
      })
    return catalogRequest
  }

  return { images, image, load, catalog, loadCatalog }
})
