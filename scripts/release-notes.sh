#!/usr/bin/env bash
# Writes one version's entry from CHANGELOG.md and CHANGELOG.en.md to notes.md
# and notes.en.md, and fails if either changelog has none. The release workflow
# checks with it first and publishes the notes with it last.
#
# Usage: scripts/release-notes.sh 1.4.1
set -euo pipefail

version="$1"
for lang in "" ".en"; do
  awk -v v="$version" '
    /^## / { on = (index($0, "## " v " ") == 1 || $0 == "## " v); next }
    on { print }
  ' "CHANGELOG$lang.md" > "notes$lang.md"
  if ! grep -q '[^[:space:]]' "notes$lang.md"; then
    echo "CHANGELOG$lang.md has no entry for $version" >&2
    exit 1
  fi
done
