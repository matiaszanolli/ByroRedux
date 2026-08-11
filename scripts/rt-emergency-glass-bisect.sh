#!/usr/bin/env bash
# Deterministic emergency predicate for the Skyrim Arch-Mage alchemy-glass
# regression. Intended for `git bisect run` in an isolated worktree.

set -uo pipefail

script_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="${BYROREDUX_RT_BISECT_REPO:-${script_repo}}"
data="${BYROREDUX_SKYRIM_DATA:-/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data}"
artifact_root="${BYROREDUX_RT_BISECT_OUT:-/tmp/byroredux-rt-glass-bisect}"
frames="${BYROREDUX_RT_BISECT_FRAMES:-60}"
good_luma="${BYROREDUX_RT_BISECT_GOOD_LUMA:-0.30}"
png="${artifact_root}/frame.png"
log="${artifact_root}/engine.log"

mkdir -p "${artifact_root}"
rm -f "${png}" "${log}"

if [[ ! -f "${data}/Skyrim.esm" ]]; then
    echo "rt-glass-bisect: Skyrim.esm not found under ${data}" >&2
    exit 125
fi

if ! cargo build --manifest-path "${repo}/Cargo.toml" --release \
    -p byroredux --bin byroredux; then
    echo "rt-glass-bisect: build failed; skipping revision" >&2
    exit 125
fi

engine="${repo}/target/release/byroredux"
(
    cd "${data}" || exit 125
    RUST_LOG=error xvfb-run -a \
        env -u WAYLAND_DISPLAY -u GDK_BACKEND XDG_SESSION_TYPE=x11 \
        "${engine}" \
        --esm Skyrim.esm \
        --cell WinterholdCollegeArchMageQuarters \
        --bsa "Skyrim - Meshes0.bsa" \
        --bsa "Skyrim - Meshes1.bsa" \
        --textures-bsa "Skyrim - Textures0.bsa" \
        --textures-bsa "Skyrim - Textures1.bsa" \
        --textures-bsa "Skyrim - Textures2.bsa" \
        --textures-bsa "Skyrim - Textures3.bsa" \
        --textures-bsa "Skyrim - Textures4.bsa" \
        --textures-bsa "Skyrim - Textures5.bsa" \
        --textures-bsa "Skyrim - Textures6.bsa" \
        --textures-bsa "Skyrim - Textures7.bsa" \
        --textures-bsa "Skyrim - Textures8.bsa" \
        --fly \
        --camera-pos -556.74,132.16,364.16 \
        --camera-forward -1,-0.10,0 \
        --bench-frames "${frames}" --bench-mode renderer-static \
        --upscaler taa \
        --screenshot "${png}"
) >"${log}" 2>&1
capture_status=$?

if (( capture_status != 0 )) || [[ ! -s "${png}" ]]; then
    echo "rt-glass-bisect: capture failed (exit ${capture_status}); skipping revision" >&2
    tail -n 30 "${log}" >&2 || true
    exit 125
fi

if ! command -v convert >/dev/null 2>&1; then
    echo "rt-glass-bisect: ImageMagick convert is required" >&2
    exit 125
fi

# The 80x80 crop is wholly inside the central authored glass sphere at the
# fixed 1280x720 camera. Three-run calibration on 2026-08-10:
#   ec8d924d (good): 0.493765, 0.493765, 0.493765
#   65cc29d6 (bad):  0.091602, 0.091600, 0.091598
# A 0.30 split is over 0.19 away from either cluster.
luma="$(convert "${png}" -crop 80x80+610+480 -colorspace gray \
    -format '%[fx:mean]' info:)"
bench="$(awk '/^bench:/{line=$0} END{print line}' "${log}")"

echo "rt-glass-bisect: revision=$(git -C "${repo}" rev-parse --short=12 HEAD) luma=${luma} threshold=${good_luma}"
[[ -n "${bench}" ]] && echo "${bench}"

if awk -v observed="${luma}" -v threshold="${good_luma}" \
    'BEGIN { exit !(observed >= threshold) }'; then
    echo "rt-glass-bisect: GOOD"
    exit 0
fi

echo "rt-glass-bisect: BAD"
exit 1
