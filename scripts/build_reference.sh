#!/usr/bin/env bash
# Regenerate docs/language-reference.md by concatenating the wiki lessons into a
# single page. Do not edit the generated file by hand: edit the files in wiki/
# and re-run this script.

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/docs/language-reference.md"

{
  echo "# MiruScriptX Language Reference"
  echo
  echo "This single page is generated from the wiki/ learning stages by"
  echo "scripts/build_reference.sh. It gathers every lesson in one place so you"
  echo "can search the whole language at once. To change it, edit the files in"
  echo "wiki/ and re-run the script."
  echo
  echo "## Contents"
  echo
  for file in "$root"/wiki/*.md; do
    title="$(head -n 1 "$file" | sed 's/^#\{1,6\} *//')"
    anchor="$(printf '%s' "$title" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-')"
    echo "- [$title](#$anchor)"
  done
  echo
  for file in "$root"/wiki/*.md; do
    # Copy each lesson, dropping its trailing Previous/Next navigation footer
    # (everything from the first horizontal rule onward).
    awk 'BEGIN { in_footer = 0 } /^---$/ { in_footer = 1 } in_footer == 0 { print }' "$file"
    echo
  done
} > "$out"

echo "Wrote $out"
