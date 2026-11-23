# #7313: SCR-D7-03: the `feature-matrix.md` extender row still says "unaudited" with 2026-09-10 LOC figures

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer (doc-rot)
- **Untrusted-Input**: No
- **Location**: `docs/feature-matrix.md:215`
- **Status**: NEW (#3953 closed the "silent row" gap and added this row; it has not been refreshed since)
- **Description**: The row says "~23.9k LOC across `crates/sdk/src/` (14 245), `papyrus_provider/` (6 377) and `compatibility.rs` + `obscript*.rs` (3 294), measured 2026-09-10. No dedicated audit pass has run over it". Dim 7 has been a dedicated pass since the 2026-09-14 skill revision, and AUDIT_SCRIPTING_2026-10-11/-10-17 recorded Dim 7 findings. The current live counts are `crates/sdk/src` 14 701, `papyrus_provider/` 7 634 (8 743 with `compatibility.rs`), and `obscript*.rs` has since grown (the 3 294 figure is not comparable).
- **Evidence**: `docs/feature-matrix.md:215` (quoted above); `find crates/scripting/src/papyrus_provider -name '*.rs' | xargs wc -l` gives 7 634.
- **Impact**: The status floor understates audit coverage and the LOC are stale. Doc rot only.
- **Related**: #3953 (closed).
- **Suggested Fix**: Re-measure the LOC and replace "No dedicated audit pass has run over it" with a pointer to `/audit-scripting` Dim 7. Keep the "not verified against real mod content" caveat.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
