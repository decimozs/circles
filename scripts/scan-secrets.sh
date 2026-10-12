#!/usr/bin/env bash
# Looks for secrets in the app's logs and database. Prints what it finds and exits 1 if anything.
# Usage: scripts/scan-secrets.sh [data-dir]   (default: the app data folder)
set -uo pipefail
DATA="${1:-$HOME/Library/Application Support/circles}"
[ -d "$DATA" ] || { echo "No data folder at $DATA"; exit 2; }

PATTERNS='sk-ant-[A-Za-z0-9_-]{10,}|ghp_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|[Bb]earer [A-Za-z0-9._-]{20,}'
found=0
while IFS= read -r -d '' file; do
  hits=$(strings -a "$file" | grep -E -o "$PATTERNS" | sort -u | head -5)
  if [ -n "$hits" ]; then
    echo "SECRET-LIKE TEXT in $file:"
    echo "$hits" | sed -E 's/^(.{8}).*/  \1…/'
    found=1
  fi
done < <(find "$DATA" -type f \( -name '*.log*' -o -name '*.db' -o -name '*.db-wal' -o -name '*.sqlite*' \) -print0)

[ "$found" = 0 ] && echo "No secret-like text found in $DATA."
exit "$found"
