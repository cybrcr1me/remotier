#!/usr/bin/env node
/**
 * Compose the updater manifest a release publishes as `latest.json`.
 *
 * A release is built in two halves - CI does Windows and Linux, a developer machine does
 * macOS - so the manifest is written twice and has to survive being merged. Whoever runs
 * second passes the copy already on the release with `--merge` and adds its own platforms
 * to it.
 *
 * The merge is version-gated on purpose: a manifest left over from an earlier release
 * names an older version, and folding today's macOS entry into it would offer every
 * platform a download that is not the version the file claims.
 *
 *   node scripts/updater-manifest.mjs \
 *     --version 0.1.1 --tag v0.1.1 \
 *     --out latest.json [--merge current.json] [--notes-file notes.md] \
 *     --entry darwin-aarch64,Remotier.app.tar.gz,bundle/macos/Remotier.app.tar.gz.sig
 */

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

/** What the app is configured to fetch. The one place that knows the repository. */
const MANIFEST_PATH = '/releases/latest/download/latest.json'

/**
 * Where this release's assets live, derived from the endpoint the app will ask.
 *
 * Derived rather than configured twice: the manifest has to be reachable at the URL the
 * installed copies poll, and a second copy of the repository name in a build script is a
 * second thing to get wrong.
 */
export function releaseBase(endpoint, tag) {
  if (!endpoint.endsWith(MANIFEST_PATH)) {
    throw new Error(`the updater endpoint is not ${MANIFEST_PATH}: ${endpoint}`)
  }
  if (!tag) throw new Error('a manifest needs the tag whose assets it points at')
  return `${endpoint.slice(0, -MANIFEST_PATH.length)}/releases/download/${tag}`
}

/**
 * @param {object} options
 * @param {string} options.version            the version this release publishes
 * @param {string} options.baseUrl            where its assets live, without a trailing /
 * @param {{platform: string, asset: string, signature: string}[]} options.entries
 * @param {object|null} [options.existing]    the manifest already on the release
 * @param {string} [options.notes]            release notes, shown in the update panel
 * @param {Date} [options.now]
 */
export function buildManifest({ version, baseUrl, entries, existing = null, notes = '', now = new Date() }) {
  if (!version) throw new Error('a manifest needs the version it is publishing')
  if (!baseUrl) throw new Error('a manifest needs the base URL of the release assets')
  if (entries.length === 0) throw new Error('a manifest with no platforms updates nobody')

  // Only a manifest for this same version has anything worth keeping: the other half of
  // this release wrote it. One for another version is a leftover and is discarded.
  const carried = existing && existing.version === version ? existing : null

  const platforms = { ...(carried?.platforms ?? {}) }

  for (const { platform, asset, signature } of entries) {
    if (!signature) throw new Error(`${platform} has no signature, so nothing could verify it`)
    platforms[platform] = {
      signature: signature.trim(),
      // Pinned to this release rather than to /latest/download/: "latest" moves the
      // moment the next release is published, and a client that read this manifest a
      // second earlier would then download a different build than it verified against.
      url: `${baseUrl}/${asset}`,
    }
  }

  return {
    version,
    notes: notes || carried?.notes || '',
    pub_date: carried?.pub_date ?? now.toISOString(),
    platforms,
  }
}

/** `--entry darwin-aarch64,Remotier.app.tar.gz,path/to/file.sig` */
function parseEntry(value) {
  const [platform, asset, sigPath] = value.split(',')
  if (!platform || !asset || !sigPath) {
    throw new Error(`--entry wants platform,asset,signature-file - got "${value}"`)
  }
  return { platform, asset, signature: readFileSync(sigPath, 'utf8') }
}

function main(argv) {
  const options = { entries: [] }

  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i]
    const value = argv[i + 1]
    switch (flag) {
      case '--version': options.version = value; i += 1; break
      case '--tag': options.tag = value; i += 1; break
      case '--out': options.out = value; i += 1; break
      case '--merge': options.merge = value; i += 1; break
      case '--notes-file': options.notesFile = value; i += 1; break
      case '--entry': options.entries.push(parseEntry(value)); i += 1; break
      default: throw new Error(`unknown argument: ${flag}`)
    }
  }

  if (!options.out) throw new Error('--out is required')

  // A missing or unreadable `--merge` file is the ordinary case for whoever runs first.
  let existing = null
  if (options.merge) {
    try {
      existing = JSON.parse(readFileSync(options.merge, 'utf8'))
    } catch {
      existing = null
    }
  }

  const config = JSON.parse(readFileSync(join(ROOT, 'src-tauri/tauri.conf.json'), 'utf8'))
  const endpoint = config.plugins?.updater?.endpoints?.[0] ?? ''

  const manifest = buildManifest({
    version: options.version,
    baseUrl: releaseBase(endpoint, options.tag),
    entries: options.entries,
    existing,
    notes: options.notesFile ? readFileSync(options.notesFile, 'utf8').trim() : '',
  })

  writeFileSync(options.out, `${JSON.stringify(manifest, null, 2)}\n`)
  console.log(`${options.out}: ${manifest.version} for ${Object.keys(manifest.platforms).join(', ')}`)
}

// Run only when invoked directly, so the test can import the builder.
if (process.argv[1] && import.meta.url === `file://${process.argv[1]}`) {
  try {
    main(process.argv.slice(2))
  } catch (e) {
    console.error(e.message)
    process.exit(1)
  }
}
