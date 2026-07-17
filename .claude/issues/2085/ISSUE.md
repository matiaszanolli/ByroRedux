# FNV-D8-03: CLAUDE.md references asset_provider.rs as a single file; it's now a directory

- **Severity**: LOW
- **Labels**: low, import-pipeline, documentation
- **Location**: `CLAUDE.md` workspace-structure tree ("Asset Provider" row), `CLAUDE.md:282`

## Description
`byroredux/src/asset_provider.rs` was split into `byroredux/src/asset_provider/{archive,material,mod,script,texture,tests}.rs`; `CLAUDE.md` still documents it as one file (`src/asset_provider.rs    TextureProvider, BSA texture/mesh extraction, resolve_texture`, plus an inline reference at line 282). The CWD-relative path-resolution behavior it documents was independently confirmed accurate at the code level (`Archive::open`, no directory-joining against `--esm`) — this is a doc-rot finding only, not a behavior bug.

## Evidence
`CLAUDE.md`'s workspace-structure tree lists `src/asset_provider.rs` as a single file; `byroredux/src/asset_provider.rs` does not exist on disk — `byroredux/src/asset_provider/` is a directory containing `archive.rs`, `material.rs`, `mod.rs`, `script.rs`, `texture.rs`, `tests.rs`.

## Suggested Fix
Update the workspace-structure row and inline reference to point at the directory (or `archive.rs`/`texture.rs` specifically).

## Completeness Checks
- [ ] **SIBLING**: Check CLAUDE.md for other single-file→directory splits not yet reflected (common ByroRedux pattern per session-close notes)
