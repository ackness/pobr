#!/usr/bin/env bash
# Detects a release PR (the workspace version differs from the base revision)
# and validates its release metadata. Prints `release=true|false`; when
# GITHUB_OUTPUT is set, also writes it there.
#
# Usage: devs/scripts/release-check.sh <base-rev>
#
# A release PR must not reuse an existing tag, and Cargo.lock must already
# carry the new version for the application packages.
set -euo pipefail

base="${1:?usage: release-check.sh <base-rev>}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

workspace_version() {
  awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f&&/^version/{gsub(/[" ]/,"");split($0,a,"=");print a[2]}'
}

emit() {
  echo "release=$1"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then echo "release=$1" >> "$GITHUB_OUTPUT"; fi
}

new="$(workspace_version < Cargo.toml)"
old="$(git show "$base:Cargo.toml" | workspace_version)"
if [ -z "$new" ] || [ -z "$old" ]; then
  echo "error: could not read [workspace.package].version (base='$old', head='$new')" >&2
  exit 1
fi

if [ "$new" = "$old" ]; then
  echo "Workspace version unchanged ($new): not a release PR."
  emit false
  exit 0
fi
echo "Release PR: $old -> $new"

failed=0
if git ls-remote --exit-code --tags origin "refs/tags/v$new" > /dev/null 2>&1; then
  echo "error: tag v$new already exists; choose a new version" >&2
  failed=1
fi

# Application and tool packages inherit the workspace version; the lockfile
# must be refreshed with it (`cargo metadata --format-version 1 > /dev/null`).
locked="$(awk '/^name = "pobr-cli"$/{getline; gsub(/[" ]/,""); split($0,a,"="); print a[2]}' Cargo.lock)"
if [ "$locked" != "$new" ]; then
  echo "error: Cargo.lock has pobr-cli $locked, expected $new; refresh the lockfile" >&2
  failed=1
fi

if [ "$failed" -ne 0 ]; then exit 1; fi
emit true
