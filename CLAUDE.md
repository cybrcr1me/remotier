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

## Packaging

`src-tauri/tests/bundle_config.rs` guards the release config: versions agreeing across the
three manifests, icons present and in the right binary format, the entitlements file
existing, a non-default identifier, and a CSP that forbids remote scripts. These fail
locally instead of during a release build on a platform nobody is watching.

Icons are generated from `design/icon.svg` with `bunx tauri icon design/icon.svg -o
src-tauri/icons` — regenerate rather than hand-editing the PNGs. ImageMagick renders that
SVG badly (it drops gradients and strokes); `tauri icon` uses resvg and gets it right.

Before tagging, run `scripts/check-version.sh`. See `docs/RELEASING.md`.

Building bundles locally: run `bun run tauri build` from the repository root, never from
`src-tauri/` (the `beforeBuildCommand` needs the root `package.json`). The `.dmg` step
needs macOS Automation → Finder permission; the `.app` does not. See `docs/RELEASING.md`.

## Host credentials vs identities

A host either uses an identity (and inherits one through its group chain) or carries its
own username plus credentials. `hosts.auth_kind` decides: `NULL` means "use the identity",
anything else means the host's own fields win outright. The editor presents this as an
either/or toggle, and saving one mode clears the other so it is never ambiguous.

`hosts.username` on its own still overrides the identity's username while leaving the
authentication method alone - useful when several hosts share a key but not a login name.

## Terminal sizing

`term.open()` and `FitAddon.fit()` must not run against a zero-sized container, and the
WebGL renderer must not be attached to one either - it produces a canvas that never
paints, which looks exactly like a session producing no output. `TerminalPane` therefore
fits and attaches WebGL from the resize observer, once the element reports real
dimensions.

## Live test isolation

`REMOTIER_KNOWN_HOSTS` overrides the known_hosts location. `ssh_live.rs` points it at a
temp file so the suite neither reads nor writes the developer's real file - otherwise
"a host never seen before" depends on what they happen to have trusted.

## Remotier design language

Dark only; there is no light theme. Import `tokens.css` and use `var(--rm-*)` — never raw hex in components.

Surfaces: canvas `--rm-void` #000, panels `--rm-shell` #050605, hover/selected `--rm-raised` #0F110F, borders `--rm-rule` #1F221F.
Text: primary `--rm-bone` #E8EBE6, body `--rm-ink-2` #A8ADA8, secondary `--rm-dim` #7E827E. Nothing dimmer than #7E827E for text a user must read.
Accent `--rm-lime` #D9FF00 is a signal, not a surface: one lime element per view, two at most. Never a lime gradient, never lime behind body text, never a second accent hue.
Status: connected = lime, warning = #E8A33D, error = #E5484D, idle = #6B6F6B, shown as a 6px square, never a pill or badge.
Type: 'Martian Mono' 600/700 for headings and the wordmark only (tracking −2% to −3.5%, never below 12px, never in paragraphs); 'JetBrains Mono' 300/400/500 for everything else. Labels are 10–11px uppercase, tracking 0.16em.
Layout: 8px spacing grid (4px in dense lists). Radius 0 for data rows and tables, 5px for controls, 10px for panels. Separate regions with 1px rules — no box-shadows, no elevation. Data rows are 36px with no zebra striping.
Motion: 120ms state, 240ms panels, `cubic-bezier(0.2, 0, 0, 1)`. No spring, no bounce. Connection state changes are instant. Loading is a blinking lime block cursor, never a spinner.
Every action needs a keyboard shortcut, displayed inline next to the action, not hidden in a menu.
UI copy: terse, exact, deadpan. State the fact, then the next keystroke ("No host selected. Pick one. ⌘K searches all 41."). No exclamation marks, no emoji, no "Oops", no "Let's", no welcome tours.

## Tabs stay mounted

`TerminalsView` renders every tab and hides the inactive ones with `v-show`. Do not switch
this to `v-if` or key the SplitView on the active tab: unmounting a pane disposes its
xterm, which drops the IPC channel, which ends the SSH session behind it - so switching
tabs would silently disconnect you.

Consequences that have to be kept in mind:

- A hidden tab reports zero size, so `TerminalPane` refits when `tabActive` turns true.
- Only the visible tab may take focus, hence the `tabActive` prop guarding `term.focus()`.
- Output from a hidden tab sets an unread marker, shown as a dot on the tab.
