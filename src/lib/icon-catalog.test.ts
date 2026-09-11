import { describe, expect, it } from 'vitest'
import type { CatalogIcon } from '@/lib/types'
import { searchCatalog } from './icon-catalog'

const icon = (name: string, reference: string, tags: string[] = []): CatalogIcon => ({
  name,
  reference,
  tags,
})

const CATALOG = [
  icon('Grafana', 'grafana', ['Monitoring']),
  icon('Home Assistant', 'home-assistant', ['Home Automation']),
  icon('Portainer', 'portainer', ['Containers']),
  icon('Uptime Kuma', 'uptime-kuma', ['Monitoring']),
]

const references = (entries: CatalogIcon[]) => entries.map(entry => entry.reference)

describe('searchCatalog', () => {
  it('matches nothing until something is typed', () => {
    expect(searchCatalog(CATALOG, '')).toEqual([])
    expect(searchCatalog(CATALOG, '   ')).toEqual([])
  })

  it('ignores case and surrounding space', () => {
    expect(references(searchCatalog(CATALOG, '  PORTAINER '))).toEqual(['portainer'])
  })

  it('puts an exact match first, then a prefix, then a substring', () => {
    const catalog = [
      icon('Uptime Kuma', 'uptime-kuma'),
      icon('Kuma Status', 'kuma-status'),
      icon('Kuma', 'kuma'),
    ]
    expect(references(searchCatalog(catalog, 'kuma'))).toEqual(['kuma', 'kuma-status', 'uptime-kuma'])
  })

  it('finds an icon by its reference when the name is spelled differently', () => {
    expect(references(searchCatalog(CATALOG, 'home-ass'))).toEqual(['home-assistant'])
  })

  it('ranks a tag match below every name match', () => {
    const catalog = [icon('Prometheus', 'prometheus', ['Monitoring']), icon('Monit', 'monit')]
    expect(references(searchCatalog(catalog, 'monit'))).toEqual(['monit', 'prometheus'])
  })

  it('keeps catalog order among equal matches', () => {
    expect(references(searchCatalog(CATALOG, 'monitoring'))).toEqual(['grafana', 'uptime-kuma'])
  })

  it('stops at the limit', () => {
    expect(searchCatalog(CATALOG, 'a', 2)).toHaveLength(2)
  })
})
