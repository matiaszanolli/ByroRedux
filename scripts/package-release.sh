#!/usr/bin/env bash
# Assemble the Linux release tarball from an already-built `target/release`.
#
#   scripts/package-release.sh <tag> [out-dir]
#
# Produces, in <out-dir> (default `dist/`):
#   byroredux-<tag>-linux-x86_64.tar.gz
#   SHA256SUMS
# and prints the tarball path on the last line. The tarball holds one
# directory, `byroredux-<tag>-linux-x86_64/`, with the binaries, `assets/`
# (read relative to the working directory at runtime), the licences and
# `packaging/linux/README.txt` as `README.txt`.
#
# `--self-test` packages a fake `target/release` in a temp dir and checks the
# layout, so CI can run it without a full build.

set -euo pipefail

BINARIES=(byroredux byro-launcher byro-detect byro-dbg)
ASSET_FILES=(assets/debug_profiles.toml assets/graphics_presets.toml)
DOC_FILES=(LICENSE THIRD_PARTY_NOTICES.md)
VENDORED_LICENSES=(third_party/fidelityfx-sdk-v1.1.4/LICENSE.txt)

package() {
    local root="$1" tag="$2" out="$3" target_dir="$4"
    local name="byroredux-${tag}-linux-x86_64"
    local stage
    stage="$(mktemp -d)"
    trap 'rm -rf "${stage}"' RETURN
    mkdir -p "${stage}/${name}/assets" "${out}"

    for bin in "${BINARIES[@]}"; do
        local src="${target_dir}/${bin}"
        [[ -x "${src}" ]] || { echo "package-release: missing ${src} (build with --release first)" >&2; return 1; }
        install -m 0755 "${src}" "${stage}/${name}/${bin}"
    done
    for file in "${ASSET_FILES[@]}"; do
        install -m 0644 "${root}/${file}" "${stage}/${name}/${file}"
    done
    for file in "${DOC_FILES[@]}"; do
        install -m 0644 "${root}/${file}" "${stage}/${name}/${file}"
    done
    for file in "${VENDORED_LICENSES[@]}"; do
        mkdir -p "${stage}/${name}/$(dirname "${file}")"
        install -m 0644 "${root}/${file}" "${stage}/${name}/${file}"
    done
    install -m 0644 "${root}/packaging/linux/README.txt" "${stage}/${name}/README.txt"

    # Sorted names, fixed owner and mtime: the same inputs give the same bytes.
    # Explicit status checks: `package` runs under `$(...)`, where `set -e` is off.
    local mtime="${SOURCE_DATE_EPOCH:-0}" tar_out="${out}/${name}.tar.gz"
    ( cd "${stage}" && find "${name}" -print0 | LC_ALL=C sort -z \
        | tar --null --no-recursion --owner=0 --group=0 --numeric-owner \
              --mtime="@${mtime}" -T - -cf - | gzip -9n ) > "${tar_out}" \
        || { echo "package-release: could not build ${tar_out}" >&2; return 1; }
    tar -tzf "${tar_out}" >/dev/null || { echo "package-release: ${tar_out} is not a readable tarball" >&2; return 1; }
    ( cd "${out}" && sha256sum "${name}.tar.gz" > SHA256SUMS ) || return 1
    echo "${out}/${name}.tar.gz"
}

self_test() {
    root="$(cd "$(dirname "$0")/.." && pwd)"
    fake="$(mktemp -d)"; out="$(mktemp -d)"
    # shellcheck disable=SC2064 -- expand now: the variables are gone at EXIT.
    trap "rm -rf '${fake}' '${out}'" EXIT
    for bin in "${BINARIES[@]}"; do printf '#!/bin/sh\n' > "${fake}/${bin}"; chmod +x "${fake}/${bin}"; done
    local tarball
    tarball="$(package "${root}" v0.0-selftest "${out}" "${fake}")" || exit 1
    tarball="$(tail -n1 <<<"${tarball}")"
    local listing
    listing="$(tar -tzf "${tarball}")"
    for want in byro-launcher byroredux byro-detect byro-dbg assets/debug_profiles.toml \
                assets/graphics_presets.toml LICENSE THIRD_PARTY_NOTICES.md README.txt \
                third_party/fidelityfx-sdk-v1.1.4/LICENSE.txt; do
        grep -q "/${want}\$" <<<"${listing}" || { echo "self-test: ${want} missing from the tarball" >&2; exit 1; }
    done
    ( cd "${out}" && sha256sum --check --quiet SHA256SUMS ) || { echo "self-test: SHA256SUMS does not verify" >&2; exit 1; }
    local again
    again="$(package "${root}" v0.0-selftest "${out}/again" "${fake}")" || exit 1
    again="$(tail -n1 <<<"${again}")"
    cmp -s "${tarball}" "${again}" || { echo "self-test: the tarball is not reproducible" >&2; exit 1; }
    echo "package-release self-test ok"
}

if [[ "${1:-}" == "--self-test" ]]; then
    self_test
    exit 0
fi

tag="${1:?usage: package-release.sh <tag> [out-dir]}"
root="$(cd "$(dirname "$0")/.." && pwd)"
package "${root}" "${tag}" "${2:-${root}/dist}" "${root}/target/release"
