#!/usr/bin/env bash
# Reclaim disk space from target/ once it grows "out of reach" — a manual
# guard, not a cron job or build hook (by design; see the script's own
# discussion if that changes).
#
# target/ has three independent disease vectors, and this script treats
# them differently because they don't behave the same way under a rebuild:
#
#   1. target/debug/incremental — pure incremental-compilation cache. Stale
#      the moment source changes, worthless to keep across sessions, cheap
#      to regenerate. Wiped unconditionally, no age check.
#
#   2. target/debug/examples — one hash-suffixed binary (+ a `.d` file, +
#      one `.dwo` split-debuginfo companion per codegen unit — see the
#      workspace Cargo.toml's `split-debuginfo = "unpacked"` comment for why
#      those exist) per build of every example ever compiled. cargo never
#      deletes the previous hash when a dependency-graph tweak mints a new
#      one, so this accumulates forever. Measured 2026-09-04: 354k files,
#      83 GB, and — because this workspace rebuilds many times a day — an
#      absolute mtime cutoff barely helps (only 6.3k of 354k files were
#      older than 14 days). The actual disease is "N stale hash-generations
#      of the same example," not age, so this prunes by generation instead:
#      per example basename, keep only the files sharing the most-recently
#      modified hash for that basename and delete every older hash's files.
#      Always leaves each example runnable at its latest build.
#
#   3. Abandoned named CARGO_TARGET_DIR trees — scripts/*.sh (bisects, FSR
#      benchmark matrices, renderer-anchor sweeps, …) each point
#      CARGO_TARGET_DIR at their own target/<name> to avoid stomping the
#      main build. Those are whole one-off trees, not per-file hash churn,
#      so age-based pruning is the right tool here: anything under target/
#      that isn't a recognized standard subdirectory AND hasn't been
#      touched in --age-days is deleted outright.
#
# Deliberately NOT touched: target/debug/deps (the single biggest
# contributor — measured 251 GB, over a million orphaned `.dwo` files — but
# also the one an ordinary `cargo build`/`cargo test` actually reads from;
# pruning it forces a slow full relink) and target/release. If deps/ ever
# needs the same treatment, extend prune_examples_stale_hashes's generation
# logic to it rather than reaching for `cargo clean`.
#
# Usage:
#   scripts/prune-target-cache.sh              # prune only if target/ > --limit-gb
#   scripts/prune-target-cache.sh --force       # prune regardless of current size
#   scripts/prune-target-cache.sh --dry-run     # report what would be freed, change nothing
#   scripts/prune-target-cache.sh --limit-gb 50 --age-days 7
#
# Not wired to cron, a git hook, or a Claude Code hook — run it by hand (or
# from your own shell alias/cron) when `du -sh target` looks out of hand.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-${repo_root}/target}"
limit_gb=100
age_days=14
dry_run=0
force=0

usage() {
    cat >&2 <<EOF
Usage: $(basename "$0") [--limit-gb N] [--age-days N] [--target-dir PATH] [--dry-run] [--force]

  --limit-gb N     Only prune if target/ exceeds N GB (default: ${limit_gb})
  --age-days N     Abandoned CARGO_TARGET_DIR trees older than N days are
                    deleted outright (default: ${age_days})
  --target-dir PATH  Target directory to operate on (default: \$CARGO_TARGET_DIR
                    or <repo>/target)
  --dry-run        Print what would be deleted; delete nothing
  --force          Prune even if target/ is under --limit-gb
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
    --limit-gb)
        limit_gb="$2"
        shift 2
        ;;
    --age-days)
        age_days="$2"
        shift 2
        ;;
    --target-dir)
        target_dir="$2"
        shift 2
        ;;
    --dry-run)
        dry_run=1
        shift
        ;;
    --force)
        force=1
        shift
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        echo "prune-target-cache: unknown argument: $1" >&2
        usage
        exit 2
        ;;
    esac
done

if [[ ! -f "${target_dir}/CACHEDIR.TAG" ]]; then
    echo "prune-target-cache: '${target_dir}' doesn't look like a cargo target dir" \
        "(no CACHEDIR.TAG) — refusing to touch it" >&2
    exit 2
fi

du_bytes() {
    du -sb "$1" 2>/dev/null | cut -f1
}

human() {
    numfmt --to=iec --suffix=B "$1" 2>/dev/null || echo "${1}B"
}

run() {
    if [[ "${dry_run}" -eq 1 ]]; then
        echo "  [dry-run] $*"
    else
        "$@"
    fi
}

