/**
 * Searching the selfh.st icon catalog.
 *
 * Under 3,000 entries, searched in the webview as someone types - small enough that a ranked
 * scan beats building an index, and pure so it can be tested without either.
 */

import type { CatalogIcon } from '@/lib/types'

/**
 * How many results the picker shows. Each one is an image fetched on first sight, so a
 * short prefix must not turn into hundreds of downloads.
 */
export const RESULT_LIMIT = 24

/**
 * Icons matching `query`, best first: an exact name or reference, then one starting with the
 * query, then one containing it, then a tag containing it. Equal matches keep catalog order,
 * which is alphabetical.
 *
 * An empty query matches nothing - the picker asks for a search rather than listing
 * thousands of logos.
 */
export function searchCatalog(
  entries: CatalogIcon[],
  query: string,
  limit = RESULT_LIMIT,
): CatalogIcon[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return []

  const ranked: { entry: CatalogIcon, rank: number }[] = []
  for (const entry of entries) {
    const rank = rankOf(entry, needle)
    if (rank !== null) ranked.push({ entry, rank })
  }

  // `sort` is stable, so entries of equal rank stay in catalog order.
  return ranked
    .sort((a, b) => a.rank - b.rank)
    .slice(0, limit)
    .map(({ entry }) => entry)
}

function rankOf(entry: CatalogIcon, needle: string): number | null {
  const name = entry.name.toLowerCase()
  if (name === needle || entry.reference === needle) return 0
  if (name.startsWith(needle) || entry.reference.startsWith(needle)) return 1
  if (name.includes(needle) || entry.reference.includes(needle)) return 2
  if (entry.tags.some(tag => tag.toLowerCase().includes(needle))) return 3
  return null
}
