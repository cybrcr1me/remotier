# Releasing

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`. They must match.
2. Commit, then tag: `git tag v0.1.0 && git push origin v0.1.0`.
3. The `release` workflow builds macOS (both architectures), Windows and Linux, and
   opens a **draft** release. Check the assets, then publish it.

## Bundles produced

| Platform | Artifacts |
| --- | --- |
| macOS | `.app`, `.dmg` (arm64 and x86_64 built separately) |
| Windows | NSIS `.exe` installer |
| Linux | `.deb`, `.AppImage` |

## Building locally

`bun run tauri build` from the repository root. Running it from `src-tauri/` fails: the
`beforeBuildCommand` needs the `package.json` that lives at the root.

The `.app` builds anywhere. The **`.dmg` step needs macOS Automation permission**: it
drives Finder over AppleScript to lay out the window, and without it the build stops with

```
execution error: Not authorised to send Apple events to Finder. (-1743)
```

Grant your terminal access under System Settings → Privacy & Security → Automation →
Finder, or just let CI produce the DMG. This is an environment permission, not a
configuration problem, and it does not affect the `.app`.

To skip bundling entirely while checking that the release profile compiles:

```bash
bun run tauri build --no-bundle
```

## Code signing

Not configured yet. Without it:

- **macOS** shows "Remotier cannot be opened because the developer cannot be verified".
  Users have to right-click → Open, once.
- **Windows** shows a SmartScreen warning until the binary builds reputation.

There is a second reason to fix this beyond the warnings: on macOS a keychain item's ACL
is bound to the binary that created it. An unsigned build gets a new identity on every
rebuild and loses access to its own vault key, which is why debug builds use a separate
on-disk key store (see `src-tauri/src/state.rs`). A stable signing identity is what makes
the keychain path work reliably in release builds.

To enable it:

- macOS: set `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` as repository secrets. Notarisation is
  what removes the Gatekeeper prompt.
- Windows: set `WINDOWS_CERTIFICATE` and `WINDOWS_CERTIFICATE_PASSWORD`, or configure a
  `signCommand` in `tauri.conf.json` for a cloud signing service.

## Auto updates

Not in this version. `tauri-plugin-updater` reads a signed manifest, so it needs the
signing keys above plus an `updater` section in `tauri.conf.json` pointing at the
releases feed. Adding it after signing is set up is a small change; adding it before is
not possible.
