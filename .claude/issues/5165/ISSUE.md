# #5165 — TOOL-D1-2026-10-02-01: The cell-load debug requests still open client-chosen absolute ESM and archive paths after #4752 closed (residual of TOOL-D1-2026-09-29-05)

Labels: low,tech-debt,safety,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5165

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Debug Trust Boundary
- **Exposure**: any local process on a debug build, or on a release build with `BYRO_DEBUG_SERVER=1`. It is read-only and same-user.
- **Location**: `crates/debug-server/src/evaluator.rs:91-130`; `byroredux/src/debug_load.rs:52-99` (dispatch), `:293+` (`exec_load_interior` → `cell_loader::load_cell_with_masters`), and the exterior twin
- **Status**: NEW. This is the unfixed residual of closed #4752. The 09-29 report recorded it as D1-05 under #4752 and did not file it separately, so it was lost when #4752 closed.
- **Description**: `LoadInteriorCell` and `LoadExteriorCell` copy the client's `esm`, `masters`, `bsas` and `textures_bsas` strings into `PendingDebugLoad`. They are then opened as given. Only `LoadNif` goes through `resolve_nif_bytes`' rule: relative paths, confined under `allowed_roots` derived from the startup `--esm`/`--bsa` args (`debug_load.rs:151-185`). The #4752 close comment says `63c0aee3b` "covers all three independent paths". The cell-load paths are a fourth.
- **Evidence**: `evaluator.rs:98-104` builds `PendingDebugLoad::InteriorCell { esm: esm.clone(), masters: masters.clone(), bsas: bsas.clone(), … }` with no path check. `debug_load.rs` passes `DebugLoadSource { esm: &esm, … }` straight to the loader.
- **Impact**: Any local process can point the ESM, BSA and BA2 parsers (untrusted-input code) at any file the user can read. It is read-only, but it keeps a parser attack surface reachable without authentication on every debug build.
- **Related**: #4752, TOOL-D1-2026-09-29-05, #5009 (same file, `resolve_nif_bytes` archive order)
- **Suggested Fix**: Apply `resolve_nif_bytes`' root rule to every cell-load path: relative only, canonicalized under the startup roots. Alternatively, restrict cell loads to the startup archive set, as `tex.dump` does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

