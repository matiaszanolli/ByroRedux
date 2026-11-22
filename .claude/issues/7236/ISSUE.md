# #7236: GAME-2026-11-22-13: `install_block_model` has no doc comment; its doc sits stacked on `install_fall_model`

**Labels**: low,gameplay,combat,doc-rot,documentation

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-13**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 4 (combat)
- **Domain**: combat · doc-rot · **Game**: all (static)
- **Trigger**: Reading the code.
- **Location**: `byroredux/src/combat.rs:618-639`
- **Status**: NEW
- **Description**: #7056 (787cdf19c) inserted `install_fall_model` between #7054's doc comment for `install_block_model` and that function. The block line now reads as the first paragraph of `install_fall_model`'s doc, and `install_block_model` has none.
- **Evidence**: Lines 618-619, "Install (or, for a game without block rules, remove) the [`BlockModel`] …", sit directly above the fall paragraph on `install_fall_model`. `install_block_model` at line 631 has no `///`.
- **Impact**: rustdoc and reader confusion; no behaviour change.
- **Related**: closed GAME-5 (006770e3a, same class), #7203
- **Suggested Fix**: Move the block line above `install_block_model`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
