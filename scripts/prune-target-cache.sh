#!/usr/bin/env bash
# Reclaim disk space from target/ once it grows "out of reach" — a manual
# guard, not a cron job or build hook (by design; see the script's own
# discussion if that changes).
#
# target/ has five independent disease vectors, and this script treats
# them differently because they don't behave the same way under a rebuild:
#
#   1. <profile>/incremental — pure incremental-compilation cache. Stale
#      the moment source changes, worthless to keep across sessions, cheap
#      to regenerate. Wiped unconditionally (under the profile's build
#      lock, below), no age check.
#
#   2. target/debug/examples under a pre-1.100 cargo — one hash-suffixed
#      binary (+ a `.d` file, + one `.dwo` split-debuginfo companion per
#      codegen unit — see the workspace Cargo.toml's `split-debuginfo =
#      "unpacked"` comment for why those exist) per build of every example
#      ever compiled. cargo never deletes the previous hash when a
#      dependency-graph tweak mints a new one, so this accumulates forever.
#      Measured 2026-09-04: 354k files, 83 GB, and — because this workspace
#      rebuilds many times a day — an absolute mtime cutoff barely helps
#      (only 6.3k of 354k files were older than 14 days). The actual disease
#      is "N stale hash-generations of the same example," not age, so this
#      prunes by generation instead: per example basename, keep only the
#      files sharing the most-recently modified hash for that basename and
#      delete every older hash's files. Always leaves each example runnable
#      at its latest build. Under cargo >= 1.100 examples/ holds only the
#      uplifted, unhashed binaries, and step 5 removes the hashed leftovers.
#
#   3. Abandoned named CARGO_TARGET_DIR trees — scripts/*.sh (bisects, FSR
#      benchmark matrices, renderer-anchor sweeps, …) each point
#      CARGO_TARGET_DIR at their own target/<name> to avoid stomping the
#      main build. Those are whole one-off trees, not per-file hash churn,
#      so age-based pruning is the right tool here: anything under target/
#      that isn't a recognized standard subdirectory AND hasn't been
#      touched in --age-days is deleted outright.
#
#   4. Stale unit generations (cargo >= 1.100, the toolchain pin since
#      2026-11-14). cargo 1.100 gives every compilation unit its own
#      directory, <profile>/build/<pkg>/<hash>/, holding `fingerprint/` and
#      `out/` (plus `run/` for a build-script run). The hash moves whenever
#      the unit's toolchain, features, profile or dependency graph does, and
#      cargo never deletes the old directory — the old deps/ disease (251 GB,
#      over a million orphaned `.dwo` files) in a new shape. Pruning by
#      "newest hash per package" would be wrong here: a package legitimately
#      owns many live units at once (lib, test harness, each example, check
#      vs build, `-p` vs `--workspace` feature unification — byroredux-core
#      alone has ~20). Two signals are reliable instead:
#        a. Toolchain. Each unit's fingerprint JSON opens with
#           {"rustc":<hash of `rustc -vV`>}. The script learns the pinned
#           toolchain's hash by checking an empty crate with the cargo that
#           `rustup which cargo` resolves from the repo root (so a distro
#           cargo shadowing the shims, #4466, can't skew it); a unit with any
#           other hash was built by a toolchain this repo no longer uses, and
#           cargo will never read it again. This is the bulk of the garbage
#           after a bump. As a guard against a mis-probe, a profile dir with
#           no unit of the pinned hash yet keeps its other-toolchain units:
#           build once with the new pin, then prune.
#        b. Last use. Every build that includes a unit, even a no-op one,
#           reads its fingerprint hash file (`<kind>-<name>`) and dep-info
#           (measured 2026-11-14: a fresh `cargo build` moved those atimes on
#           exactly the four units in its graph and left the test/example/
#           clippy units alone). It does NOT read the `.json`, which only
#           explains a rebuild — and which this script reads for (a), so the
#           JSON's atime is never part of the signal. On a `relatime` mount
#           the rest tracks "last read by any cargo command" to within a day,
#           so a unit whose non-JSON fingerprint files are all older than
#           --age-days is one no build you run still needs. A wrongly-aged
#           unit costs one rebuild, never correctness. With `noatime` that
#           signal is gone — mtime would delete every long-stable
#           dependency — so only (a) runs.
#
#   5. Pre-1.100 leftovers. Once the pinned cargo writes the new layout,
#      a profile dir's deps/, .fingerprint/, build/<pkg>-<hash>/ build-script
#      dirs and hashed examples/<name>-<hash>* files are never read again
#      (the 1.96 → 1.100 bump left 371 GB of them in this checkout).
#
# Steps 1, 4 and 5 cover the standard profile dirs (debug/, release/, and
# <triple>/{debug,release}/ for cross builds; this workspace defines no
# custom profiles) and hold each one's `.cargo-lock` — the lock every cargo
# version takes for a build — so a build in flight is skipped, not broken,
# and none can start mid-prune. Named CARGO_TARGET_DIR trees are left to
# step 3: a bisect tree is deliberately built by an older toolchain.
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
  --age-days N     Unit directories no cargo command has read in N days, and
                    abandoned CARGO_TARGET_DIR trees untouched for N days, are
                    deleted (default: ${age_days})
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

