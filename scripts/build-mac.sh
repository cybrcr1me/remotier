#!/usr/bin/env bash
# A signed - and, if the notarisation credentials are present, notarised - macOS build.
#
# This one is run by hand. CI builds Windows and Linux; GitHub bills macOS runners at ten
# times the Linux rate, and the signing certificate lives in a developer keychain rather
# than in repository secrets.
#
# Credentials come from `.env` at the repository root - see `.env.example` - or from the
# environment, which wins over the file so a one-off build can override one value.
#
# Required:
#   APPLE_SIGNING_IDENTITY  "Developer ID Application: Name (TEAMID)" - `bun run \
#                           build:mac --list-identities` prints what this machine has.
#
# For notarisation (optional here, but a build without it still shows Gatekeeper's
# "cannot be opened because the developer cannot be verified"):
#   APPLE_ID                the Apple ID the certificate belongs to
#   APPLE_PASSWORD          an app-specific password, not the account password
#   APPLE_TEAM_ID           the 10-character team identifier
#
# Flags:
#   --list-identities  print the signing identities in this keychain and stop
#   --no-upload        build and verify, but do not attach the DMG to the release
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [ -f "$root/.env" ]; then
  # An app-specific password in a tracked file is a password published to whoever can
  # read the repository. Refuse rather than warn: by the time anyone reads a warning the
  # commit exists.
  if git -C "$root" check-ignore -q .env 2>/dev/null; then
    : # ignored, as it should be
  else
    echo ".env is not ignored by git - add it to .gitignore before putting a password" \
         "in it." >&2
    exit 1
  fi

  # Anything already in the environment wins, so an explicit
  # `APPLE_SIGNING_IDENTITY=... bun run build:mac` overrides the file rather than being
  # silently replaced by it. Held one by one rather than by restoring a dump of the whole
  # environment, which trips over any readonly variable in it.
  for var in APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
    eval "held_$var=\${$var:-}"
  done

  # `set -a` exports what the file defines; `set +a` puts that back.
  set -a
  # shellcheck disable=SC1091
  . "$root/.env"
  set +a

  for var in APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
    held="held_$var"
    if [ -n "${!held}" ]; then
      export "$var=${!held}"
    fi
  done
fi

upload=1
for arg in "$@"; do
  case "$arg" in
    --list-identities)
      security find-identity -v -p codesigning
      exit 0
      ;;
    --no-upload) upload=0 ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo "this builds a macOS bundle and has to run on macOS" >&2
  exit 1
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  cat >&2 <<'MSG'
APPLE_SIGNING_IDENTITY is not set, so the bundle would be unsigned.

  export APPLE_SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)"

`scripts/build-mac.sh --list-identities` lists what this keychain holds. An unsigned
release build also cannot keep its keychain vault entry across rebuilds - the ACL is
bound to the binary that created it. See docs/RELEASING.md.
MSG
  exit 1
fi

scripts/check-version.sh

# Universal, so one download covers both architectures. Building the two separately is
# what the old CI matrix did, and it doubled the assets for no one's benefit.
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

notarise=1
for var in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  if [ -z "${!var:-}" ]; then
    notarise=0
  fi
done
if [ "$notarise" = 0 ]; then
  echo "note: APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID not all set - signing without" \
       "notarising, so Gatekeeper will still warn on another machine." >&2
fi

# The DMG step drives Finder over AppleScript to lay the window out, and fails with
# "Not authorised to send Apple events to Finder. (-1743)" until this terminal is granted
# System Settings -> Privacy & Security -> Automation -> Finder.
bun run tauri build --target universal-apple-darwin --bundles app,dmg

version=$(node -p "require('$root/package.json').version")
out="$root/src-tauri/target/universal-apple-darwin/release/bundle"
app="$out/macos/Remotier.app"
dmg="$out/dmg/Remotier_${version}_universal.dmg"

echo
echo "Verifying the signature"
codesign --verify --deep --strict --verbose=2 "$app"

if [ "$notarise" = 1 ]; then
  # Only meaningful once the ticket is stapled; a notarised build that fails here would
  # warn on the user's machine and nowhere else, which is the worst place to find out.
  spctl --assess --type execute --verbose=2 "$app"
  xcrun stapler validate "$dmg"
fi

echo
echo "Built ${version}:"
echo "  $app"
echo "  $dmg"

tag="v${version}"

if [ "$upload" = 0 ]; then
  echo
  echo "Not uploading (--no-upload). To attach it later:"
  echo "  gh release upload $tag \"$dmg\""
  exit 0
fi

if ! command -v gh >/dev/null; then
  echo
  echo "gh is not installed, so the DMG was not uploaded. Install it (brew install gh)" >&2
  echo "or attach the file by hand:" >&2
  echo "  gh release upload $tag \"$dmg\"" >&2
  exit 1
fi

# The release is created by the tag's CI run, which builds Windows and Linux. Uploading
# into a release this script created instead would produce a second, Mac-only release
# under the same tag once CI caught up.
if ! gh release view "$tag" >/dev/null 2>&1; then
  echo
  echo "No release exists for $tag yet. It is created by the release workflow the tag" >&2
  echo "starts; wait for that to finish, then re-run with --no-upload skipped, or:" >&2
  echo "  gh release upload $tag \"$dmg\"" >&2
  exit 1
fi

echo
echo "Uploading to $tag"
# --clobber so a rebuild replaces the asset rather than failing on the name.
gh release upload "$tag" "$dmg" --clobber

echo
echo "Attached. The release is still a draft - publish it when the assets look right:"
echo "  gh release view $tag --web"
echo "  gh release edit $tag --draft=false"
