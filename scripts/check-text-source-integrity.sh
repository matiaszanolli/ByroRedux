#!/usr/bin/env bash
# Reject raw NUL bytes in tracked source/documentation files. Rust accepts a
# literal NUL inside a byte string, but traditional grep then classifies the
# entire file as binary and silently hides regression guards from audit tools.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

failed=0
while IFS= read -r -d '' source; do
    if rg --text --quiet '\x00' -- "${source}"; then
        echo "NUL ${source}" >&2
        failed=1
    fi
done < <(
    git ls-files -z -- \
        '*.rs' '*.md' '*.glsl' '*.vert' '*.frag' '*.comp' '*.sh' '*.toml' \
        '*.yml' '*.yaml'
)

if [[ "${failed}" -ne 0 ]]; then
    echo "check-text-source-integrity: replace raw NUL bytes with source escapes (for example, \\0)" >&2
    exit 1
fi

echo "check-text-source-integrity: tracked text sources contain no raw NUL bytes"
