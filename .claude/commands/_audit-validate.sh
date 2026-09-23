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
    # context, so they can't go stale in the "wrong dir" sense this gate
    # targets.
    #
    # #3439 — but that is only half the claim they make. A bare basename
    # carries no *directory* information while still asserting
    # *existence*, and the second half is exactly the rot that reached
    # `docs/engine/` when #3202 extended the glob there: a file that was
    # deleted rather than moved. So skip a bare basename only when it
    # resolves somewhere in the tree (genuine shorthand); report it when
    # it resolves nowhere.
    if [[ "$p" != */* ]]; then
        # Resolves somewhere in the tree -> genuine shorthand, skip.
        # Otherwise fall through to the rules below: `feedback_*.md` and
        # the archive/asset extensions are bare basenames too, and each
        # has its own reason to be skipped that outlives this one.
        path_exists "$p" && return 0
    fi
    [[ "$p" == /tmp/* ]] && return 0
    [[ "$p" == feedback_*.md ]] && return 0
    [[ "$p" == *.bsa || "$p" == *.esm || "$p" == *.ba2 || "$p" == *.nif ]] && return 0
    [[ "$p" == *"://"* ]] && return 0
    # #3202 — two artifact classes the extractor produces on its own, both
    # surfaced by extending the glob to `docs/engine/`. Neither is a path
    # reference that can go stale, so neither should be reported as one:
    #
    #   (a) Deliberate prose elision — `crates/plugin/.../actor_value_derive.rs`,
    #       `byroredux/src/systems/{follow,escort,...}.rs`. The `...` says "and
    #       the rest"; there is no path here to resolve.
    #   (b) A brace span the one-pair `expand_braces` could not close, leaving
    #       a stray `{` or `}` in the result: multi-line spans
    #       (`crates/spt/src/{tag.rs, stream.rs,` wraps mid-list) and nested
    #       pairs (`byroredux/src/{fog,render/{fog_volumes,lights}}.rs`).
    #       Reporting `byroredux/src/fog}.rs` as STALE says nothing about the
    #       doc — it is the extractor failing to parse, and the enclosing
    #       real paths are checked by the other expansions anyway.
    [[ "$p" == *...* ]] && return 0
    [[ "$p" == *"{"* || "$p" == *"}"* ]] && return 0
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
#
# #3202 — `docs/engine/*.md` joins them. `_audit-common.md` lists eighteen
# of those files as "the authoritative, code-verified reference for their
# domain" and tells every audit to prefer them over re-deriving facts from
# source, yet they were checked by neither half of this gate. The existing
# logic already worked there; only the glob kept it blind — extending it
# would have caught the `GpuCamera` 336 -> 352 B doc drift on day one
# instead of four days and one audit sweep later. Reference docs are what
# audits are told to believe, so they get the same policing as the skills.
command_files=(
    .claude/commands/audit-*/SKILL.md
    .claude/commands/_audit-*.md
)
# #4771 — the audit-baseline READMEs are audit infrastructure too: the
# runtime one cited the pre-split `audit-runtime.md` skill path for weeks
# with this gate green, because no glob reached it.
reference_docs=(
    docs/engine/*.md
    .claude/audit-baselines/*/README.md
)
skill_files=("${command_files[@]}" "${reference_docs[@]}")
shopt -u nullglob

# Enumerate every checkable repo path once so partial refs like
# `cell/mod.rs` (shorthand for `crates/plugin/src/esm/cell/mod.rs`)
# resolve via path-suffix match. Excludes target/ and node_modules/
# to keep the list tight.
missing_basenames=()
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

# #4366 — can a repo-rooted token lifted out of a fenced block be checked as
# a single path? Templates, globs, truncated brace lists and filename stems
# name no one file, so reporting them STALE would say nothing about the doc.
fenced_token_is_checkable() {
    local t="$1"
    [[ "$t" == *"<"* || "$t" == *">"* || "$t" == *"*"* ]] && return 1
    local opens="${t//[^\{]/}" closes="${t//[^\}]/}"
    (( ${#opens} != ${#closes} )) && return 1
    [[ "$t" == *_ || "$t" == *- ]] && return 1
    return 0
}

# ---------------------------------------------------------------------------
# `--selftest` — regression coverage for the skip rules (#3439).
#
# The gate has no unit-test harness and its own output cannot distinguish
# "this rule works" from "nothing happened to exercise it", so the two
# behaviours #3439 turns on are asserted directly against the live tree.
# Runs and exits before the repo scan.
# ---------------------------------------------------------------------------
if [[ "${1:-}" == "--selftest" ]]; then
    selftest_failures=0
    expect() {
        local want="$1" desc="$2" p="$3"
        if should_skip "$p"; then got="skip"; else got="report"; fi
        if [[ "$got" != "$want" ]]; then
            echo "SELFTEST FAIL: \`$p\` — expected $want, got $got ($desc)"
            selftest_failures=$((selftest_failures + 1))
        else
            echo "ok: \`$p\` -> $got ($desc)"
        fi
    }

    # A bare basename that resolves is genuine shorthand — still skipped.
    expect skip "resolving bare basename stays shorthand" "lib.rs"
    expect skip "resolving bare basename stays shorthand" "mod.rs"
    # The #3439 case: a bare basename that resolves nowhere asserts the
    # existence of a file that is not there. Must NOT be skipped.
    expect report "deleted bare basename must reach the checker" "ai.rs"
    expect report "deleted bare basename must reach the checker" \
        "definitely_not_a_real_file_9f3a.rs"
    # Regression on the fix itself: making the bare-basename rule return
    # early instead of falling through stole the later rules from every
    # bare basename they covered. `feedback_*.md` is the one with teeth —
    # none of those files are tracked, so a short-circuit turns all of
    # them into advisories.
    expect skip "feedback_*.md keeps its own skip rule" "feedback_no_guessing.md"
    expect skip "asset extensions keep their own skip rule" "Skyrim.esm"
    # Unchanged rules, pinned so a future edit to the block notices.
    expect skip "brace-expansion artifacts still skipped" "byroredux/src/fog}.rs"
    expect skip "prose elision still skipped" "byroredux/src/systems/....rs"

    # #4366 — the fenced-block scan's token filter.
    expect_fenced() {
        local want="$1" desc="$2" t="$3"
        if fenced_token_is_checkable "$t"; then got="check"; else got="ignore"; fi
        if [[ "$got" != "$want" ]]; then
            echo "SELFTEST FAIL: fenced \`$t\` — expected $want, got $got ($desc)"
            selftest_failures=$((selftest_failures + 1))
        else
            echo "ok: fenced \`$t\` -> $got ($desc)"
        fi
    }
    expect_fenced check "plain repo path is checked" "crates/renderer/src/mesh.rs"
    expect_fenced check "directory path is checked" "crates/renderer/src/texture_registry/"
    expect_fenced check "closed brace list is checked" "docs/smoke-tests/{a,b}.sh"
    expect_fenced ignore "placeholder template" "docs/audits/AUDIT_<X>_<date>.md"
    expect_fenced ignore "glob" "crates/renderer/shaders/*.comp"
    expect_fenced ignore "brace list truncated mid-line" "byroredux/src/systems/{animation,"
    expect_fenced ignore "filename stem" "docs/audits/AUDIT_TECH_DEBT_"

    if (( selftest_failures > 0 )); then
        echo "SELFTEST: $selftest_failures failure(s)."
        exit 1
    fi
    echo "SELFTEST: all skip-rule expectations hold."
    exit 0
fi

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
                # #3439 — a bare basename asserts existence but carries no
                # directory, so it cannot be "stale" in this gate's
                # wrong-dir sense; it can only be a deleted file. That is a
                # real class and used to be invisible here. It is reported
                # ADVISORY rather than FATAL because the extractor cannot
                # tell a path from any other dotted token: console commands
                # (`mem.frag`), and truncations of longer names produced by
                # the extension pattern having no trailing boundary
                # (`GpuInstance.vertex_offset` -> `GpuInstance.vert`,
                # `self.shutdown` -> `self.sh`), all arrive here looking
                # identical to real rot. Tightening that pattern to require
                # a closing backtick was measured at 174 of 4,091 refs lost
                # (4.3%), which trades this gate's coverage for its noise —
                # so the noise is quarantined instead.
                if [[ "$p" != */* ]]; then
                    missing_basenames+=("$skill:$line_num — \`$p\`")
                else
                    echo "STALE: $skill:$line_num — \`$p\`"
                    stale_count=$((stale_count + 1))
                fi
            elif [[ "$VERBOSE" == "1" ]]; then
                echo "ok: $skill:$line_num — $p"
            fi
        done < <(expand_braces "$local_path")
    done < <(grep -noE '`[A-Za-z0-9_./{},-]+\.(rs|md|toml|comp|frag|vert|glsl|wgsl|sh|xml)' "$skill" || true)
done

# ---------------------------------------------------------------------------
# Fenced-block path scan (#4366)
#
# The loop above only reads backticked tokens, but `_audit-common.md`'s
# Project Layout — the map every audit is told to trust — is a fenced block
# with no backticks, so its rows sat outside the gate. *texture_registry.rs*,
# *boot.rs* and *asset_provider/material.rs* each outlived their split there
# (#4365, and #4121 / #4244 / #4020 before it). Inside fences, repo-rooted
# path tokens get the same FATAL rule; bare basenames go to the deleted-file
# advisory, since a layout row names a module by basename once its directory
# is established.
#
# Command files only: `docs/engine/` fences are shell snippets and format
# dumps, not layout claims.
# ---------------------------------------------------------------------------
for skill in "${command_files[@]}"; do
    [[ -f "$skill" ]] || continue
    while IFS=$'\t' read -r line_num text; do
        while read -r token; do
            [[ -n "$token" ]] || continue
            token="${token%%:[0-9]*}"
            token="${token%[.,;:)]}"
            fenced_token_is_checkable "$token" || continue
            while read -r p; do
                # A nested brace pair the one-pair expander cannot resolve.
                [[ "$p" == *"{"* || "$p" == *"}"* ]] && continue
                checked_count=$((checked_count + 1))
                if ! path_exists "${p%/}"; then
                    echo "STALE: $skill:$line_num — $p (fenced block)"
                    stale_count=$((stale_count + 1))
                fi
            done < <(expand_braces "$token")
        done < <(grep -oE '(crates|byroredux|docs|tools|scripts)/[A-Za-z0-9_./{},<>*-]+' <<< "$text" || true)
        while read -r b; do
            [[ -n "$b" ]] || continue
            path_exists "$b" && continue
            missing_basenames+=("$skill:$line_num — \`$b\` (fenced block)")
        done < <(grep -oE '(^|[[:space:](,;])[A-Za-z0-9_]+\.(rs|md|toml|comp|frag|vert|glsl|sh)\b' <<< "$text" \
                    | sed -E 's/^[[:space:](,;]+//' || true)
    done < <(awk '/^[[:space:]]*```/ { fenced = !fenced; next } fenced { printf "%d\t%s\n", NR, $0 }' "$skill")
done

echo
echo "Checked $checked_count refs across ${#skill_files[@]} skill files."

# #3439 — the deleted-file class, reported and never fatal. See the comment
# at the accumulation site for why this tier exists rather than STALE.
if (( ${#missing_basenames[@]} > 0 )) && [[ "${SKIP_BASENAME_CHECK:-0}" != "1" ]]; then
    echo
    echo "ADVISORY (deleted-file refs) — backticked bare basenames that resolve"
    echo "nowhere in the tree. Each is either (a) a file that was deleted rather"
    echo "than moved — update or drop the reference, or (b) not a path at all"
    echo "(a console command, or a longer name the extractor truncated) — in"
    echo "which case italicise it instead of backticking."
    printf '  %s\n' "${missing_basenames[@]}"
    echo
    echo "  ${#missing_basenames[@]} advisory ref(s). Not a failure."
    echo "  Set SKIP_BASENAME_CHECK=1 to silence."
fi

# ---------------------------------------------------------------------------
# NUL bytes in tracked text sources (FATAL)
#
# #3210 — three raw NUL bytes inside byte-string literals in
# `crates/plugin/src/esm/records/misc/quest.rs`'s sibling
# `crates/plugin/src/esm/records/tests.rs` (`b"Long Barrel<NUL>"`, where the
# source meant the two-character escape `\0`) made GNU grep classify the whole
# 1,944-line file as binary and skip it silently. That hid 40 regression guards
# citing 31 issue numbers from the `grep -rn "#<N>" --include='*.rs'` discovery
# recipe that all 27 audit skills prescribe — including two guards that landed
# after the file went binary and so were never greppable at any point in their
# life. They are valid Rust and compile fine, so nothing in the build complains.
#
# The failure mode is the worst kind: an auditor following the documented recipe
# concludes "fix present, no guard" (a PARTIAL where the truth is PASS) or
# "guard deleted" (a FAIL against a fix that is right there). The 2026-08-20
# sweep came one command from publishing exactly that FAIL.
#
# This is FATAL rather than advisory because there is no legitimate reason for a
# tracked `.rs` / `.md` / shader / script source to contain a NUL, and because
# the cost of missing one is measured in false audit findings.
#
# `scripts/check-text-source-integrity.sh` already owns the check and CI already
# runs it on every PR ("Reject grep-blinding NUL bytes"). Delegate rather than
# reimplement: this gate is what an auditor runs locally, and CI only fires on a
# PR, so the value here is reaching the same verdict before the push — not a
# second copy of the rule that can drift from the first.
# ---------------------------------------------------------------------------
if ! nul_report=$(scripts/check-text-source-integrity.sh 2>&1); then
    echo
    echo "STALE  tracked text source(s) contain NUL bytes — plain \`grep\` skips them"
    echo "       silently, hiding every symbol and issue citation inside. See #3210."
    printf '%s\n' "$nul_report" | sed 's/^/       /'
    stale_count=$((stale_count + 1))
fi

# ---------------------------------------------------------------------------
# Ownership-map coverage (`.claude/commands/_audit-owners.md`)
#
# Replaces the hand-maintained "Crate count: N" literal, which went stale on
# every crate addition (#2261 hkx, #2420 mod-runtime) and could only ever say
# "the number moved", never "nobody audits the new thing". The map is the one
# place ownership lives; this derives everything else from the live tree.
#   FATAL     a row whose path prefix matches no tracked file (rotted row)
#   FATAL     a tracked crates/* or tools/* directory with no owner row
#   ADVISORY  a byroredux/src module >=300 non-test LOC that only the
#             `byroredux/src/` catch-all row reaches (owned in name only)
# ---------------------------------------------------------------------------
owners_md=.claude/commands/_audit-owners.md
if [[ ! -f "$owners_md" ]]; then
    echo
    echo "STALE  $owners_md is missing — ownership coverage cannot be checked"
    stale_count=$((stale_count + 1))
else
    owner_prefixes=()
    while IFS= read -r row; do
        [[ -n "$row" ]] || continue
        while IFS= read -r pfx; do
            [[ -n "$pfx" ]] && owner_prefixes+=("$pfx")
        done < <(expand_braces "$row")
    done < <(sed -nE 's/^\| `([^`]+)` \|.*/\1/p' "$owners_md")

    has_tracked_prefix() {  # $1 = prefix; true iff some tracked path starts with it
        awk -v p="$1" 'index($0, p) == 1 { found = 1; exit } END { exit !found }' "$all_paths_file"
    }

    if (( ${#owner_prefixes[@]} == 0 )); then
        echo
        echo "STALE  $owners_md — no parseable ownership rows"
        stale_count=$((stale_count + 1))
    fi

    # Every owner a row names must be a real skill (`per-game` is the one
    # placeholder: it means "whichever audit-<game> applies").
    while IFS= read -r owner; do
        [[ -n "$owner" && "$owner" != "per-game" ]] || continue
        if [[ ! -f ".claude/commands/audit-${owner}/SKILL.md" ]]; then
            echo
            echo "STALE  $owners_md — owner \`$owner\` has no .claude/commands/audit-${owner}/SKILL.md"
            stale_count=$((stale_count + 1))
        fi
    done < <(sed -nE 's/^\| `[^`]+` \| ([^|]+) \|.*/\1/p' "$owners_md" \
                | tr ';,' '\n\n' \
                | sed -E 's/\([^)]*\)//g; s/ Dims? [0-9][0-9+-]*//g; s/^[[:space:]]+//; s/[[:space:]]+$//' \
                | sort -u)

    for pfx in "${owner_prefixes[@]}"; do
        if ! has_tracked_prefix "$pfx"; then
            echo
            echo "STALE  $owners_md — owner row \`$pfx\` matches no tracked path (moved or deleted)"
            stale_count=$((stale_count + 1))
        fi
    done

    # True iff some owner row sits at or under `$1` (a specific owner), or
    # covers it from above. `$2` = "specific" ignores the byroredux/src/ catch-all.
    is_owned() {
        local target="$1" mode="${2:-any}" pfx
        for pfx in "${owner_prefixes[@]}"; do
            [[ "$mode" == "specific" && "$pfx" == "byroredux/src/" ]] && continue
            [[ "$target" == "$pfx"* || "$pfx" == "$target"* ]] && return 0
        done
        return 1
    }

    while IFS= read -r d; do
        is_owned "$d/" any || {
            echo
            echo "STALE  $d/ has no owner row in $owners_md — add one (and an audit that covers it)"
            stale_count=$((stale_count + 1))
        }
    done < <(cut -d/ -f1-2 "$all_paths_file" | grep -E '^(crates|tools)/[^/]+$' | sort -u)

    unowned_modules=()
    while IFS= read -r entry; do
        [[ "$entry" == *_tests.rs || "$entry" == *tests ]] && continue
        loc=$(grep -E "^byroredux/src/${entry}(\.rs$|/)" "$all_paths_file" \
                | grep -vE '(_tests?\.rs|/tests?/|/tests\.rs)$' \
                | xargs cat 2>/dev/null | wc -l | tr -d ' ')
        (( loc >= 300 )) || continue
        is_owned "byroredux/src/$entry" specific || unowned_modules+=("byroredux/src/$entry ($loc LOC)")
    done < <(grep -E '^byroredux/src/' "$all_paths_file" | sed -E 's|^byroredux/src/([^/]+).*|\1|; s|\.rs$||' | sort -u)

    if (( ${#unowned_modules[@]} > 0 )); then
        echo
        echo "ADVISORY (ownership) — byroredux/src modules >=300 LOC reached only by the catch-all row:"
        printf '  %s\n' "${unowned_modules[@]}"
        echo "  Add a specific row to $owners_md (or fold into an existing prefix). Not a failure."
    fi
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
# Heuristic: every backticked identifier-shaped token >=7 chars that appears in
# NO tracked source file. Advisory only, because the noise floor is real and
# mostly legitimate: baseline TSV column names, git hashes, memory-file slugs,
# rustc lint names, and deliberate references to symbols that SHOULD NOT exist.
# Filters below remove the known-benign classes; what survives is worth a look,
# not an automatic failure. Historical names should be *italicised*, not
# backticked (same rule as backwards-looking paths).
#
# Two structural blind spots closed 2026-08-20 (#3197) — before that, this
# block printed "0 advisories" for reasons that had nothing to do with being
# clean:
#   (a) the needle was anchored `[a-z]`, so every SCREAMING_SNAKE_CASE constant
#       was excluded BEFORE any existence check ran. That is the convention for
#       budgets, limits and flag bits — MAX_TOTAL_BONES, GLASS_RAY_BUDGET,
#       INSTANCE_FLAG_*, MAT_FLAG_* — i.e. exactly the class audit skills quote
#       most, and exactly the class whose drift is a wrong number in a GPU
#       layout contract. 157 such symbols were backticked and none examined.
#   (b) the corpus was raw `grep -qw` over whole lines, so a symbol whose ONLY
#       occurrence is inside an assertion that it must NOT exist counted as
#       evidence that it DOES. That is what hid #3052's REFRACT_PASSTHRU_BUDGET,
#       whose sole hit is `!src.contains("REFRACT_PASSTHRU_BUDGET = 2")`.
# Both had to close together: widening the regex alone still missed #3052.
# The corpus now also covers shader sources, since skills legitimately cite GLSL
# constants (RESTIR_M_CAP lives in triangle.frag, not in any .rs).
# ---------------------------------------------------------------------------
if [[ "${SKIP_SYMBOL_CHECK:-0}" != "1" ]]; then
    src_blob=$(mktemp)
    trap 'rm -f "$all_paths_file" "$src_blob"' EXIT
    # Shader sources count: skills cite GLSL constants that exist in no .rs.
    # Lines that ASSERT a symbol is absent (`!src.contains("FOO")`, `!source
    # .contains(...)`) must not count as evidence the symbol exists — see (b).
    # `grep -a` is load-bearing: several .rs test fixtures embed raw NIF/BSA
    # bytes, and without it grep calls the whole concatenated stream binary and
    # emits nothing, silently truncating the corpus to ~70% and turning every
    # symbol past the first NUL into a false advisory.
    git ls-files '*.rs' '*.glsl' '*.vert' '*.frag' '*.comp' '*.rgen' '*.rchit' \
        | xargs cat 2>/dev/null \
        | grep -a -vE '!\s*[A-Za-z_][A-Za-z0-9_]*\s*\.contains\(|!\s*contains\(' \
        > "$src_blob" || true

    # #3202 — the two corpora are reported separately rather than merged.
    # `docs/engine/` contributes ~275 advisories to the command files' ~10,
    # and merging them buries the tuned list under the untuned one. The doc
    # advisories are not mostly wrong — the run that added them caught
    # `extract_tangents` (the real symbol is `extract_tangents_from_extra_data`)
    # — but reference docs legitimately name a great deal of vocabulary that
    # is not, and never will be, a repo symbol: Papyrus event names, nif.xml
    # field names, Vulkan entry points, GMST/perk/actor-value rosters, and
    # on-disk format fields. Those are filtered by pattern below where they
    # are mechanically identifiable. What is left over is a genuinely mixed
    # bag of drifted symbols and forward-looking design names, so it stays
    # advisory and stays in its own section with its own count.
    symbol_advisory() {
        local label="$1"
        shift
        local -a files=("$@")
        local suspect_count=0
        local sym
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
            [[ "$sym" == TECH_DEBT || "$sym" == VERTEX_INPUT ]] && continue  # prose, not symbols

            # #3202 — external vocabulary the reference docs quote by name.
            # None of these can ever resolve to a repo symbol, so flagging
            # them says nothing about drift. Pattern-matched, not listed, so
            # the filter does not become the hand-maintained roster #2983
            # penalised.
            [[ "$sym" == has_* || "$sym" == uses_* ]] && continue   # nif.xml condition fields
            [[ "$sym" == bhk_* || "$sym" == nif_* ]] && continue    # nif.xml block/field names
            [[ "$sym" =~ ^On[A-Z] ]] && continue                    # Papyrus event names
            [[ "$sym" =~ ^Get[A-Z] ]] && continue                   # CTDA condition functions
            [[ "$sym" =~ ^[Vv]k[A-Z] || "$sym" == VK_* ]] && continue  # Vulkan / ash API

            grep -qw "$sym" "$src_blob" && continue
            if (( suspect_count == 0 )); then
                echo
                echo "ADVISORY ($label) — backticked symbols not found in any tracked source file:"
            fi
            printf '  %-46s %s\n' "$sym" \
                "$(grep -rlE "\`$sym(\`| =)" "${files[@]}" 2>/dev/null \
                    | sed 's|.claude/commands/||;s|/SKILL.md||;s|docs/engine/||' | tr '\n' ' ')"
            suspect_count=$((suspect_count + 1))
        done < <(
            # Pass 1: a backticked span that is exactly one identifier.
            # Pass 2: a backticked span of the form `SYMBOL = value` — how skills
            # quote a constant together with its value. Blind spot (c), found while
            # closing (a) and (b): #3052's `REFRACT_PASSTHRU_BUDGET = 2` is matched
            # by neither the old lowercase needle NOR the widened whole-span one,
            # because the span is not a bare identifier. Narrow on purpose — a
            # general "leading word of any backticked span" rule re-floods this
            # list with shell snippets and prose.
            {
                grep -rhoE '`[A-Za-z][A-Za-z0-9_]{6,}`' "${files[@]}" 2>/dev/null | tr -d '`'
                grep -rhoE '`[A-Za-z][A-Za-z0-9_]{6,} =' "${files[@]}" 2>/dev/null \
                    | sed 's/^`//; s/ =$//'
            } | sort -u
        )

        if (( suspect_count > 0 )); then
            echo
            echo "  $suspect_count advisory symbol(s) in $label. Each is either (a) genuinely"
            echo "  renamed — update the reference, or (b) an intentional historical /"
            echo "  never-should-exist / not-yet-built name — italicise it instead of"
            echo "  backticking. Not a failure."
        fi
    }

    symbol_advisory "audit skills" "${command_files[@]}"
    symbol_advisory "docs/engine reference docs" "${reference_docs[@]}"
    echo
    echo "  Set SKIP_SYMBOL_CHECK=1 to silence both advisories."
fi

# ---------------------------------------------------------------------------
# Guard-citation attribution (#4013).
#
# The path gate above cannot catch this class: the skills cite a regression
# guard as `` `test_name` (`some_tests.rs`) ``, and when the test moves to a
# sibling file the backticked path still RESOLVES — only the symbol->file
# association is wrong. An auditor working the checklist opens the named
# file, does not find the guard, and either files a false regression or
# re-derives the location by hand. Three of the six citations in this shape
# were wrong when the check was written, all three pointing at
# `gpu_instance_layout_tests.rs` for tests living in `shader_contract_tests.rs`.
#
# Deliberately narrow — an identifier IMMEDIATELY followed by a
# parenthesised `*tests.rs` path, which is the guard-citation idiom and
# nothing else. A looser same-sentence rule was tried first and produced
# 40% false positives (a function named mid-bullet next to an unrelated
# "Guard tests:" clause at the end of the same line, and a brace-expanded
# `{a,b}.rs` set that names both correct homes).
#
# Fails ONLY on a provable mis-attribution: the identifier is defined
# somewhere in the tree, and nowhere that matches the cited path. An
# identifier found nowhere is left to the symbol advisory above — that is
# "renamed or never existed", a different defect with a different fix, and
# hard-failing on it would block a skill written ahead of its guard.
misattributed_count=0
rs_paths_file=$(mktemp)
grep -E '\.rs$' "$all_paths_file" > "$rs_paths_file"

for skill in "${skill_files[@]}"; do
    [[ -f "$skill" ]] || continue
    while IFS= read -r match; do
        [[ -n "$match" ]] || continue
        line_num="${match%%:*}"
        span="${match#*:}"
        span="${span#\`}"                 # drop the leading backtick FIRST,
        ident="${span%%\`*}"              # so this cuts at the CLOSING one
        cited="${span##*(\`}"
        cited="${cited%\`)}"
        [[ -n "$ident" ]] || continue

        # Where is `fn <ident>` actually defined? `xargs -a` rather than
        # `$(cat ...)` — the repo has thousands of tracked .rs files and the
        # expanded form is an ARG_MAX hazard.
        homes=$(xargs -a "$rs_paths_file" grep -lE "fn ${ident}[[:space:]]*\(" 2>/dev/null || true)
        [[ -n "$homes" ]] || continue   # nowhere: the symbol advisory's job

        if ! grep -qE "(^|/)${cited//./\\.}\$" <<< "$homes"; then
            if (( misattributed_count == 0 )); then
                echo
                echo "MIS-ATTRIBUTED guard citations (the path resolves; the symbol is not in it):"
            fi
            echo "  $skill:$line_num"
            echo "      \`$ident\` cited as living in \`$cited\`"
            echo "      actually defined in: $(tr '\n' ' ' <<< "$homes")"
            misattributed_count=$((misattributed_count + 1))
        fi
    done < <(grep -noE '`[a-z][a-z0-9_]{11,}`[[:space:]]*\(`[A-Za-z0-9_/.]*tests\.rs`\)' "$skill" 2>/dev/null || true)
done
rm -f "$rs_paths_file"

if (( misattributed_count > 0 )); then
    echo
    echo "FAIL: $misattributed_count mis-attributed guard citation(s)."
    echo "Fix: repoint the citation at the file that actually defines the test."
fi

if (( stale_count > 0 || misattributed_count > 0 )); then
    echo
    (( stale_count > 0 )) && echo "FAIL: $stale_count stale path reference(s)."
    (( stale_count > 0 )) && echo "Fix: update the audit skill files, OR delete the stale ref if the target moved."
    exit 1
fi
echo "OK: all path references valid."
