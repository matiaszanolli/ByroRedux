#!/bin/sh
# dump.sh <bsa> <outdir> : extract every menus\ file via bsa_extract_one
BSA="$1"; OUT="$2"
/mnt/data/src/gamebyro-redux/target/debug/examples/archive_list_all "$BSA" 'menus\' | while IFS= read -r f; do
  case "$f" in menus\\*) ;; *) continue;; esac
  o="$OUT/$(printf "%s" "$f" | tr "\\\\" "/")"; mkdir -p "$(dirname "$o")"
  /mnt/data/src/gamebyro-redux/target/debug/examples/bsa_extract_one "$BSA" "$f" "$o" >/dev/null
done
