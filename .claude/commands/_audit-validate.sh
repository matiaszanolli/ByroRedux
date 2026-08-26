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
reference_docs=(
    docs/engine/*.md
)
skill_files=("${command_files[@]}" "${reference_docs[@]}")
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

if (( stale_count > 0 )); then
    echo
    echo "FAIL: $stale_count stale path reference(s)."
    echo "Fix: update the audit skill files, OR delete the stale ref if the target moved."
    exit 1
fi
echo "OK: all path references valid."
