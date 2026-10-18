# Releasing

ByroRedux publishes **Linux x86_64 alpha binaries** as GitHub pre-releases, one
at the close of each working session. Releases are named after the session:
`v0.<session number>` (Session 101 -> `v0.101`). The Cargo workspace version is
not bumped; the tag is the release identity, and the binaries report it
(`byroredux --version` -> `byroredux 0.1.0 (v0.101, <commit>)`).

## What a release contains

`byroredux-<tag>-linux-x86_64.tar.gz` (plus `SHA256SUMS`), holding one folder:

| File | Purpose |
|---|---|
| `byroredux` | The engine. |
| `byro-launcher` | Resident window that detects installed games and starts the engine. |
| `byro-detect` | Prints the detected game installs. |
| `byro-dbg` | Console client for a running engine (`BYRO_DEBUG_SERVER=1` on the engine). |
| `assets/` | `debug_profiles.toml` and `graphics_presets.toml`. Read **relative to the current directory**, so the binaries are run from this folder. |
| `README.txt` | Requirements and how to run (from [`packaging/linux/README.txt`](../packaging/linux/README.txt)). |
| `LICENSE`, `THIRD_PARTY_NOTICES.md`, `third_party/.../LICENSE.txt` | Licences, including the vendored FidelityFX SDK. |

No game data is included or ever will be.

## Cutting a release

1. Close the session (`/session-close`) and push; wait for CI on the final commit
   to go green. The release workflow refuses a commit whose CI has not succeeded.
2. Tag that commit and push the tag:

   ```bash
   git tag -a v0.101 -m "Session 101"
   git push origin v0.101
   ```

3. [`.github/workflows/release.yml`](../.github/workflows/release.yml) builds with
   the pinned toolchain on `ubuntu-22.04` (glibc 2.35 is the floor for the
   binaries), runs `--version`/`--help`/`ldd` smoke checks that need no GPU,
   packages with [`scripts/package-release.sh`](../scripts/package-release.sh),
   and publishes the pre-release with notes and the changelog since the previous
   `v0.*` tag.

To rehearse without publishing, run the workflow manually
(`gh workflow run release.yml -f tag=v0.101 -f publish=false`): the tarball is
uploaded as a workflow artifact instead of a release. `scripts/package-release.sh
--self-test` checks the archive layout and reproducibility, and runs in CI.

## What is *not* verified by the pipeline

CI has no GPU. The release job proves the binaries link and start far enough to
print their version; it cannot prove the renderer runs. Run the new build on real
hardware before announcing it. Known alpha limitations (asset lookup relative to
the working directory, ray-query hardware required) are listed in the release's
`README.txt`.