scratch="$(mktemp -d)"
trap 'rm -rf -- "${scratch}"' EXIT

du_bytes() {
    du -sb "$1" 2>/dev/null | cut -f1
}

# Total bytes of the NUL-separated paths in file $1 (0 for an empty list).
du_list_bytes() {
    if [[ -s "$1" ]]; then
        du -scb --files0-from="$1" 2>/dev/null | tail -1 | cut -f1
    else
        echo 0
    fi
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

# The target-triple check needs an EXACT match against rustc's own list, not
# a "looks like N hyphen-separated segments" pattern: this workspace's ad-hoc
# probe names (renderer-anchor-c25-v-head-filtered-r3,
# rt-decomposition-medtek-2026-08-11, …) all have 2+ hyphens too, so a loose
# pattern silently reclassified them as "standard" and skipped every one.
_target_triples=""
is_known_target_triple() {
    if [[ -z "${_target_triples}" ]]; then
        _target_triples="$(rustc --print target-list 2>/dev/null)"$'\n'
    fi
    [[ $'\n'"${_target_triples}" == *$'\n'"$1"$'\n'* ]]
}

size_before=$(du_bytes "${target_dir}")
limit_bytes=$((limit_gb * 1024 * 1024 * 1024))

echo "prune-target-cache: ${target_dir} is $(human "${size_before}") (limit: ${limit_gb} GB)"

if [[ "${force}" -eq 0 && "${size_before}" -lt "${limit_bytes}" ]]; then
    echo "prune-target-cache: under the limit, nothing to do (pass --force to prune anyway)"
    exit 0
fi

echo "prune-target-cache: pruning$([ "${dry_run}" -eq 1 ] && echo ' (dry run)')…"

# ── the pinned toolchain: its fingerprint hash, and which layout it writes ──
# Check an empty crate from the repo root, so rustup applies rust-toolchain.toml.
# Its own `[workspace]` table keeps cargo from adopting it into this workspace.
# The hash is the same for every crate and target dir (1.100.0 →
# 2642861616376027855, 1.96.0 → 16696009966390864400 on this machine).
pinned_rustc=""
pinned_new_layout=""
pinned_cargo="cargo"
if command -v rustup >/dev/null 2>&1; then
    pinned_cargo="$(cd "${repo_root}" && rustup which cargo 2>/dev/null)" || pinned_cargo="cargo"
fi
mkdir -p "${scratch}/probe/src"
printf '[package]\nname = "prune_probe"\nversion = "0.0.0"\nedition = "2021"\n\n[workspace]\n' \
    >"${scratch}/probe/Cargo.toml"
: >"${scratch}/probe/src/lib.rs"
if (cd "${repo_root}" && "${pinned_cargo}" check --quiet --offline \
    --manifest-path "${scratch}/probe/Cargo.toml" --target-dir "${scratch}/probe/target") \
    >/dev/null 2>&1; then
    probe_json="$(find "${scratch}/probe/target" -name 'lib-prune_probe.json' -print -quit)"
    if [[ -n "${probe_json}" ]]; then
        pinned_rustc="$(sed -n '1s/^{"rustc":\([0-9]*\),.*/\1/p' "${probe_json}")"
        if [[ "${probe_json}" == */build/prune_probe/* ]]; then
            pinned_new_layout=1
        else
            pinned_new_layout=0
        fi
    fi
fi
if [[ -z "${pinned_rustc}" ]]; then
    echo "  note: couldn't fingerprint the pinned toolchain (cargo probe failed) —" \
        "toolchain-stale units and pre-1.100 leftovers are left alone"
fi

# atime is only a "last read" signal when the mount maintains it.
use_atime=0
mount_opts="$(findmnt -no OPTIONS --target "${target_dir}" 2>/dev/null || true)"
if [[ -n "${mount_opts}" && ",${mount_opts}," != *,noatime,* ]]; then
    use_atime=1
else
    echo "  note: ${target_dir} is not on a mount that maintains atime" \
        "(${mount_opts:-unknown}) — units are pruned by toolchain only, not by age"
fi

# ── 2. examples — keep only each basename's most-recently-built hash ──
# Pre-1.100 cargo only; under the new layout step 5 owns the hashed files.
examples_dir="${target_dir}/debug/examples"
if [[ -d "${examples_dir}" && "${pinned_new_layout}" != 1 ]]; then
    stale_list="${scratch}/stale-examples"

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
    debug | release | doc | tmp | miri | package | \
        .rustc_info.json | .rustdoc_fingerprint.json | CACHEDIR.TAG)
        return 0
        ;;
    esac
    is_known_target_triple "$1"
}

while IFS= read -r -d '' entry; do
    name="$(basename "${entry}")"
    is_standard_subdir "${name}" && continue
    size=$(du_bytes "${entry}")
    echo "  abandoned tree: ${name} ($(human "${size}"), untouched ${age_days}+ days)"
    run rm -rf -- "${entry}"
done < <(find "${target_dir}" -mindepth 1 -maxdepth 1 -mtime "+${age_days}" -print0)

# ── 1, 4 + 5. per profile: incremental, stale units, pre-1.100 leftovers ──
profile_dirs=()
for profile in debug release; do
    [[ -d "${target_dir}/${profile}" ]] && profile_dirs+=("${target_dir}/${profile}")
done
while IFS= read -r -d '' triple_dir; do
    is_known_target_triple "$(basename "${triple_dir}")" || continue
    for profile in debug release; do
        [[ -d "${triple_dir}/${profile}" ]] && profile_dirs+=("${triple_dir}/${profile}")
    done
done < <(find "${target_dir}" -mindepth 1 -maxdepth 1 -type d -print0)

# Delete the NUL-separated paths in file $1, reported as "$2: N <noun>, SIZE".
prune_list() {
    local list="$1" label="$2" noun="$3" count
    count=$(tr -cd '\0' <"${list}" | wc -c)
    [[ "${count}" -gt 0 ]] || return 0
    echo "  ${label}: ${count} ${noun}, $(human "$(du_list_bytes "${list}")")"
    if [[ "${dry_run}" -eq 1 ]]; then
        if [[ "${noun}" == "unit dirs" ]]; then
            # Which packages they belong to (build/<pkg>/<hash>), busiest first.
            tr '\0' '\n' <"${list}" | awk -F/ '{ print $(NF - 1) }' | sort | uniq -c | sort -rn |
                head -8 | awk '{ printf "      %5d  %s\n", $1, $2 }'
        fi
        echo "  [dry-run] rm -rf <${count} ${noun}>"
    else
        xargs -0 -a "${list}" rm -rf --
    fi
}

cutoff=$(($(date +%s) - age_days * 86400))
for profile_dir in "${profile_dirs[@]}"; do
    rel="${profile_dir#"${target_dir}"/}"
    exec {lock_fd}>>"${profile_dir}/.cargo-lock"
    if ! flock -n "${lock_fd}"; then
        echo "  ${rel}: cargo is building here — skipped"
        exec {lock_fd}>&-
        continue
    fi

    # 1. incremental — unconditional wipe.
    if [[ -d "${profile_dir}/incremental" ]]; then
        echo "  ${rel} incremental: $(human "$(du_bytes "${profile_dir}/incremental")")"
        run rm -rf -- "${profile_dir}/incremental"
    fi

    # 4. Unit generations: one record per `build/<pkg>/<hash>/` unit that has
    # fingerprint files — "<newest fingerprint atime>\t<rustc hash>\t<dir>".
    # A unit with no fingerprint files yet is mid-first-build: never touched.
    build_dir="${profile_dir}/build"
    units="${scratch}/units"
    : >"${units}"
    if [[ -d "${build_dir}" ]]; then
        find "${build_dir}" -mindepth 4 -maxdepth 4 -path '*/fingerprint/*' -type f ! -name '*.json' \
            -printf '%A@\t%h\n' >"${scratch}/atimes"
        find "${build_dir}" -mindepth 4 -maxdepth 4 -path '*/fingerprint/*.json' -type f \
            -exec grep -m1 -oH '^{"rustc":[0-9]*' {} + >"${scratch}/rustcs" 2>/dev/null || true
        awk -F'\t' '
            FNR == NR {
                unit = $2
                sub(/\/fingerprint$/, "", unit)
                if (unit !~ /\/build\/[^/]+\/[0-9a-f]{16}$/) next
                if (!(unit in last) || $1 > last[unit]) last[unit] = $1
                next
            }
            {
                at = index($0, "/fingerprint/")
                unit = substr($0, 1, at - 1)
                hash = $0
                sub(/.*:\{"rustc":/, "", hash)
                rustc[unit] = hash
            }
            END {
                for (unit in last) printf "%s\t%s\t%s\n", last[unit], rustc[unit], unit
            }' "${scratch}/atimes" "${scratch}/rustcs" >"${units}"
    fi
    # 4a applies only where the pinned toolchain has built something (the
    # mis-probe guard); 4b ages out whatever 4a doesn't take.
    by_toolchain=""
    if [[ -n "${pinned_rustc}" ]]; then
        if awk -F'\t' -v pinned="${pinned_rustc}" \
            '$2 == pinned { found = 1 } END { exit !found }' "${units}"; then
            by_toolchain="${pinned_rustc}"
        elif [[ -s "${units}" ]]; then
            echo "  ${rel}: no unit built by the pinned toolchain yet — other-toolchain units kept" \
                "(build once with the pin, then re-run)"
        fi
    fi
    awk -F'\t' -v pinned="${by_toolchain}" -v cutoff="${cutoff}" -v aged="${use_atime}" \
        -v stale="${scratch}/toolchain-stale" -v unused="${scratch}/unused" '
        BEGIN { printf "" >stale; printf "" >unused }
        pinned != "" && $2 != "" && $2 != pinned { printf "%s%c", $3, 0 >stale; next }
        aged == 1 && $1 < cutoff { printf "%s%c", $3, 0 >unused }' "${units}"
    prune_list "${scratch}/toolchain-stale" "${rel} units from another toolchain" "unit dirs"
    prune_list "${scratch}/unused" "${rel} units unused for ${age_days}+ days" "unit dirs"
    if [[ "${dry_run}" -eq 0 && -d "${build_dir}" ]]; then
        find "${build_dir}" -mindepth 1 -maxdepth 1 -type d -empty -delete
    fi

    # 5. Pre-1.100 leftovers — only once the pinned cargo writes the new layout.
    if [[ "${pinned_new_layout}" == 1 ]]; then
        leftovers="${scratch}/leftovers"
        : >"${leftovers}"
        for old in deps .fingerprint; do
            [[ -e "${profile_dir}/${old}" ]] && printf '%s\0' "${profile_dir}/${old}" >>"${leftovers}"
        done
        if [[ -d "${build_dir}" ]]; then
            find "${build_dir}" -mindepth 1 -maxdepth 1 -type d -regextype posix-extended \
                -regex '.*-[0-9a-f]{16}' -print0 >>"${leftovers}"
        fi
        if [[ -d "${profile_dir}/examples" ]]; then
            find "${profile_dir}/examples" -mindepth 1 -maxdepth 1 -type f -regextype posix-extended \
                -regex '.*-[0-9a-f]{16}([.].*)?' -print0 >>"${leftovers}"
        fi
        prune_list "${leftovers}" "${rel} pre-1.100 layout" "paths"
    fi

    exec {lock_fd}>&-
done

size_after=$(du_bytes "${target_dir}")
if [[ "${dry_run}" -eq 1 ]]; then
    echo "prune-target-cache: dry run — no changes made. Re-run without --dry-run to apply."
else
    freed=$((size_before - size_after))
    echo "prune-target-cache: $(human "${size_before}") -> $(human "${size_after}")" \
        "(freed $(human "${freed}"))"
fi
