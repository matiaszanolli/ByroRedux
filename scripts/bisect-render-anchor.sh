#!/usr/bin/env bash
# Build the current checkout and classify it against one immutable renderer
# reference. Intended as the command passed to `git bisect run`.
#
# Usage:
#   git bisect run scripts/bisect-render-anchor.sh REF_BIN [OUT_DIR] [FRAMES]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -lt 1 || $# -gt 3 ]]; then
    echo "usage: $0 REF_BIN [OUT_DIR] [FRAMES]" >&2
    exit 2
fi

reference_bin="$(realpath -- "$1")"
artifact_dir="${2:-$repo_root/target/renderer-anchor-bisect/current}"
frames="${3:-60}"

if [[ ! -x "$reference_bin" ]]; then
    echo "reference binary is not executable: $reference_bin" >&2
    exit 2
fi

cd "$repo_root"
cargo build -p byroredux --bin byroredux
candidate_bin="$(realpath -- "$repo_root/target/debug/byroredux")"
if [[ "$reference_bin" == "$candidate_bin" ]]; then
    echo "reference must be immutable and distinct from target/debug/byroredux" >&2
    exit 2
fi

exec "$repo_root/scripts/check-render-anchor.sh" \
    "$reference_bin" "$candidate_bin" "$artifact_dir" "$frames"
