import { describe, expect, it } from 'vitest'
// @ts-expect-error - a plain script, deliberately not part of the app's TypeScript build.
import { buildManifest, releaseBase } from './updater-manifest.mjs'

const BASE = 'https://github.com/cybrcr1me/remotier/releases/download/v1.2.0'

const windows = {
  platform: 'windows-x86_64',
  asset: 'Remotier-setup.exe',
  signature: 'WINDOWS-SIG\n',
}

const mac = {
  platform: 'darwin-aarch64',
  asset: 'Remotier.app.tar.gz',
  signature: 'MAC-SIG',
}

describe('releaseBase', () => {
  const endpoint = 'https://github.com/cybrcr1me/remotier/releases/latest/download/latest.json'

  it('points at the tag, taking the repository from the endpoint', () => {
    expect(releaseBase(endpoint, 'v1.2.0')).toBe(BASE)
  })

  it('refuses an endpoint that is not the static manifest', () => {
    // The whole scheme rests on the manifest being reachable at a version-free URL; an
    // endpoint of some other shape means the two halves have drifted apart.
    expect(() => releaseBase('https://example.com/updates/{{target}}', 'v1.2.0'))
      .toThrow(/latest\.json/)
  })
})

describe('buildManifest', () => {
  it('points each platform at this release, not at latest', () => {
    // "latest" moves when the next release is published, so a client that fetched this
    // manifest a moment earlier would download a build its signature does not cover.
    const manifest = buildManifest({ version: '1.2.0', baseUrl: BASE, entries: [windows] })

    expect(manifest.platforms['windows-x86_64'].url).toBe(`${BASE}/Remotier-setup.exe`)
    expect(manifest.platforms['windows-x86_64'].signature).toBe('WINDOWS-SIG')
    expect(manifest.version).toBe('1.2.0')
  })

  it('keeps the platforms the other half of the release wrote', () => {
    const first = buildManifest({ version: '1.2.0', baseUrl: BASE, entries: [windows] })
    const second = buildManifest({
      version: '1.2.0',
      baseUrl: BASE,
      entries: [mac],
      existing: first,
    })

    expect(Object.keys(second.platforms).sort()).toEqual(['darwin-aarch64', 'windows-x86_64'])
    expect(second.pub_date).toBe(first.pub_date)
  })

  it('discards a manifest left over from an earlier release', () => {
    // Folding today's macOS entry into last release's file would offer Windows a download
    // that is not the version the manifest claims to be.
    const stale = buildManifest({ version: '1.1.0', baseUrl: BASE, entries: [windows] })
    const manifest = buildManifest({
      version: '1.2.0',
      baseUrl: BASE,
      entries: [mac],
      existing: stale,
    })

    expect(Object.keys(manifest.platforms)).toEqual(['darwin-aarch64'])
  })

  it('refuses an entry with no signature', () => {
    expect(() =>
      buildManifest({
        version: '1.2.0',
        baseUrl: BASE,
        entries: [{ ...mac, signature: '' }],
      }),
    ).toThrow(/signature/)
  })

  it('refuses to write a manifest that updates nobody', () => {
    expect(() => buildManifest({ version: '1.2.0', baseUrl: BASE, entries: [] }))
      .toThrow(/no platforms/)
  })
})
