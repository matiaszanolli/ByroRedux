# FNV-D8-NEW-01: no aggregate zero-archives-opened guard

**Severity**: LOW · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D8-NEW-01)
**Location**: `byroredux/src/asset_provider/archive.rs` (`open_with_numeric_siblings`); `byroredux/src/asset_provider/texture.rs` (`build_texture_provider`); call sites in `byroredux/src/scene.rs` and `byroredux/src/main.rs`
**Status**: NEW (delta on prior AUDIT_FNV_2026-06-14 §D8-02, which verified the gotcha and proposed but never filed the `--esm`-parent fallback)

## Description
A bare/relative `--bsa`/`--textures-bsa` value that fails to resolve against CWD produces only a per-archive `log::warn!` (`archive.rs:313`); `open_with_numeric_siblings` returns without aborting and the engine proceeds with empty archive vectors. The cell/interior load then succeeds with placeholder textures. There is **no aggregate guard** distinguishing "0 mesh archives opened" (a misconfigured run) from a legitimate load, so a bench prints a spurious FPS (the ROADMAP repro: ~36 entities / ~1792 FPS) that can be mistaken for a real number. The `--game <key>` profile path (joins names against the resolved Steam Data dir) is immune; smoke scripts use absolute paths and are immune.

## Evidence
`open_with_numeric_siblings` (`archive.rs:306`) warns-and-returns on `Archive::open` error (`:313`, sibling `:327`). `TextureProvider::extract_mesh`/`extract` iterate a possibly-empty `Vec<Archive>` and return `None` with no caller-side "provider is empty" assertion. Repo grep finds no `mesh_archives.is_empty()` / "0 mesh archives" aggregate gate anywhere.

## Impact
UX trap / masked-failure-as-data, not a correctness defect. Blast radius: any manual (non-`--game`) bench/repro invocation run from the wrong directory or with a mistyped archive name. The happy path is unaffected.

## Related
AUDIT_FNV_2026-06-14 §D8-02; ROADMAP "Repro-command CWD note"; sibling-load #1661 (CLOSED, unrelated).

## Suggested Fix
Two cheap, independent options: (a) when a bare archive name's CWD-relative path doesn't exist, retry resolution against the `--esm` parent directory before giving up (the D8-02 QoL suggestion); and/or (b) after `build_texture_provider`, if a cell/interior/exterior load was requested but `mesh_archives.is_empty()`, emit a single `log::error!` ("requested cell load but 0 mesh archives opened — check --bsa paths / CWD") so a spurious bench is self-evident. Neither changes the happy path.

## Completeness Checks
- [ ] **SIBLING**: Apply the same "0 archives opened" check to both mesh and texture providers (and the materials-ba2 path if present)
- [ ] **TESTS**: A test that requests a cell load with an unresolvable archive name asserts the loud error/guard fires (not a silent near-empty load)
