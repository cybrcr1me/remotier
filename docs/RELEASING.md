# Releasing

A release is built in two halves. **CI builds Windows and Linux; macOS is built by hand**
on a developer machine and uploaded to the same draft.

That split is deliberate: GitHub bills macOS runners at ten times the Linux rate, and the
Developer ID certificate lives in a keychain rather than in repository secrets. The
release stays a **draft** until the Mac bundle is attached, so a version is never
published with no Mac download.

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`. They must match: `bun run check:version`.
2. Commit, then tag: `git tag v0.1.0 && git push origin v0.1.0`.
3. The `release` workflow checks the tag against the manifests, opens a **draft** release,
   builds Windows and Linux, and attaches them under their published names.
4. On the Mac, once CI has opened the draft: `bun run build:mac`. It builds, signs,
   notarises, verifies and attaches the DMG to that draft.
5. Check that all four assets are there under the names below, then publish:

   ```bash
   gh release view v0.1.0 --web
   gh release edit v0.1.0 --draft=false
   ```

   Publishing stays manual. Everything else in step 4 is not.

## Credentials

Copy `.env.example` to `.env` and fill it in once:

```bash
APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
APPLE_ID="you@example.com"
APPLE_PASSWORD="abcd-efgh-ijkl-mnop"   # app-specific password, not the account one
APPLE_TEAM_ID="TEAMID"
```

`.env` is gitignored, and the script **refuses to run if it ever stops being** - by the
time anyone reads a warning about a committed password, the commit exists. Anything
already exported wins over the file, so a one-off can override a single value.

Nothing in `.env` reaches the app: Vite only exposes `VITE_`-prefixed variables to the
frontend bundle, and the Rust side never reads these.

## Published asset names

The website links straight at the newest build:

```
https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.dmg
https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier-setup.exe
https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.AppImage
https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.deb
```

GitHub resolves that path **by filename, not by version**, so every release has to publish
these exact four names. Tauri's own output is version-stamped
(`Remotier_0.1.0-beta2_amd64.AppImage`), so both halves of the release rename before
uploading - the workflow in its "Collect the bundles" step, `build-mac.sh` by copying the
stapled DMG to `Remotier.dmg`. **Renaming an asset breaks every download link on the
site**, so treat these as a published interface.

The workflow fails if a glob matches anything other than exactly one file, rather than
publishing a release one asset short - a missing file here is a dead download button, and
nothing else would notice.

| Platform | Published as | Built by |
| --- | --- | --- |
| macOS | `Remotier.dmg` (universal) | `bun run build:mac`, by hand |
| Windows | `Remotier-setup.exe` (NSIS) | the `release` workflow |
| Linux | `Remotier.AppImage`, `Remotier.deb` | the `release` workflow |

The version is on the release itself, not in the filenames.

## The macOS build

`scripts/build-mac.sh`, behind `bun run build:mac`. It refuses to produce an unsigned
bundle, builds `universal-apple-darwin` so one download covers both architectures, and
verifies what it made before telling you it worked:

- `codesign --verify --deep --strict` always.
- `spctl --assess` and `xcrun stapler validate` when the notarisation credentials are
  present. A notarised build that fails these would warn on the *user's* machine and
  nowhere else, which is the worst place to find out.

Then it uploads the DMG to the release for `v<version>` with `gh release upload
--clobber`, so a rebuild replaces the asset instead of failing on the name.

### Notarisation, and why the checks are what they are

Signing proves who built it. **Notarisation** is Apple scanning the binary and issuing a
ticket saying it passed — without one, macOS refuses to open the app on any machine but
yours, no matter how correctly it is signed.

Four things have to hold, and the script fails on each rather than letting Apple reject
the upload ten minutes later:

- **Hardened runtime.** Notarisation refuses a bundle without it. Tauri signs with it on;
  the script checks the `runtime` flag is actually in the signature.
- **Entitlements with no debug keys.** `src-tauri/entitlements.plist` grants the network
  client and user-selected file access the app needs under the hardened runtime.
  `get-task-allow` would fail the submission — do not add it.
- **A secure timestamp**, which `codesign` adds automatically.
- **A stapled ticket.** Issued is not enough: stapling writes it into the artifact, which
  is what lets Gatekeeper clear the app on a machine that is offline or behind a captive
  portal. Tauri staples the `.app`; the script submits and staples the **DMG** too, since
  that is the file people actually download.

It ends by asking `spctl` — Gatekeeper's own verdict — rather than trusting that the
earlier steps succeeded.

`APPLE_PASSWORD` is an **app-specific password** from appleid.apple.com, not the Apple ID
password, and `APPLE_TEAM_ID` is the 10-character code in the parentheses of your signing
identity. Notarisation takes a few minutes; `notarytool --wait` blocks until Apple
answers, and prints a submission id you can pass to `xcrun notarytool log <id>` if it is
rejected.

It will not *create* a release: the tag's CI run does that, and a release created here
would become a second, Mac-only one under the same tag as soon as CI caught up. If the
draft is not there yet, the script says so and prints the upload command to run later.

Flags: `--no-upload` builds and verifies only; `--list-identities` prints the signing
identities this keychain holds and stops.

Notarisation is optional to the script and not optional in practice: without it macOS
still shows "Remotier cannot be opened because the developer cannot be verified" and the
user has to right-click → Open. The script says so loudly and carries on, because a
signed-but-unnotarised build is still worth having while testing.

The **`.dmg` step needs macOS Automation permission**: it drives Finder over AppleScript
to lay out the window, and without it the build stops with

```
execution error: Not authorised to send Apple events to Finder. (-1743)
```

Grant your terminal access under System Settings → Privacy & Security → Automation →
Finder. This is an environment permission, not a configuration problem, and it does not
affect the `.app`.

## Building locally, unsigned

`bun run tauri build` from the repository root. Running it from `src-tauri/` fails: the
`beforeBuildCommand` needs the `package.json` that lives at the root.

On Debian/Ubuntu the build needs, on top of Tauri's own list:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf libgtk-3-dev libudev-dev
```

