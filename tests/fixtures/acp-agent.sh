#!/usr/bin/env bash
# A fake ACP agent for tests/acp.rs. It appends each line it receives to <dir>/got, prints
# <dir>/reply.<n> after the n-th line when that file exists, and exits with <code> when
# stdin closes.
dir=$1
n=0
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$dir/got"
  n=$((n + 1))
  if [ -f "$dir/reply.$n" ]; then cat "$dir/reply.$n"; fi
done
exit "${2:-0}"
