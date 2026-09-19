#!/usr/bin/env bash
# .claude/commands/_audit-route.sh
#
# Route changed paths to their owning audits using `_audit-owners.md`
# (first matching row wins). Reads paths on stdin, one per line; prints
# `path<TAB>risk<TAB>owners`, or `path<TAB>-<TAB>(unrouted)` when no row matches
# (an unrouted path means the ownership map needs a row).
#
#   git diff --name-only HEAD~10..HEAD | .claude/commands/_audit-route.sh
#   git diff --name-only HEAD~10..HEAD | .claude/commands/_audit-route.sh | sort -t$'\t' -k2,2r

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

map=.claude/commands/_audit-owners.md
[[ -f "$map" ]] || { echo "missing $map" >&2; exit 1; }

# Expand one `prefix{a,b}suffix` brace pair (same convention as _audit-validate.sh).
expand_braces() {
    local path="$1"
    if [[ "$path" == *"{"*"}"* ]]; then
        local prefix="${path%%\{*}" rest="${path#*\{}"
        local inner="${rest%%\}*}" suffix="${rest#*\}}" part
        local IFS=','
        for part in $inner; do printf '%s\n' "${prefix}${part}${suffix}"; done
    else
        printf '%s\n' "$path"
    fi
}

prefixes=(); owners=(); risks=()
while IFS=$'\t' read -r path owner risk; do
    while IFS= read -r p; do
        prefixes+=("$p"); owners+=("$owner"); risks+=("$risk")
    done < <(expand_braces "$path")
done < <(sed -nE 's/^\| `([^`]+)` \| ([^|]+) \| ([A-Z]+) \|.*/\1\t\2\t\3/p' "$map")

while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    routed=0
    for i in "${!prefixes[@]}"; do
        if [[ "$f" == "${prefixes[$i]}"* ]]; then
            printf '%s\t%s\t%s\n' "$f" "${risks[$i]}" "${owners[$i]}"
            routed=1
            break
        fi
    done
    (( routed )) || printf '%s\t-\t(unrouted)\n' "$f"
done
