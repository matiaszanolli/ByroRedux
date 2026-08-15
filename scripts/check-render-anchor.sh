#!/usr/bin/env bash
# Compare two already-built ByroRedux binaries on deterministic renderer paths.
#
# Usage:
#   scripts/check-render-anchor.sh REF_BIN CANDIDATE_BIN [OUT_DIR] [FRAMES]
#
# Optional environment:
#   BYROREDUX_ANCHOR_PATHS=static,pan,orbit,dolly,cut
#   BYROREDUX_ANCHOR_SCENE_ARGS_JSON='["--cornell"]'
#   BYROREDUX_ANCHOR_XVFB=0                      # use an existing display
#   BYROREDUX_ANCHOR_TEST_PERTURB=magenta-block   # proves the gate can fail
#   BYROREDUX_ANCHOR_* threshold overrides documented in renderer_anchor.rs
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ $# -lt 2 || $# -gt 4 ]]; then
    echo "usage: $0 REF_BIN CANDIDATE_BIN [OUT_DIR] [FRAMES]" >&2
    exit 2
fi

reference_source="$(realpath -- "$1")"
candidate_source="$(realpath -- "$2")"
artifact_dir="${3:-$repo_root/target/renderer-anchor}"
frames="${4:-60}"

if [[ ! -x "$reference_source" ]]; then
    echo "reference binary is not executable: $reference_source" >&2
    exit 2
fi
if [[ ! -x "$candidate_source" ]]; then
    echo "candidate binary is not executable: $candidate_source" >&2
    exit 2
fi
if [[ ! "$frames" =~ ^[1-9][0-9]*$ ]]; then
    echo "frames must be a positive integer, got: $frames" >&2
    exit 2
fi
mkdir -p "$artifact_dir"
artifact_dir="$(realpath -- "$artifact_dir")"
mkdir -p "$artifact_dir/binaries"
reference_copy="$artifact_dir/binaries/reference-byroredux"
candidate_copy="$artifact_dir/binaries/candidate-byroredux"

# Cargo may rebuild `target/debug/byroredux` while compiling the integration
# harness. Stage explicit paths so neither side can change underneath a run.
# Prefer hard links (a linker replacement gets a new inode, so the staged one
# remains stable) and fall back to copy-on-write/copy across filesystems.
stage_binary() {
    local source="$1"
    local destination="$2"
    if [[ "$source" == "$(realpath -m -- "$destination")" ]]; then
        return
    fi
    if ! ln -f -- "$source" "$destination" 2>/dev/null; then
        cp -p --reflink=auto -- "$source" "$destination"
    fi
}

stage_binary "$reference_source" "$reference_copy"
stage_binary "$candidate_source" "$candidate_copy"

export BYROREDUX_ANCHOR_REFERENCE_BIN="$reference_copy"
export BYROREDUX_ANCHOR_CANDIDATE_BIN="$candidate_copy"
export BYROREDUX_ANCHOR_OUT="$artifact_dir"
export BYROREDUX_ANCHOR_WORKDIR="$repo_root"
export BYROREDUX_ANCHOR_FRAMES="$frames"
export BYROREDUX_ANCHOR_XVFB="${BYROREDUX_ANCHOR_XVFB:-1}"

cd "$repo_root"
exec cargo test -p byroredux --test renderer_anchor \
    reference_binary_matches_candidate_on_every_camera_path -- \
    --ignored --exact --nocapture
