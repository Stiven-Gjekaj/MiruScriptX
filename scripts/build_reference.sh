#!/usr/bin/env bash
# Regenerate docs/language-reference.md by concatenating the wiki lessons into a
# single page. Do not edit the generated file by hand: edit the files in wiki/
# and re-run this script.

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/docs/language-reference.md"

# A lesson's heading, and the anchor that heading lands on.
title_of() { head -n 1 "$1" | sed 's/^#\{1,6\} *//'; }
anchor_of() {
  title_of "$1" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-'
}

# A link from one lesson to another is right in wiki/, where each lesson is its
# own file, and dead here, where they are all one page. Rewrite each into the
# anchor of the section it now points into.
relink=""
for file in "$root"/wiki/*.md; do
  name="$(basename "$file" | sed 's/\./\\./g')"
  relink="${relink}s|]($name)|](#$(anchor_of "$file"))|g;"
done

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
    echo "- [$(title_of "$file")](#$(anchor_of "$file"))"
  done
  echo
  for file in "$root"/wiki/*.md; do
    # Copy each lesson, dropping its trailing Previous/Next navigation footer
    # (everything from the first horizontal rule onward).
    awk 'BEGIN { in_footer = 0 } /^---$/ { in_footer = 1 } in_footer == 0 { print }' "$file" |
      sed "$relink"
    echo
  done
} > "$out"

echo "Wrote $out"
