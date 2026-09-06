# Remotier

Local-first SSH client (Termius alternative). Tauri 2 + Vue 3 + shadcn-vue.

## Hard rules

- **UI is shadcn-vue only.** Check `src/components/ui/` before writing markup. Add components with
  `bunx --bun shadcn-vue@latest add <name>`. Never hand-edit generated files in `src/components/ui/`
  unless intentionally customizing (and note it).
- **No hand-written CSS.** Tailwind v4 utilities + shadcn semantic tokens (`bg-background`,
  `text-muted-foreground`). No `<style>` blocks, no `.css` files beyond `src/assets/index.css`
  (`@theme` tokens) and the vendored `@xterm/xterm/css/xterm.css` import.
- **No `space-x-*` / `space-y-*`** — use `flex` + `gap-*`. Use `size-*` when w == h.
- **Vue: Composition API + `<script setup lang="ts">`.** No Options API. No `any`.
- **All disk, DB, crypto and network access lives in Rust.** The webview has no `fs`/`sql`
  capability by design. Add a `#[tauri::command]` instead of reaching for a JS plugin.
- **Package manager is `bun`.** `bun add`, `bunx --bun`.

## Layout

- `src/` — Vue app. `stores/` Pinia, `lib/` shared helpers, `views/` routed pages,
  `components/{layout,hosts,keys,identities,terminal}/` feature components.
- `src-tauri/src/` — `db/` SQLite + migrations, `crypto/vault.rs` secret sealing,
  `ssh/` russh engine, `vars.rs` placeholder resolver, `commands/` IPC surface.

## Conventions

- Terminal bytes travel over `tauri::ipc::Channel` as `InvokeResponseBody::Raw`, never Tauri events.
  Events are for low-frequency lifecycle only.
- Secrets are never stored plaintext: `crypto::vault` seals them with a keychain-backed DEK
  (XChaCha20-Poly1305) into the `secrets` table; rows hold a `secret_ref`.
- Placeholder resolution (`{{var}}`) is implemented **once**, in `src-tauri/src/vars.rs`.
  The UI previews via an invoke — do not reimplement it in TS.
- SQLite must never run on the main thread: DB commands are `#[tauri::command(async)]`.
- `var_values` is local-only and must never be included in any future sync payload.

## Commands

```bash
bun run tauri dev
cd src-tauri && cargo test && cargo clippy -- -D warnings
bunx vue-tsc --noEmit
```

## Key store

Debug and release builds deliberately differ:

- **Release**: DEK in the OS keychain, database `remotier.db`.
- **Debug**: DEK in `dev-dek.key` (0600) next to the database, which is `remotier-dev.db`.

Reason: on macOS a keychain item's ACL is bound to the binary that created it, so an
unsigned debug build loses access to its own entry on every rebuild. The separate dev
database also keeps experiments out of real data. See `src-tauri/src/state.rs`.

A missing or unreadable key store is **not** fatal - `AppState::vault()` returns an error
and only secret-touching commands fail. Never make it panic in `setup()`.

## SSH engine

- `ssh/connect.rs` dials, authenticates and opens the PTY. `ssh/session.rs` owns the live
  channel in a tokio task. `ssh/resolve.rs` turns a stored host into a dial target,
  applying group inheritance and `{{placeholders}}`.
- **Host key trust**: an unknown host is *refused* under `HostKeyPolicy::Strict`, and the
  error carries the fingerprint. The UI shows it and retries with `TrustOnce` /
  `TrustAndSave`. A *changed* key is refused under every policy - no flag overrides it.
- Terminal output goes over `tauri::ipc::Channel` as `InvokeResponseBody::Raw`, coalesced
  at 4ms / 64KB. Never move it onto Tauri events.

### Live SSH tests

`src-tauri/tests/ssh_live.rs` is `#[ignore]`d so `cargo test` stays hermetic. To run it:

```bash
ssh-keygen -t ed25519 -f /tmp/remotier-testkey -N ""
docker run -d --name remotier-test -p 2222:2222 \
  -e PASSWORD_ACCESS=true -e USER_NAME=test -e USER_PASSWORD=testpass \
  -e PUBLIC_KEY="$(cat /tmp/remotier-testkey.pub)" \
  lscr.io/linuxserver/openssh-server:latest
cargo test --test ssh_live -- --ignored --test-threads=1
```

For the agent test, use a throwaway agent - never the developer's own:

```bash
eval "$(ssh-agent -s -a /tmp/remotier-agent.sock)"
SSH_AUTH_SOCK=/tmp/remotier-agent.sock ssh-add /tmp/remotier-testkey
SSH_AUTH_SOCK=/tmp/remotier-agent.sock cargo test --test ssh_live -- --ignored
```

## Frontend tests

`bun run test` (vitest + happy-dom). Tests live beside their subject as `*.test.ts`.

Pure logic is kept out of components so it can be tested without a DOM:
`lib/layout.ts` (split tree), `lib/connect-flow.ts` (host key prompt), `lib/shortcuts.ts`
(key matching), `lib/terminal-theme.ts` (colour derivation), `stores/sessions.ts`.

Components stay thin over those modules. `TerminalPane.vue` is not unit tested - it needs
a real WebGL/canvas context; it is covered by running the app.

## Dev fixture

`cargo run --example seed_dev` adds a host pointing at the dockerised test server
(`test@127.0.0.1:2222`) to the **dev** database. Requires that container to be running.

## Literal `{{ }}` in templates

Vue's tokenizer closes an interpolation on the first `}}`, so `{{ '{{name}}' }}` is a
compile error, not a workaround. Use `@/lib/placeholder` to build the string in script.

Note `vue-tsc --noEmit` does **not** catch this - only `bun run build` does. Run the build,
not just the typecheck, before calling frontend work done.

## Session resume and workspaces

Tabs are serialised by `lib/session-state.ts` into the single `session_state` row,
debounced 400ms after any change. Workspaces use the same format under a name.

Two rules hold in both:

- **Session ids are never stored.** An SSH session does not survive a restart, and a stale
  id would render a pane as connected to something that no longer exists.
- **Restored panes do not auto-connect.** They keep their host and wait. Reconnecting on
  launch is opt-in (`session.autoReconnect`); otherwise opening the app would fire every
  saved connection at once.

A corrupt snapshot yields `null` and an empty workspace rather than throwing - losing the
tab layout is acceptable, failing to start is not.
