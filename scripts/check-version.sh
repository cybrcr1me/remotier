#!/usr/bin/env bash
# Verifies package.json, Cargo.toml and tauri.conf.json agree on the version.
# Run before tagging a release; the release workflow builds from the tag.
#
# With an argument (a tag such as v0.1.0) the tag has to agree with them too. A tag that
# does not is worse than a failed build: it produces installers whose filenames and
# in-app version disagree with the release they are attached to.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

pkg=$(node -p "require('$root/package.json').version")
conf=$(node -p "require('$root/src-tauri/tauri.conf.json').version")
cargo=$(grep -m1 '^version = ' "$root/src-tauri/Cargo.toml" | cut -d'"' -f2)

printf 'package.json      %s\n' "$pkg"
printf 'tauri.conf.json   %s\n' "$conf"
printf 'Cargo.toml        %s\n' "$cargo"

if [ "$pkg" != "$conf" ] || [ "$pkg" != "$cargo" ]; then
  echo "versions disagree" >&2
  exit 1
fi

# The `v` belongs to the tag and nowhere else. Caught here because the comparison below
# strips it from the tag, so a manifest carrying one reports "the tag does not match"
# with both sides printing the same string - which reads as the check being broken.
# Cargo and Tauri both refuse a non-semver version anyway, several minutes later.
case "$pkg" in
  v*)
    echo "the manifests say '$pkg': drop the leading v, the tag carries it" >&2
    exit 1
    ;;
esac

if [ $# -gt 0 ]; then
  tag="$1"
  printf 'tag               %s\n' "$tag"
  if [ "${tag#v}" != "$pkg" ]; then
    echo "the tag does not match the version in the manifests" >&2
    exit 1
  fi
fi

echo "versions agree: $pkg"
