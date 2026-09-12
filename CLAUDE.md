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
  The UI previews via an invoke — do not reimplement it in TS. Scopes are `global` (the
  account, empty `scope_id`), `group` and `host`, layered weakest first so a nearer scope
  overrides a wider one. Settings → Placeholders lists every declaration with this machine's
  answer (`lib/placeholder-list.ts` pairs the two halves) and is where **account**
  placeholders are declared; a group's are declared in the group editor, because they travel
  with the group and are shared with whoever else has it. Migration 4 rebuilds both variable
  tables — SQLite cannot widen a `CHECK` in place — and a test covers the data surviving it.
- SQLite must never run on the main thread: DB commands are `#[tauri::command(async)]`.
- `var_values` is local-only and must never be included in any future sync payload.

## Commands

```bash
bun run tauri dev
cd src-tauri && cargo test && cargo clippy -- -D warnings
bunx vue-tsc -b   # not --noEmit: the root tsconfig has no files of its own, so that checks nothing
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

### Hardware-backed keys

### A rejected key falls back

### The connect retry loop

`connect-flow.ts` answers everything a connection can ask for - unresolved variables, an
unknown host key, a password, a security key's PIN - and any of them can come up on the
same attempt. Three rules keep that working:

- **Every branch spreads `attempt`, never `request`.** Spreading the original throws away
  what earlier rounds collected, so a password given before the host key was queried is
  lost and asked for again.
- **Every branch `continue`s.** Connecting directly from a branch leaves the loop, and a
  prompt raised by *that* attempt has nowhere to be handled. This is exactly how an unknown
  host plus a hardware key failed with no PIN prompt at all: the host key branch returned
  its own `connect` call, so the `pinRequired` it raised escaped unanswered.
- **Every attempt gets its own output `Channel`.** A failed attempt drops the Rust end;
  Tauri then sends an end message and the webview unregisters the handler. A retry that
  reuses the first attempt's channel connects, streams output to a handler that no longer
  exists, and shows a blank terminal. A PIN-protected security key always needs a second
  attempt, which is how this surfaced - but a host key or password prompt did the same.

Each prompt is guarded so it cannot be asked twice, and the loop is bounded at one round
per prompt plus the attempt that succeeds.

When the server turns a key down, `after_key_rejected` asks what it will still accept and
acts on the answer, which is what OpenSSH does. A key that is simply not in
`authorized_keys` on this host would otherwise fail outright on a host that would happily
take a password.

- Password or keyboard-interactive still offered, and a password already typed: it is
  tried.
- Offered but nothing typed yet: `PasswordRequired`, so the UI prompts and retries.
- Nothing but public keys offered: say so, rather than prompting for a password the server
  would refuse anyway.

`Target::typed_password` exists for this. It carries a password the user typed even when
the host authenticates with a key, which is the only way the retry can use it - the key
arms of `AuthMaterial` have nowhere else to put it.

A FIDO key - `sk-ssh-ed25519@openssh.com` or `sk-ecdsa-sha2-nistp256@openssh.com`, what
`ssh-keygen -t ed25519-sk` writes for a YubiKey - stores only a credential handle on disk.
The private scalar never leaves the token.

`ssh-key` parses such a file perfectly well and then cannot sign with it: `KeypairData`'s
`Signer` has no arm for the SK types and falls through to "unsupported algorithm". The
failure surfaces as the server rejecting a key that is plainly in `authorized_keys`, which
sends you looking in the wrong place entirely.

So `connect.rs` checks the algorithm **before** decrypting - the public half of an OpenSSH
private key is cleartext even when the secret is encrypted, and a token key needs no
passphrase from us anyway - and routes these to `authenticate_agent`, matching on the
public half. The agent can sign because it talks to the token. If the agent has not got it,
the error says so and says what to run.

russh does advertise both SK algorithms, so the agent path negotiates correctly.

`PublicKeyInfo.hardwareBacked` carries this to the UI, which badges such keys in the
keychain: a FIDO key looks like any other key file, and importing one would otherwise
produce a key nothing in the app can use. Their row offers **Add to agent**
(`ssh_agent_add` → `agent::add_to_agent`, which shells out to `ssh-add`) instead of the
usual "Use this", because there is no secret in the file to adopt.

`ssh-add` is invoked with no flag for a key file and `-K` for resident keys, and a 60s
timeout - the token has to be touched while it runs.

**PINs.** The first attempt runs with `SSH_ASKPASS_REQUIRE=never`, so a key that wants a
PIN fails at once instead of blocking on a prompt with no terminal to appear on. That
failure opens a PIN dialog and retries - the same try-then-prompt shape as the password
prompt on connect, and for the same reason: most `sk-` keys are touch-only, so demanding a
PIN up front would be a prompt with no answer.

The retry writes a throwaway `SSH_ASKPASS` helper. **The PIN is never in that file** - the
script echoes an environment variable set only on the `ssh-add` child, so the secret is
never on disk and never on a command line where `ps` would show it. The helper lives in a
0700 directory, is deleted on `Drop`, and its directory name carries a per-instance counter
as well as the pid: two keys can be added at once, and a shared directory would have one
helper's cleanup delete the other's script mid-touch. Unit tests cover all three.

Windows has no askpass shell script, so a PIN there still needs a terminal.

**Which agent.** `ssh.agentSocket` overrides `SSH_AUTH_SOCK` (blank uses the environment's).
This is not a nicety: macOS hands every GUI app launchd's agent, and Apple's OpenSSH ships
no `ssh-sk-helper`, so that agent accepts a FIDO key with `ssh-add` and then answers every
sign request with `SSH_AGENT_FAILURE`. Without the setting a GUI app cannot be pointed at a
Homebrew agent that does have the helper, so hardware keys are simply unusable. It also
covers 1Password, Secretive and gpg-agent. `Target::agent_socket` carries it even on key
auth, because a stored key can turn out to be token-backed.

**In-process FIDO signing** lives in `ssh/fido.rs`, over CTAP2 via `ctap-hid-fido2`. Its
only C dependency is `hidapi`, which vendors its source and builds with `cc` - no CMake, no
NASM, no OpenSSL, which is the same constraint that chose `ring` over `aws-lc-rs`.

On **Linux** it does need `libudev-dev` at build time and `libudev1` at run time, which is
why every Linux CI job installs it and the `.deb` declares it. No Tauri setup snippet
lists it - it comes from this crate graph, not from Tauri - and the build fails inside
hidapi's build script with "Unable to find libudev", naming neither this app nor security
keys.

The signature format is OpenSSH's PROTOCOL.u2f: `string alg`, `string sig`, `byte flags`,
`uint32 counter`. Two details are load-bearing and both are tested:

- The **flags byte is copied out of the raw authenticator data**, not rebuilt from the
  parsed struct. The verifier recomputes `sha256(rpId) || flags || counter || challenge`,
  so a byte that merely means the same thing does not verify.
- The **counter is big-endian**. Little-endian verifies on the machine that wrote it and
  nowhere else, which is the worst way for this to be wrong.

A token refusing for want of a PIN comes back as `Error::PinRequired`, distinct from a
generic failure, because it is recoverable by asking - and a key file's `verify-required`
flag does not reliably predict it.

`Stage::TouchRequired` is emitted before the assertion so the wait is visible: the panel
shows "Touch your security key" in lime with a spinner. It is the one stage blocked on the
user rather than on us, hence its own `action` level in `connection-log.ts` - and the
spinner is dropped as soon as another line follows, because by then the wait is over.

**A `verify-required` credential is asked for its PIN before the token is touched.** An
authenticator does not treat such a credential as merely locked: it hides it from any
assertion that is not performing verification and answers `CTAP2_ERR_NO_CREDENTIALS`, which
reads as "wrong key" rather than "needs a PIN". The key file's flags say so up front
(`0x04`), so `sign` returns `PinRequired` without contacting the token at all - which is
also a touch the user would otherwise waste.

**The token is given the data, not its hash.** sshd verifies with `sha256(data)` as the
client data hash, and ctap-hid-fido2 computes the client data hash by hashing whatever
challenge it is handed. Hashing first made every signature cover `sha256(sha256(data))`,
which verifies nowhere; the server answered with an ordinary auth failure and
`after_key_rejected` turned it into a password prompt. `assertion_request` is pure so a test
pins this.

**A refused signature is reported, not turned into a password prompt.** russh asks the
token to sign only after the server has accepted the key, so a failure after signing, with
no partial success, is a signature or verification problem. `TokenSigner::signed` tells the
two apart; only a key the server never accepted falls back to a password.

**Do not use `get_assertion`.** Its args default to `uv: Some(true)`, demanding user
verification on every assertion, which a token without it configured answers with
`CTAP2_ERR_INVALID_OPTION` - so the convenience wrapper cannot sign an ordinary touch-only
key at all. `sign` builds the args itself: `up` always, `uv` never asked for directly, and
a PIN supplied through `.pin()`, which conveys verification and clears the option. This is
what OpenSSH does.

**`Signer::auth_sign` returns the request with the signature appended, not the signature.**
russh writes whatever comes back as the packet, so returning the bare signature makes the
server see a malformed message and disconnect - with no auth failure to report, which looks
from the app like nothing happened at all. The signature is appended to the buffer russh
supplied, length-prefixed, exactly as the agent path frames it: the declared length is
`name.len() + sig.len() + 8 + 5` for an `sk-` signature, and a test pins the blob to that.

`fido::TokenSigner` implements russh's `Signer` - the same seam the agent client plugs
into - so `authenticate_publickey_with` drives the token exactly as it would drive an
agent. `sk-` keys no longer touch the agent at all. Signing runs on `spawn_blocking`,
because it blocks for as long as the user takes to touch the token.

russh's `Signer::Error` carries nothing useful, so the signer keeps the real error and the
caller reads it back with `take_failure`; without that every token problem would arrive as
"authentication failed".

**PINs** travel the same road as passwords: the token refuses, `Error::PinRequired` reaches
the frontend, `connect-flow.ts` prompts once and retries with `request.pin`. Once, not
repeatedly - a second prompt for a PIN just refused is a loop, not a recovery. The PIN is
never stored, and is only asked for after a refusal because a key file's `verify-required`
flag does not reliably predict it.

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

## Optional boolean props

Vue casts an absent `boolean` prop to `false`, not `undefined`. So
`defineProps<{ showIcon?: boolean }>()` with `v-if="showIcon !== false"` hides the thing
whenever the prop is left off - which is how the icon picker vanished from both editors
for days. A boolean prop that should default on gets a real default from `withDefaults`.

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

## Cloud sync

Optional, account-based, and off until someone signs in. The server is a separate repo
(`remotier-sync`, axum over SQLite or Postgres); both sides compile
`remotier-sync-proto`, so there is one definition of the wire format. An end-to-end
format that drifts between two hand-written implementations fails as "decryption
failed", which says nothing about which side is wrong.

**Hybrid, not opaque.** Each record is a plaintext routing header - id, kind, group,
sort, clock, tombstone - around an encrypted payload. The header is what lets the server
route a shared group to its members without reading it. The server learns record counts,
the group tree shape, share membership and edit timing; never a hostname, username,
label, tag, port or placeholder. Fields in both places are authoritative **inside** the
ciphertext; the plaintext copy is a routing hint.

**No secrets sync.** Not by filtering - no payload struct in `record.rs` has anywhere to
put a password, a passphrase or a private key, and a test fails if a field named like one
appears. A shared group therefore hands a colleague addresses and usernames, never a
working credential.

### Keys

Argon2id over password + lowercased email gives a master key, split by HKDF into an
`authKey` sent to the server and a `wrapKey` that never leaves the device. `wrapKey`
wraps an X25519 account secret and a 32-byte personal content key, both stored server-side
as ciphertext so a new device bootstraps from the password alone. A **recovery code**,
shown once, wraps the same two blobs a second time - the server holds no key and cannot
reset a password. Because the account keys never change, a password change re-encrypts
zero records; `a_password_change_does_not_touch_a_single_record` pins that.

Locally the account keys are sealed into the `secrets` table under the existing DEK, so
sync resumes after a restart without asking for the password again. That is deliberate:
this database already protects SSH private keys that way, so demanding a password to sync
but not to use a stored key would be theatre. No vault, no sync - the same degradation as
every other secret-touching feature.

### Change tracking is done by trigger

Migration 5 adds `sync_meta` (dirty flag, tombstone, server sequence) and `sync_state`,
plus triggers on the six syncable tables. Triggers rather than edits to ~20 command
functions, for two reasons: a dirty flag that must be remembered will be forgotten, and
only a trigger sees the rows an `ON DELETE CASCADE` removes when a group is deleted.

**That cascade needs `PRAGMA recursive_triggers`**, set in `Db::configure`. Without it a
deleted subtree produces no tombstones and the next pull resurrects the lot.
`cascade_delete_leaves_tombstones` is the test; do not remove the pragma.

`var_values` and `session_state` have no triggers, on purpose - both are local-only, and
`local_only_data_is_never_collected` asserts on what `collect` actually produces rather
than on the intent.

### Things that look like details and are not

- **The database lock is never held across a network call.** `Db` is one
  `Mutex<Connection>` whose contract is "nothing awaits while the guard is held", so every
  cycle is read-under-lock, release, HTTP, reacquire to write. Holding it would stall every
  terminal command in the app for as long as the network took.
- **Payloads carry a key *fingerprint*, never a `key_id`.** Key rows do not sync, and
  `import_key` / `generate_key` mint a fresh uuid on every machine, so a synced id would
  point at nothing - or at a different key that took the id. Applying resolves the
  fingerprint against the local keychain and stores NULL when the key is not here.
- **A whole pull is applied in one transaction with `defer_foreign_keys`.** A host and its
  group can land on either side of a page boundary. A reference to something this device
  genuinely lacks becomes NULL rather than failing the record - refusing a host over a
  missing identity leaves the user with nothing.
- **`apply` clears the dirty flag after writing**, because the domain write fires the
  table's own trigger. Without that, every pulled record would be pushed straight back.
  `a_pulled_record_is_not_pushed_straight_back` is the test.
- **Settings sync by allow-list** (`sync/settings.rs`), enforced on the way out *and* the
  way in. `ssh.agentSocket` is machine-specific; a laptop must not overwrite a desktop's
  agent path with one that does not exist there.
- **`SyncStatus.applied` exists so the stores know to reload.** The engine writes SQLite
  underneath Pinia; without it the UI would show stale data until the next navigation.
- **Sign-out keeps local data and re-dirties it**, so signing back in pushes rather than
  loses. A mis-click must not be a way to delete everything.

### Group sharing

A shared group gets a random content key. Everything in that group **and every group
beneath it** is sealed with it instead of the personal key, which is what lets a colleague
read one branch and nothing else. `sync/groups.rs` holds the walk: `key_owner_map` maps
each group to the nearest ancestor that has a key, so the lookup is one hash rather than a
parent chase per record, and both it and `subtree` survive a parent cycle - SQLite will
not stop two individually valid edits making a group its own ancestor.

The key is wrapped to each member's X25519 public key with an anonymous sealed box, so the
server routes a blob it cannot open and two members' wraps of the same key look unrelated.

Four things here are not obvious:

- **Records in a shared group are stored under the group *owner's* account**, whoever
  pushed them. The route resolves that with `share_owner`. Filing a member's edit under
  the member gives one record id two independent rows, and the owner - who pulls by
  account - never sees it. `a_members_edit_lands_on_the_owners_copy` is the test.
- **The owner is not a member of their own share.** A membership check that only matches
  `user_id` locks out the person who did the sharing; `can_write_group` matches either
  side, and allows a group with no shares at all so the first push of a group about to be
  shared does not fail.
- **Revoking rotates the key.** Deleting the share row only stops *new* records reaching
  them - they already hold the old key. `unshare_group` generates a fresh key, re-wraps it
  for whoever remains, and marks the subtree dirty so the next push re-seals it.
  Whatever they already copied is theirs; nothing undoes that, and the toast says so.
- **A tombstone has to keep its group.** The domain row is gone when a deletion is
  collected, so `sync_meta` carries a `group_id` column maintained by the triggers
  (migration 6 recreates them; SQLite cannot alter a trigger in place). A deletion
  labelled personal is filed under the wrong account and the members never see it.

Identities stay personal even inside a shared group: they are account-wide, so a shared
host arrives with its identity link nulled, exactly as a key the far side lacks does.
Group-scoped placeholders *are* sealed with the group key - they travel with the group,
which is the point of declaring one there.

A record sealed under a key this machine does not hold is **skipped**, not failed - a
revoked share or a rotation not yet fetched leaves the local copy alone rather than
replacing it with something unreadable. `fetch_shared_keys` runs at the start of every
cycle, before push, for that reason.

### Workspaces and device layouts

Named workspaces sync as ordinary records - trigger, dirty flag, tombstone, like a host.

The **live** layout does not. `session_state` has no trigger on purpose: it is rewritten
400ms after every layout change, and a trigger would turn that into a push every 400ms.
It is sent instead by `push_layout`, once per cycle and only when its clock has moved past
`sync_state.layout_pushed_at`.

Incoming layouts land in `device_layouts` (migration 7), never in `session_state`:

- **A layout is offered, not applied.** Another machine's tabs replacing what is on screen
  is a decision, not a sync outcome. The Workspaces menu grows an "Other devices" section;
  opening one is a click.
- **Your own layout coming back is ignored.** `apply` skips a `DeviceLayout` whose id is
  this device, because the round trip would otherwise overwrite live tabs with whatever
  was pushed last cycle. `a_devices_own_layout_coming_back_does_not_overwrite_its_tabs`.
- **The record id *is* the device id**, so a machine has exactly one layout and pushing
  again replaces it rather than accumulating one per save.
- Signing out drops every stored layout: they mean nothing without the account.

`sync_state.device_name` comes from the OS at sign-in, via the webview - `@tauri-apps/
plugin-os` can ask and the Rust side has no dependency that can. A session created before
this existed pushes no layout rather than an unnamed one.

### The activity list

`sync_log` (migration 10) holds the last 200 records a cycle sent or received, and
Settings → Sync shows 30 of them. `sync/history.rs` owns it.

- **It is local-only, like `var_values` and `session_state`**: no trigger, never
  collected. A synced history would have every machine replay every other machine's
  activity as its own.
- **The label is copied into the row, not joined out at read time.** Half the entries are
  deletions, and after one there is nothing left to join to - so `apply` reads the name
  *before* `delete_row`, and a deleted host reads as its name rather than a bare uuid. A
  tombstone this device pushed is the one case with no label: the row was gone before the
  cycle began, so the panel shows the kind and a short id.
- **Only records the server accepted, and only records that changed something.** A refused
  push did not sync, and a pull skipped as stale did nothing - listing either as activity
  would be a lie. `a_record_that_was_skipped_is_not_in_the_history`.
- **The layout push is not logged.** It happens every cycle the tabs move, so it would be
  the only thing in the list.
- Sign-out clears it, like the stored device layouts: it names records that came from the
  account, including a colleague's through a shared group.

### Sync tests

`src-tauri/tests/sync_roundtrip.rs` runs `collect` on one in-memory `Db` and `apply` on
another - two devices, no server, no transport. That is where convergence, tombstones,
tie-breaks and AAD tampering are covered. The server's own suite lives in
`remotier-sync/server/tests/store.rs` and goes through the `Store` trait, so pointing it
at Postgres runs the same tests against the other backend.

### Building against the proto crate

`src-tauri` depends on `remotier-sync-proto` as a **git dependency on GitHub**, not from
crates.io. The crate is an implementation detail of two applications and nothing else
should depend on it, so a registry buys nothing and publishing it would invite exactly
that. `Cargo.lock` pins the resolved commit, so builds are reproducible either way.

It is pinned by `rev` rather than `branch`: a lock file would pin a branch to a commit
anyway, but a rev records which commit was *intended* rather than whichever one happened
to be at the head of `main` that day. Move it to `tag` once the sync repo tags releases.

`.cargo/config.toml` at the **repository root** sets `net.git-fetch-with-cli`. It is at the
root and not under `src-tauri/` on purpose: cargo reads that file from the working
directory and its ancestors only, and CI runs `cargo --manifest-path src-tauri/Cargo.toml`
from the root, where a config nested inside `src-tauri` is silently ignored.

That setting is insurance rather than a requirement while the sync repo is public -
libgit2 fetches anonymous HTTPS fine. It matters the moment the repo goes private, when
libgit2 fails with an authentication error naming neither the repository nor the
credential it wanted, and the git CLI just works.

The MSRV moved to **1.85** with that dependency: edition 2024 is in its graph, and an
older Cargo fails while parsing a transitive manifest - which reads as a broken dependency
rather than a stale toolchain.

`reqwest` is pinned to rustls over `ring` and the provider is installed in
`sync/client.rs`; rustls 0.23 has no default provider otherwise and every request fails at
handshake with an error that does not mention providers. `cargo tree | grep aws-lc` must
stay empty.


## Packaging

`src-tauri/tests/bundle_config.rs` guards the release config: versions agreeing across the
three manifests, icons present and in the right binary format, the entitlements file
existing, a non-default identifier, and a CSP that forbids remote scripts. These fail
locally instead of during a release build on a platform nobody is watching.

**A release is built in two halves.** The `release` workflow builds Windows and Linux from
the tag; macOS is built by hand with `bun run build:mac` (`scripts/build-mac.sh`) and
uploaded to the same release. GitHub bills macOS runners at ten times the Linux rate, and
the Developer ID certificate lives in a developer keychain rather than in repository
secrets. The workflow therefore leaves the release a **draft**: published automatically,
a version would go out with no Mac download at all.

The Mac script refuses to build unsigned, produces one universal binary rather than two
per-architecture bundles, verifies what it made - `codesign --verify` always,
`spctl --assess` and `stapler validate` when notarising - and then attaches the DMG to the
release itself. An unverified notarised build fails on the user's machine and nowhere
else. `scripts/check-version.sh` takes the tag as an optional argument, which is how the
workflow fails a mismatched tag in seconds rather than after an hour of build time.

Two rules in that script are load-bearing:

- **It uploads into the release CI made; it never creates one.** `gh release create` here
  would produce a second, Mac-only release under the same tag the moment the workflow
  caught up. A missing draft is reported, not worked around.
- **Publishing stays manual.** The script attaches the asset and prints the
  `--draft=false` command; nothing in the automated path makes a release public.

Credentials come from `.env` at the root (`.env.example` documents the four `APPLE_*`
names), and the script **exits if `.env` is not gitignored**. A warning would be read
after the commit containing an app-specific password already existed. The environment
wins over the file, so one value can be overridden for a single run.

### Brand artwork

The logo is a two-step build, and skipping the first step is how the icon silently ends up
in the wrong typeface:

```bash
bun run scripts/outline-brand-svg.js     # design/assets/*.svg -> outlined artwork
bunx tauri icon design/icon.svg -o src-tauri/icons
```

`design/assets/` holds the designer's files, which set `>_` and REMOTIER as live `<text>`
in Martian Mono. The brand guide calls those layout reference only: nothing that rasterises
them is guaranteed to have the face, and resvg, browsers and installers all fall back to
another monospace without complaining. `outline-brand-svg.js` converts the type to paths
and writes `design/icon.svg`, `public/favicon.svg` and `design/mark.svg` - so edit
`design/assets/`, never the generated files. The wordmark and lockup are skipped on
purpose: they pin the lime cursor at an `x` implying an advance Martian Mono does not have,
so outlining them strands the cursor to the right. Nothing consumes them - the sidebar
wordmark is live text with the font loaded. It also strips the C2PA metadata blob
those exports carry, which is most of their file size.

The script expands the variable woff2 to TTF before instancing the weight axis: fontkit's
`getVariation` returns an object with no usable `cmap` while the tables are still
woff2-compressed, so the 700 weight comes back as 400 or throws.

Regenerate rather than hand-editing the PNGs. ImageMagick renders the SVG badly (it drops
strokes); `tauri icon` uses resvg and gets it right.

**The app icon deliberately departs from `BRAND.md`.** The guide specifies a near-black
tile with a lime glyph; the shipped icon is the inverse - lime tile, black glyph - because
that is what reads on a dock full of dark icons, and it matches the mark in the sidebar.
The user chose this over the guide. Do not "correct" it back.

It is also inset: the tile is 824×824 inside the 1024 canvas, which is the margin macOS
expects. Drawn edge to edge it renders visibly larger than every neighbouring icon, which
is what the full-bleed version looked like.

`BrandMark.vue` drops the underscore because it renders at 16px, where the two shapes stop
reading as one mark. The app icon keeps it.

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

## Where the palette is wired

`src/assets/tokens.css` holds the `--rm-*` design tokens verbatim from `design/tokens.css`.
`src/assets/index.css` is the **only** place they meet shadcn's semantic names - components
keep using `bg-background` / `text-muted-foreground`, so the generated set in
`src/components/ui/` needs no edits and `shadcn-vue add` keeps working.

Three things there are easy to undo by accident:

- **Radii are deliberately stock shadcn.** `--radius: 0.625rem` is untouched, and the
  design's 0/5/10px scale is not applied.
- **Dark only, but `<html class="dark">` stays.** The generated components carry `dark:`
  variants, which key off the class, not the token values. `:root` and `.dark` therefore
  resolve to the same palette so no unprefixed context can fall back to white.
  The flip side: a generated component's `dark:` classes beat a plain override passed in
  `class`. An outline button's selected border needs `dark:border-primary` as well as
  `border-primary`, or `dark:border-input` wins and nothing looks selected.
- **`cn-font-heading` is ours.** The nova preset stamps it on every generated title but
  never defines it; `index.css` does. It is the single point that decides the heading face.

Fonts: Geist carries the interface. The monospaced faces are **accents only** - JetBrains
Mono (`font-mono`) for anything the shell produced (hostnames, paths, fingerprints,
placeholders, the terminal), Martian Mono (`font-display`) for the wordmark. A mono body
font makes ordinary UI text read as terminal output, so `--font-sans` stays proportional.

`vue-sonner` needs its own stylesheet, imported in `index.css`. Without it a toast has no
surface, no position and no stacking - it lands as bare text in the document flow, which is
what "the notification is broken" looks like. `Sonner.vue` only feeds it colour variables
and gives no hint that the stylesheet is missing, and `shadcn-vue add sonner` does not add
it. `ui-conventions.test.ts` fails if the import goes.

`shadcn-vue add` re-injects a Google Fonts `@import url(...)` at the top of `index.css`
every time it runs - confirmed again when `breadcrumb` was added. Delete it: the CSP is `default-src 'self'`, so the request is refused
and the face falls back silently. All three faces come from npm. `ui-conventions.test.ts`
fails if the import comes back.

Motion is enforced globally rather than per component: `--default-transition-duration` and
`--default-transition-timing-function` are set from `--rm-dur-state` / `--rm-ease`, so a
bare `transition` already runs at 120ms on the design's curve.

`lib/terminal-theme.ts` draws the terminal's selection from `--muted-foreground`, not
`--accent`: accent is the hover surface, one step off the canvas, and a translucent
selection built from it is invisible on a near-black terminal.

## App icons

A host or group icon is an opaque key. A built-in one names a Lucide icon in
`lib/appearance.ts`; `selfhst:<reference>` names an app icon from the selfh.st catalog -
a Portainer or Proxmox logo. `EntityIcon.vue` renders either, and is the only thing that
should.

- **Only the key syncs.** Each device fetches the image itself, in
  `src-tauri/src/icons.rs`, and caches it in the app's cache directory. The webview never
  goes online: its CSP allows `data:` images, so Rust hands back a `data:` URL. A build
  that predates the catalog does not know the prefix and shows its default icon.
- **The reference is validated** to `[a-z0-9._-]`, starting with a letter or digit, before
  it goes near a URL or a file name. Every reference in the catalog fits.
- **SVG, then PNG, then WebP.** About one icon in six has no SVG. jsDelivr answers a missing
  file with 403, not 404, so both mean "try the next format".
- **A 200 that is not an image is refused**, or a captive portal's login page would be
  cached as an icon for good.
- **The index is refreshed weekly**, and a stale copy is served when the CDN cannot be
  reached. `Error::Icon` exists because `From<reqwest::Error>` phrases every transport
  failure as the sync server's.
- **Search runs in the webview** (`lib/icon-catalog.ts`) and returns 24 results, after
  typing pauses. Each result is an image fetched on first sight, so a short prefix must not
  become hundreds of downloads. Fetching tells jsDelivr which icons were looked at - that
  was accepted when choosing the catalog.
- A catalog icon shows the built-in default until its image arrives, and when it cannot be
  fetched, so it never renders as an empty box. `stores/icons.ts` remembers a failure for
  the session rather than retrying on every render.
- The image is `draggable="false"`. It sits inside draggable tabs, and an image drags
  itself.

## Picking a group

`GroupPicker.vue` is the one control for choosing a group - the host's group, and a group's
parent. It is a `Popover` over `Command`, so it searches, and it shows each group's colour,
icon, nesting depth and the path above it. The flat `Select` it replaced could not
distinguish two groups called "Staging" under different parents.

- The **full path is the search value**, so typing a parent's name finds everything under
  it. That is the point of showing the nesting at all.
- `exclude` drops a group **and its whole subtree** (`descendantIds`), which is what stops a
  group being reparented inside itself. The old filter excluded only the group itself;
  `buildTree` survives the resulting cycle by dropping the group, so a bad edit made it
  vanish from the list rather than erroring.
- Indentation is capped at four levels. Past that it costs more room than the structure it
  conveys, and the path beside the name says it anyway.
- The picker speaks `string | null`; the editors keep the `INHERIT` sentinel they use
  everywhere else and bridge with a computed.

A tab speaks for everything it holds, never for whichever pane happens to be focused. A
single-pane tab wears its host's icon (`tabHostId`); a split tab shows a split icon
(`tabIsSplit`), because any one host's icon would claim the whole tab is that host. The
active tab is tinted the same way (`tabColors`): one colour solid, several blended left to
right in pane order, lime when no host has one. A host with no colour stays in the blend as
a neutral stop, so a tab that is only partly red does not look entirely red. The blend is an
inline style (`tintGradient`): the stops are data, and Tailwind's gradient utilities are
fixed class names with three stops at most. Inactive tabs are untinted; a single-pane tab's
icon takes its host's colour, a split tab's icon none. Every tab has a border, transparent unless active, so the
active tab is not a pixel larger than its neighbours. The close button is always shown:
hidden until hover, it left an empty slot on every other tab and made them look lopsided.

## The hosts grid is a folder browser

The list view nests, so it shows the whole structure at once. The grid cannot, so it
browses one level at a time: groups appear as cards above the hosts, clicking one opens it,
and a breadcrumb with a home button leads back out. `folder` in `HostsView` holds the open
path as group ids; `lib/tree.ts` does the walking (`nodesAt`, `breadcrumb`, `summarise`).

- **Search scopes to the open folder** and looks all the way down it, reporting hosts flat
  with the path each came from. A match three groups below is still a match; hiding it
  because the user is standing a level up would make search useless. At the root it behaves
  as it always did.
- **A vanished folder falls back to the root.** A group can be deleted while the user is
  inside it, and an unnamed empty level with a breadcrumb that no longer resolves leaves
  them with no way back. `nodesAt` returns `null` for that, which the watcher acts on.
- The breadcrumb shows **whenever the grid is on**, including at the root, where it reads
  "All hosts". Gating it on being inside a group shifts every card down by the bar's height
  on the way in, and takes the way out with it on the way back.
- **Groups and hosts are separate categories**, each under a label and in a grid of its
  own. Sharing one grid let the first host trail on after the last folder, so nothing read
  as where the folders ended. The list does the same at its top level only - there an
  expanded last group ran straight into the ungrouped hosts; deeper down, the group a row
  sits in already labels it.
- New hosts and groups are created **in the open folder**, not at the root.
- The path line on a host card only appears for search results. While browsing, the
  breadcrumb already says where you are.

## The tabs live in the window header

`AppHeader` renders `TabBar` beside the sidebar toggle. That row held nothing else, and a
tab bar beneath it cost every terminal a row of height. The header renders it on **every**
page rather than the terminals view teleporting it in: the row is empty on the other pages
anyway, and a running session is worth reaching from wherever you are. The view toolbars
elsewhere keep their own row under the header.

So the bar has to work from a page where `TerminalsView` does not exist. Clicking a tab, and
`+`, route to `/terminals` themselves, and the quick connect dialog's open flag lives in
`lib/quick-connect.ts` rather than in the view - `+` has to open a dialog that is not
mounted yet, and have it appear once the terminals are back.

The header is the window's drag region, and the tab bar now covers it. The attribute on the
header does not reach an element laid over it, so the bar and its strip carry
`data-tauri-drag-region` too - without it the window cannot be dragged by its title bar at
all. Tabs do not carry it, which keeps dragging a tab from dragging the window. The same
trap sits in the sidebar header, where the mark and the wordmark cover the strip reserved
for the macOS window buttons: they are `pointer-events-none`, so the mousedown lands on the
header that has the attribute. `ui-conventions.test.ts` checks all of it.

## Quick connect only takes over an empty pane

⌘K sets the host on the focused pane when that pane holds nothing - no host and no session.
Anything else opens a tab, whose pane is created with the host and dials on mount.

Pointing a pane that already has a host at another one leaves its session running and its
scrollback on screen, so the terminal goes on showing the host before it. That is what
"the session did not switch" looked like: the tab title changed and nothing else did.

The other half of it is in `TerminalPane`: a pane given a host **while it is mounted** has
to dial from a watcher, because the mount-time connect ran when there was no host to dial.
It fits first, in a frame where the terminal is no longer `display: none` - a pane with no
host reports no size, and connecting straight away opens the PTY at xterm's default 80x24.

## Dragging tabs and panes

Tabs reorder by dragging within the bar, and dropping a tab (or a pane, by its grip in the
top-right corner) on the edge of a pane splits there. `lib/dnd.ts` holds the geometry -
which quarter of a pane the pointer is in, where a dragged tab would land - as plain
arithmetic so it is testable without a layout engine. `lib/drag.ts` holds what is being
dragged, deliberately outside the sessions store, which is watched deeply and written to
disk: a pointer gesture is not session state.

Two details that look like polish but are not:

- **Tabs focus on `click`, never `mousedown`.** Focusing on press puts the tab's own panes
  on screen before its drag starts, so every pane the pointer then crosses belongs to the
  tab being dragged - and a tab cannot be inserted into itself. That made every drop a
  silent no-op in every direction. A drag never produces a `click`, so `click` is safe.
  `canDropOnPane` in `lib/drag.ts` holds the rule and is tested; `ui-conventions.test.ts`
  fails if `@mousedown` returns to the tab bar.
- Dragging a tab onto a pane of that same tab does nothing, by design - there is nothing to
  move. Rearranging inside a tab is the pane grip, or `⌘D` / `⌘⇧D`.
- The pane grip is a `div`, not a `button`: WebKit is unreliable about dragging form
  controls. The pane itself cannot be draggable, because that would take precedence over
  selecting text in the terminal.
- `dragleave` fires whenever the pointer crosses into a **child** element, so both drop
  targets check `relatedTarget` before clearing their highlight. Without it the indicator
  flickers over every tab and over the terminal.
- A centre drop is not a split. Dropping a tab into the middle of a pane just focuses it.
- Each pane carries a chip in its top-right corner - the host's icon and name, the drag
  handle, and a close button. The button sits beside the handle, not inside it: the handle
  is a `role="button"` div, and a control nested in another is invalid.
  Panes are otherwise indistinguishable once a shell has painted over them.
- `dragDropEnabled` is `false` in `tauri.conf.json`. It must stay that way: the webview's
  native file-drop handler otherwise swallows the HTML5 drag events.

A tab's title is **derived**, in `lib/tab-title.ts`, from the hosts its panes are on -
`tab.name` survives only as the fallback for a tab that has connected to nothing yet.
A stored name goes stale the moment tabs merge or a pane is dragged in, leaving the tab
advertising whichever host happened to be the drop target. Repeats are counted rather than
repeated (`terminal.shop ×2`), because two shells on one host is the normal way to work.
Nothing renames a tab on connect for the same reason.

## Knowing a session is dead

A connection that was open when the machine suspended is almost never still there when it
comes back, and nothing involved says so: the socket still looks open to the kernel, and
the peer will not volunteer that it has gone.

`ssh/liveness.rs` and the watchdog in `ssh/session.rs` handle this, entirely in Rust. Every
session gets a task that pings the server and waits for the pong, and reports
`SessionEvent::Lost` when no answer comes. The frontend does not poll for this and has no
timer of its own - it reacts to the event, marks the pane `lost` and offers Reconnect.

Suspend is detected by comparing the two clocks. `Instant` does not advance while the
machine is suspended - `CLOCK_MONOTONIC`, `mach_absolute_time` and the Windows performance
counter all stop with it - while the wall clock does, so the difference between them is the
time spent asleep. That is also why russh's own keepalive is not enough on its own: it
counts in monotonic time, so on wake it restarts its `30s × 3` from the beginning and a
connection that died hours ago looks fine for another two minutes.

Details worth keeping:

- **The ping runs in its own task, not in the pump's `select!` loop.** Waiting up to
  `PING_TIMEOUT` for a reply inside that loop would stall terminal output for every other
  message. The watchdog holds an `Arc<Handle>`; russh's `send_ping` takes `&self`.
- **`Lost` is a separate event from `Failed`.** The user did nothing wrong, so the pane
  offers to reconnect instead of reporting an error, and the message comes from the
  backend, which is the only side that knows whether the machine slept or the server went
  quiet.
- **A backwards clock is not a suspend.** NTP corrections and time zone changes move the
  wall clock without the machine having stopped; treating those as a wake would tear down
  healthy sessions.
- The watchdog also covers a link lost while awake, on its fixed interval, which is why
  detection no longer depends on russh's keepalive at all.

## A shell that exits closes its pane

`exit`, logout or Ctrl-D closes the pane, and the tab once no pane is left. The `closed`
event carries the shell's exit status, and only a status closes anything
(`closeExitedSession`). A lost connection, a channel closed with no status, or a session
ended from this side keeps its pane, because that is where Reconnect is.

- **Any exit status closes, zero or not.** `exit` after a failed command exits non-zero,
  and a pane that stayed open would look as though `exit` did nothing. The cost is that a
  forced command's parting message - a git host's "no shell access" - closes with it.
- **EOF alone does not end the pump.** OpenSSH sends the exit status before EOF; Dropbear
  can send it after. Stopping at EOF lost the status there, so the pane never closed.
- The session is detached before the pane closes, so `closePane` has nothing to
  disconnect: the session is already over.

## Terminals outlive their components

`lib/terminal-registry.ts` owns every xterm, keyed by pane id, along with its container
element and the connection state the user can see. `TerminalPane` borrows an entry while
mounted and **disposes nothing** on unmount.

That is not an oversight. Vue answers a change of position in the layout tree by unmounting
the component and mounting a new one, so a pane that is merely dragged to another split
looks exactly like a pane that was closed. Disposing on unmount would drop the IPC channel
and end the SSH session behind a pane the user only moved - the same failure as unmounting
a hidden tab, reached by a different route.

`TerminalsView` watches the set of live pane ids and calls `releaseMissing`, which is the
only thing that ever disposes a terminal. The layout is the only place that knows the
difference between moved and closed.

Consequences:

- Handlers on the terminal outlive the component that installed them, so they read
  `entry.sessionId` and `entry.paneId` rather than closing over props. A captured
  `props.tabId` would credit output to the wrong tab the first time a pane moved - hence
  `noteOutputFromPane`, which asks which tab holds the pane right now.
- `acquire` re-parents the existing container. `term.open()` is called once per pane, ever;
  calling it again is what xterm does not tolerate.
- A pane that arrives already connected must not dial out again, so the mount-time connect
  is guarded on the status being idle with no session.

## Tabs stay mounted

`TerminalsView` renders every tab and hides the inactive ones with `v-show`. Do not switch
this to `v-if` or key the SplitView on the active tab: unmounting a pane disposes its
xterm, which drops the IPC channel, which ends the SSH session behind it - so switching
tabs would silently disconnect you.

Consequences that have to be kept in mind:

- A hidden tab reports zero size, so `TerminalPane` refits when `tabActive` turns true.
- Only the visible tab may take focus, hence the `tabActive` prop guarding `term.focus()`.
- Output from a hidden tab sets an unread marker, shown as a dot on the tab.
