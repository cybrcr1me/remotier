# Remotier

A local-first SSH client. Hosts and groups with inherited defaults, a key repository that
does not force you to import anything, and tabbed terminals with split panes.

Built with Tauri 2, Vue 3 and russh.

## Why

Termius grew heavy and moved most of the SSH workflow behind a subscription. Remotier
keeps the parts that matter for day-to-day work and stores everything on your machine.

## Features

- **Hosts and groups** — nested groups where port, identity and jump host are inherited.
  A field left blank on a host follows the group chain.
- **Placeholders** — a shared group can define its username as `{{wg_user}}`, and each
  person fills in their own value. The declarations are shared; the values never leave
  your machine. Useful for bastions such as Warpgate.
- **Keys, without a walled garden** — generate or import keys into an encrypted vault, or
  point at the keys already in `~/.ssh` and leave them where they are. ssh-agent works too.
- **Terminals** — tabs and nested splits over xterm.js, saved as workspaces and restored
  on launch.
- **Shares your OpenSSH setup** — verifies against your real `~/.ssh/known_hosts`, and can
  import hosts from `~/.ssh/config`.

## Security

Passwords, key passphrases and imported private keys are encrypted with
XChaCha20-Poly1305 under a key held in the OS keychain, so the database on its own is
useless. A host key that has *changed* is refused outright rather than offered as a
prompt.

## Development

Requires [Bun](https://bun.sh) and a [Rust](https://rustup.rs) toolchain.

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

Live SSH tests need a real server and are `#[ignore]`d by default; see `CLAUDE.md`.

Debug builds use a separate database and key store from release builds, so experiments
never touch your real hosts.

## Releasing

See [docs/RELEASING.md](docs/RELEASING.md).

## Status

MVP. SFTP, port forwarding, snippets and cloud sync are not implemented. Builds are not
yet code signed, so macOS and Windows will warn on first launch.
