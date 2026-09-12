<div align="center">

<img src="design/mark.svg" width="88" alt="">

# Remotier

**A local-first SSH client.**
Hosts and groups with inherited defaults, keys that stay where you put them,
and tabbed terminals with split panes.

<img src="https://img.shields.io/badge/Tauri-2-D9FF00?style=flat-square&labelColor=0F110F" alt="Tauri 2">
<img src="https://img.shields.io/badge/Vue-3-D9FF00?style=flat-square&labelColor=0F110F" alt="Vue 3">
<img src="https://img.shields.io/badge/Rust-russh-D9FF00?style=flat-square&labelColor=0F110F" alt="Rust">
<img src="https://img.shields.io/badge/macOS_·_Windows_·_Linux-0F110F?style=flat-square" alt="macOS, Windows, Linux">

</div>

## Download

| macOS | Windows | Linux |
| --- | --- | --- |
| [Remotier.dmg][dmg] | [Remotier-setup.exe][exe] | [AppImage][appimage] · [.deb][deb] |

macOS builds are signed and notarised, and the app updates itself from these releases.
Windows is not yet signed, so SmartScreen warns until the binary earns reputation.

## Features

- **Hosts and groups** — nested groups where port, identity and jump host are inherited.
  A field left blank on a host follows the group chain.
- **Placeholders** — a shared group can set its username to `{{wg_user}}` and each person
  fills in their own value. Declarations are shared; the values never leave your machine.
- **Keys, without a walled garden** — generate or import into an encrypted vault, or point
  at what is already in `~/.ssh` and leave it there. ssh-agent and FIDO keys work too.
- **Terminals** — tabs and nested splits over xterm.js, saved as workspaces and restored
  on launch.
- **Optional sync** — end-to-end encrypted, on your own server or ours. Groups can be
  shared with colleagues. Keys and passwords are never uploaded, by construction.
- **Your OpenSSH setup** — verifies against your real `known_hosts`, imports from
  `~/.ssh/config`.

## Security

Passwords, passphrases and imported private keys are sealed with XChaCha20-Poly1305 under
a key in the OS keychain, so the database alone is useless. A host key that has *changed*
is refused outright — no prompt, no override.

## Development

Needs [Bun](https://bun.sh) and a [Rust](https://rustup.rs) toolchain.

```bash
bun install
bun run tauri dev
```

| Command | What it does |
| --- | --- |
| `bun run test` | Frontend tests |
| `bun run build` | Typecheck and build the frontend |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust tests |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets` | Lints |

Debug builds use a separate database and key store, so experiments never touch real hosts.
Architecture notes live in [CLAUDE.md](CLAUDE.md), releasing in
[docs/RELEASING.md](docs/RELEASING.md).

## Status

Beta. SFTP, port forwarding and snippets are not implemented yet.

© 2026 Eric Jaquome, Lucas Regh. All rights reserved — see [LICENSE](LICENSE).

[dmg]: https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.dmg
[exe]: https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier-setup.exe
[appimage]: https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.AppImage
[deb]: https://github.com/cybrcr1me/remotier/releases/latest/download/Remotier.deb
