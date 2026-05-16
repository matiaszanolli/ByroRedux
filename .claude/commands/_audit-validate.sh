#!/usr/bin/env bash
# .claude/commands/_audit-validate.sh
#
# Validates file/dir path references in `.claude/commands/audit-*.md` and
# `_audit-*.md` skill files against the live repository tree.
#
# Why: TD7-* "stale path" findings keep recurring after module splits.
# A one-shot sed sweep is reactive; this gate catches drift on the
# commit that introduces it. See #1114 / TD7-050.
#
# What it checks:
#   - Every backticked path token ending in a known source/doc extension
#     (.rs .md .toml .comp .frag .vert .glsl .wgsl .sh .xml) is resolved
#     against the repo root. Missing paths print STALE and exit 1.
#   - Brace-expanded refs like `legacy/{tes3,tes4,tes5}.rs` expand to
#     N paths and each is checked.
#   - Trailing `:NN` or `:NN-NN` line ranges are stripped before
#     existence check (line numbers may drift; the file must still
#     exist).
#
# What it skips (not real repo paths):
#   - /tmp/...                — runtime audit scratch
#   - feedback_*.md           — user-global memory (~/.claude/)
#   - *.bsa / *.esm / *.ba2 / *.nif — game data
#   - URLs (contain ://)
#
# Usage:
#   .claude/commands/_audit-validate.sh           # validate, exit 1 on stale
#   .claude/commands/_audit-validate.sh --verbose # list every ref checked

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

VERBOSE=0
[[ "${1:-}" == "--verbose" ]] && VERBOSE=1

should_skip() {
    local p="$1"
    # Bare basenames (`lib.rs`, `systems.rs`, `tests.rs`) are used as
    # shorthand inside a paragraph that already established the dir
    # context. They carry no path info to begin with, so they can't
    # go stale in the "wrong dir" sense this gate targets.
    [[ "$p" != */* ]] && return 0
    [[ "$p" == /tmp/* ]] && return 0
    [[ "$p" == feedback_*.md ]] && return 0
    [[ "$p" == *.bsa || "$p" == *.esm || "$p" == *.ba2 || "$p" == *.nif ]] && return 0
    [[ "$p" == *"://"* ]] && return 0
    return 1
}

# Expand `prefix{a,b,c}suffix` into prefix-a-suffix, prefix-b-suffix, prefix-c-suffix.
# Supports one brace pair only (which covers every observed audit-skill case).
expand_braces() {
    local path="$1"
    if [[ "$path" == *"{"*"}"* ]]; then
        local prefix="${path%%\{*}"
        local rest="${path#*\{}"
        local inner="${rest%%\}*}"
        local suffix="${rest#*\}}"
        local IFS=','
        for part in $inner; do
            printf '%s\n' "${prefix}${part}${suffix}"
        done
    else
        printf '%s\n' "$path"
    fi
}

stale_count=0
checked_count=0
shopt -s nullglob
skill_files=(.claude/commands/audit-*.md .claude/commands/_audit-*.md)
shopt -u nullglob

# Enumerate every checkable repo path once so partial refs like
# `cell/mod.rs` (shorthand for `crates/plugin/src/esm/cell/mod.rs`)
# resolve via path-suffix match. Excludes target/ and node_modules/
# to keep the list tight.
all_paths_file=$(mktemp)
trap 'rm -f "$all_paths_file"' EXIT
git ls-files > "$all_paths_file"

# True iff `p` matches any tracked path or path-suffix.
path_exists() {
    local p="$1"
    [[ -e "$p" ]] && return 0
    # Path-suffix match: any tracked path ending with `/$p`.
    grep -qE "(^|/)${p//./\\.}\$" "$all_paths_file"
}

for skill in "${skill_files[@]}"; do
    [[ -f "$skill" ]] || continue
    # Extract backticked tokens that look like file paths. The trailing
    # bracket-set must match a known source/doc extension to keep noise low.
    while IFS=: read -r line_num token; do
        # Strip leading backtick from grep match.
        token="${token#\`}"
        # Strip trailing `:NN` or `:NN-NN` line range.
        local_path="${token%:[0-9]*}"
        while read -r p; do
            should_skip "$p" && continue
            checked_count=$((checked_count + 1))
            if ! path_exists "$p"; then
                echo "STALE: $skill:$line_num — \`$p\`"
                stale_count=$((stale_count + 1))
            elif [[ "$VERBOSE" == "1" ]]; then
                echo "ok: $skill:$line_num — $p"
            fi
        done < <(expand_braces "$local_path")
    done < <(grep -noE '`[A-Za-z0-9_./{},-]+\.(rs|md|toml|comp|frag|vert|glsl|wgsl|sh|xml)' "$skill" || true)
done

echo
echo "Checked $checked_count refs across ${#skill_files[@]} skill files."
if (( stale_count > 0 )); then
    echo "FAIL: $stale_count stale path reference(s)."
    echo "Fix: update the audit skill files, OR delete the stale ref if the target moved."
    exit 1
fi
echo "OK: all path references valid."
