#!/usr/bin/env bash
# .claude/commands/_audit-validate.sh
#
# Validates file/dir path references in `.claude/commands/audit-*/SKILL.md`
# and `.claude/commands/_audit-*.md` skill files against the live repo tree.
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
# Audit skills now live in per-command subdirectories as
# `.claude/commands/<name>/SKILL.md`; the two shared `_audit-*.md`
# protocol files stay flat at the top level. Glob both shapes so the
# gate actually inspects every skill (the old flat `audit-*.md` glob
# silently matched zero files after the subdir migration).
skill_files=(
    .claude/commands/audit-*/SKILL.md
    .claude/commands/_audit-*.md
)
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

# ---------------------------------------------------------------------------
# Crate-count drift (FATAL)
#
# `_audit-common.md` documents the crate roster and tells audits to use it as a
# coverage sanity check, so a stale count silently understates required
# coverage. It went stale on two consecutive crate additions — #2261 (`hkx`)
# and #2420 (`mod-runtime`) — because the number was fixed by hand each time
# and hand-fixing does not survive the next `crates/` addition.
#
# The count is mechanically derivable, so derive it. Pointer sentences in other
# skills deliberately no longer quote a number at all (#2420); this guards the
# one remaining literal, in the file that owns it.
# ---------------------------------------------------------------------------
crate_dirs=$(find crates -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')
common_md=.claude/commands/_audit-common.md
documented=$(grep -oE '^Crate count: [0-9]+' "$common_md" 2>/dev/null | grep -oE '[0-9]+')
if [[ -z "$documented" ]]; then
    echo
    echo "STALE  _audit-common.md — no parseable 'Crate count: N' line"
    stale_count=$((stale_count + 1))
elif [[ "$documented" != "$crate_dirs" ]]; then
    echo
    echo "STALE  _audit-common.md 'Crate count: $documented' — live tree has $crate_dirs"
    echo "       Update the count AND the name list beside it."
    stale_count=$((stale_count + 1))
fi

# ---------------------------------------------------------------------------
# Symbol drift (ADVISORY — reports, never fails the gate)
#
# Paths were only half the recurring staleness. Renamed *symbols* rot the same
# way and the path gate structurally cannot see them: `merge_bgsm_into_mesh`
# and `pack_bgsm_material_flags` survived in four skills after the 2026-07-27
# material refactor, and `gpu_material_size_is_300_bytes` outlived a 300→348 B
# GpuMaterial change — a wrong number in a GPU layout contract.
#
# Heuristic: every backticked snake_case token >=7 chars that appears in NO
# tracked .rs file. Advisory only, because the noise floor is real and mostly
# legitimate: baseline TSV column names, git hashes, memory-file slugs, rustc
# lint names, and deliberate references to symbols that SHOULD NOT exist.
# Filters below remove the known-benign classes; what survives is worth a look,
# not an automatic failure. Historical names should be *italicised*, not
# backticked (same rule as backwards-looking paths).
# ---------------------------------------------------------------------------
if [[ "${SKIP_SYMBOL_CHECK:-0}" != "1" ]]; then
    src_blob=$(mktemp)
    trap 'rm -f "$all_paths_file" "$src_blob"' EXIT
    git ls-files '*.rs' | xargs cat 2>/dev/null > "$src_blob" || true

    suspect_count=0
    while read -r sym; do
        # Benign classes, in order of frequency:
        [[ "$sym" =~ ^[0-9a-f]{7,8}$ ]] && continue          # git short hashes
        [[ "$sym" == feedback_* ]] && continue               # ~/.claude memory slugs
        [[ "$sym" == nif_v10x_* ]] && continue               # memory slugs
        [[ "$sym" == bench_* || "$sym" == light_count_* ]] && continue   # baseline TSV columns
        [[ "$sym" == tex_missing_* || "$sym" == mesh_cache_* ]] && continue
        [[ "$sym" == entities_total || "$sym" == tlas_instances ]] && continue
        [[ "$sym" == static_frames || "$sym" == terrain_tile ]] && continue
        [[ "$sym" == unknown_records || "$sym" == marker_arrow ]] && continue
        [[ "$sym" == max_size || "$sym" == local_size ]] && continue     # GLSL / generic
        [[ "$sym" == unreachable_patterns ]] && continue     # rustc lint name
        [[ "$sym" == cmd_reset_query_pool ]] && continue     # ash API
        [[ "$sym" == srgb_to_linear ]] && continue           # deliberately-absent (see memory)
        [[ "$sym" == comprehensive ]] && continue            # plain English

        grep -qw "$sym" "$src_blob" && continue
        if (( suspect_count == 0 )); then
            echo
            echo "ADVISORY — backticked symbols not found in any tracked .rs:"
        fi
        printf '  %-46s %s\n' "$sym" "$(grep -rl "\`$sym\`" "${skill_files[@]}" 2>/dev/null | sed 's|.claude/commands/||;s|/SKILL.md||' | tr '\n' ' ')"
        suspect_count=$((suspect_count + 1))
    done < <(grep -rhoE '`[a-z][a-z0-9_]{6,}`' "${skill_files[@]}" 2>/dev/null | tr -d '`' | sort -u)

    if (( suspect_count > 0 )); then
        echo
        echo "  $suspect_count advisory symbol(s). Each is either (a) genuinely renamed —"
        echo "  update the skill, or (b) an intentional historical/never-should-exist"
        echo "  reference — italicise it instead of backticking. Not a failure."
        echo "  Set SKIP_SYMBOL_CHECK=1 to silence."
    fi
fi

if (( stale_count > 0 )); then
    echo
    echo "FAIL: $stale_count stale path reference(s)."
    echo "Fix: update the audit skill files, OR delete the stale ref if the target moved."
    exit 1
fi
echo "OK: all path references valid."