`libudev-dev` is the one no Tauri guide mentions: `hidapi` links it, and `hidapi` is there
because `ctap-hid-fido2` talks to security keys. Without it the build fails deep in a
build script with "Unable to find libudev", naming neither Remotier nor the feature that
pulled it in. The `.deb` declares the runtime half (`libudev1`) for the same reason.

To skip bundling entirely while checking that the release profile compiles:

```bash
bun run tauri build --no-bundle
```

## Code signing

**macOS is signed** through `bun run build:mac` and the `APPLE_*` variables above.

**Windows is not.** SmartScreen warns until the binary builds reputation. To fix it, set
`WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD` as repository secrets, or
configure a `signCommand` in `tauri.conf.json` for a cloud signing service - most
certificate vendors now require the key to stay in an HSM, which is what `signCommand` is
for.

Signing matters beyond the warnings: on macOS a keychain item's ACL is bound to the binary
that created it. An unsigned build gets a new identity on every rebuild and loses access to
its own vault key, which is why debug builds use a separate on-disk key store (see
`src-tauri/src/state.rs`). A stable signing identity is what makes the keychain path work
in release builds.

If macOS builds ever move to CI, they need `APPLE_CERTIFICATE` and
`APPLE_CERTIFICATE_PASSWORD` (a base64 `.p12` and its password) on top of the variables
above, because a runner has no keychain to read the identity from.

## Auto updates

Not in this version. `tauri-plugin-updater` reads a signed manifest, so it needs an
updater keypair (`bun run tauri signer generate`) plus an `updater` section in
`tauri.conf.json` pointing at the releases feed. Note that both halves of the release have
to produce the signature, so the manual macOS step would have to sign its own bundle and
merge its entry into `latest.json` - that is the part to think about before enabling it.
