#!/usr/bin/env bash
# The updater's half of the release checks, run before a build that will be published.
#
# Both of these fail late and confusingly otherwise: a missing public key builds fine and
# then tells every installed copy "invalid signature" when it checks, and a missing private
# key stops the bundler after the app has already been compiled.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
conf="$root/src-tauri/tauri.conf.json"

# Read on stdin rather than handing node a path. This runs on the Windows runner too,
# where the shell is Git Bash and `$conf` is an MSYS path (/d/a/...) that a native node
# cannot resolve - `require` then fails with MODULE_NOT_FOUND naming a file that is
# plainly there.
conf_field() {
  node -e 'const conf = JSON.parse(require("fs").readFileSync(0, "utf8"));
           const value = process.argv[1].split(".").reduce((at, key) => at?.[key], conf);
           process.stdout.write(String(value ?? ""))' "$1" < "$conf"
}

pubkey=$(conf_field plugins.updater.pubkey)
endpoint=$(conf_field plugins.updater.endpoints.0)

if [ -z "$pubkey" ]; then
  cat >&2 <<'MSG'
No updater public key in src-tauri/tauri.conf.json.

  bun run tauri signer generate -w .tauri/remotier.key

Put the printed public key in `plugins.updater.pubkey`, and the private key in `.env` as
TAURI_SIGNING_PRIVATE_KEY (with its password in TAURI_SIGNING_PRIVATE_KEY_PASSWORD) and in
the repository secrets of the same names. The key cannot be regenerated later: installed
copies only accept updates signed by the key they shipped with.
MSG
  exit 1
fi

# The manifest has to be reachable at a URL with no version in it, because that is what
# every already-installed copy polls. Renaming it strands them all.
case "$endpoint" in
  */releases/latest/download/latest.json) ;;
  *)
    echo "the updater endpoint is not a static latest.json URL: $endpoint" >&2
    exit 1
    ;;
esac

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "TAURI_SIGNING_PRIVATE_KEY is not set, so the bundles would carry no signature" >&2
  echo "and the updater would refuse them. See docs/RELEASING.md." >&2
  exit 1
fi

# The key lives in the repository, so the only thing keeping it out of a commit is
# .gitignore. Refuse rather than warn, for the same reason `.env` does: a warning is read
# after the commit publishing the key already exists, and this key cannot be rotated -
# every installed copy trusts only the one it shipped with.
if [ -f "$TAURI_SIGNING_PRIVATE_KEY" ] \
   && git -C "$root" ls-files --error-unmatch "$TAURI_SIGNING_PRIVATE_KEY" >/dev/null 2>&1; then
  echo "$TAURI_SIGNING_PRIVATE_KEY is tracked by git. Remove it from the index before" >&2
  echo "building: git rm --cached \"$TAURI_SIGNING_PRIVATE_KEY\"" >&2
  exit 1
fi

if [ -f "$TAURI_SIGNING_PRIVATE_KEY" ] \
   && ! git -C "$root" check-ignore -q "$TAURI_SIGNING_PRIVATE_KEY" 2>/dev/null; then
  echo "$TAURI_SIGNING_PRIVATE_KEY is not ignored by git - add it to .gitignore" >&2
  exit 1
fi

echo "updater: signing key present, manifest at $endpoint"
