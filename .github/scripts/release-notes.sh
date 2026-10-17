#!/usr/bin/env bash
# Print the CHANGELOG.md section for a release tag, for use as the GitHub
# release body. Usage: release-notes.sh v1.2.3 [path/to/CHANGELOG.md]
#
# The section starts at the "## [1.2.3]" heading and ends before the next
# "## " heading. A tag with no section, or an empty one, exits 1 with a
# message, so a release cannot go out with no notes.
set -euo pipefail

tag="${1:?usage: release-notes.sh <tag> [changelog]}"
changelog="${2:-CHANGELOG.md}"
version="${tag#v}"

notes="$(awk -v v="$version" '
  /^## \[/ { if (found) exit; if ($2 == "[" v "]") { found = 1; next } }
  /^\[[^]]+\]: / { if (found) exit }
  found { print }
' "$changelog" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}')"

if [ -z "$notes" ]; then
  echo "CHANGELOG.md has no section for $version: add '## [$version] - YYYY-MM-DD' with the changes before tagging" >&2
  exit 1
fi
printf '%s\n' "$notes"