size_before=$(du_bytes "${target_dir}")
limit_bytes=$((limit_gb * 1024 * 1024 * 1024))

echo "prune-target-cache: ${target_dir} is $(human "${size_before}") (limit: ${limit_gb} GB)"

if [[ "${force}" -eq 0 && "${size_before}" -lt "${limit_bytes}" ]]; then
    echo "prune-target-cache: under the limit, nothing to do (pass --force to prune anyway)"
    exit 0
fi

echo "prune-target-cache: pruning$([ "${dry_run}" -eq 1 ] && echo ' (dry run)')…"

# ── 1. incremental — unconditional wipe ─────────────────────────────
incremental_dir="${target_dir}/debug/incremental"
if [[ -d "${incremental_dir}" ]]; then
    incremental_size=$(du_bytes "${incremental_dir}")
    echo "  incremental: $(human "${incremental_size}")"
    run rm -rf -- "${incremental_dir}"
fi

# ── 2. examples — keep only each basename's most-recently-built hash ──
examples_dir="${target_dir}/debug/examples"
if [[ -d "${examples_dir}" ]]; then
    stale_list="$(mktemp)"
    trap 'rm -f -- "${stale_list}"' EXIT

    # For every file directly under examples/, extract (basename, hash,
    # mtime). Files without a `-<16 hex>` hash segment (the small number of
    # unhashed convenience copies cargo also writes) are left alone —
    # they're always overwritten in place on the next build, so they never
    # accumulate.
    find "${examples_dir}" -maxdepth 1 -type f -printf '%T@\t%p\n' |
        awk -F'\t' '
        {
            mtime = $1
            path = $2
            n = split(path, parts, "/")
            name = parts[n]
            if (match(name, /-[0-9a-f]{16}/)) {
                base = substr(name, 1, RSTART - 1)
                hash = substr(name, RSTART + 1, 16)
            } else {
                next  # unhashed convenience copy — always current, skip
            }
            key = base SUBSEP hash
            if (mtime > newest_mtime[base]) {
                newest_mtime[base] = mtime
                newest_hash[base] = hash
            }
            files[NR] = path
            file_base[NR] = base
            file_hash[NR] = hash
            count++
        }
        END {
            for (i = 1; i <= count; i++) {
                b = file_base[i]
                if (file_hash[i] != newest_hash[b]) {
                    print files[i]
                }
            }
        }' >"${stale_list}"

    stale_count=$(wc -l <"${stale_list}")
    if [[ "${stale_count}" -gt 0 ]]; then
        stale_bytes=$(du -cb --files0-from=<(tr '\n' '\0' <"${stale_list}") 2>/dev/null |
            tail -1 | cut -f1)
        echo "  examples: ${stale_count} stale hash-generation files, $(human "${stale_bytes:-0}")"
        if [[ "${dry_run}" -eq 1 ]]; then
            echo "  [dry-run] rm -f <${stale_count} files listed above>"
        else
            xargs -a "${stale_list}" -d '\n' rm -f --
        fi
    else
        echo "  examples: nothing stale"
    fi
fi

# ── 3. abandoned named CARGO_TARGET_DIR trees ───────────────────────
# Anything at the top level of target/ that isn't a recognized standard
# cargo output (profile dirs, doc, a target-triple cross-compile dir, the
# cargo bookkeeping files) is an ad-hoc tree some script pointed
# CARGO_TARGET_DIR at. Prune it if untouched in --age-days.
is_standard_subdir() {
    case "$1" in
    debug | release | doc | tmp | miri | package | .rustc_info.json | CACHEDIR.TAG)
        return 0
        ;;
    esac
    # Rust target-triple shape, e.g. x86_64-pc-windows-gnu, wasm32-unknown-unknown.
    [[ "$1" =~ ^[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_]+ ]]
}

while IFS= read -r -d '' entry; do
    name="$(basename "${entry}")"
    is_standard_subdir "${name}" && continue
    size=$(du_bytes "${entry}")
    echo "  abandoned tree: ${name} ($(human "${size}"), untouched ${age_days}+ days)"
    run rm -rf -- "${entry}"
done < <(find "${target_dir}" -mindepth 1 -maxdepth 1 -mtime "+${age_days}" -print0)

size_after=$(du_bytes "${target_dir}")
if [[ "${dry_run}" -eq 1 ]]; then
    echo "prune-target-cache: dry run — no changes made. Re-run without --dry-run to apply."
else
    freed=$((size_before - size_after))
    echo "prune-target-cache: $(human "${size_before}") -> $(human "${size_after}")" \
        "(freed $(human "${freed}"))"
fi
