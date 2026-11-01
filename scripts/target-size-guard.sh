#!/usr/bin/env bash
# Fail when the workspace build directory has grown past a budget.
#
# `target/` reached ~390 GB on disk before it was noticed (534 GiB by cargo's own
# count). Nothing in the build warns about that, so this is the warning: run it
# before a long session, or from a hook, and it names the biggest consumers and
# the cheapest ways to give the space back.
#
# Usage: scripts/target-size-guard.sh            # default budget
#        BYRO_TARGET_LIMIT_GB=40 scripts/target-size-guard.sh
# Exit: 0 within budget (or no target/), 1 over budget.
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target="${CARGO_TARGET_DIR:-${repo_root}/target}"
limit_gb="${BYRO_TARGET_LIMIT_GB:-100}"

[[ -d "${target}" ]] || { echo "target-size-guard: no ${target}; nothing to check"; exit 0; }

used_kb="$(du -sk "${target}" | cut -f1)"
used_gb=$(( (used_kb + 1048575) / 1048576 ))
echo "target-size-guard: ${target} is ${used_gb} GiB (budget ${limit_gb} GiB)"
if (( used_gb <= limit_gb )); then
    exit 0
fi

echo "OVER BUDGET. Largest entries:"
du -sk "${target}"/*/* 2>/dev/null | sort -rn | head -8 | while read -r kb path; do
    printf '  %6d GiB  %s\n' $(( (kb + 1048575) / 1048576 )) "${path#"${repo_root}/"}"
done
cat <<'MSG'
Cheapest first:
  rm -rf target/debug/incremental            # rebuilt on demand, often the biggest
  cargo clean -p <crate>                     # one crate's artifacts
  cargo clean --profile release              # keep the debug build
  cargo clean                                # everything
MSG
exit 1
