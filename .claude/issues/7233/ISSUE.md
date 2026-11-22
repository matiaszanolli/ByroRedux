# #7233: GAME-2026-11-22-10: A tree package's leaf is chosen once per package win; leaf conditions are never re-read while that package keeps winning

**Labels**: low,gameplay,ai,bug,game:skyrim,game:fo4,game:starfield

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-10**), audited at HEAD 2b40f76b6.

- **Severity**: LOW
- **Dimension**: 5 (package selection)
- **Domain**: ai · **Game**: Skyrim, FO4, Starfield
- **Trigger**: A tree package whose leaves carry their own conditions (116 of 1,627 vanilla Skyrim leaves, per the `tree.rs` module doc), with a condition that flips while the package stays the winner.
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:1463-1500` (behaviour recomputed every pass, installed only on `changed`); `byroredux/src/npc_spawn/ai_package/tree.rs:93-110`
- **Status**: NEW (low confidence on the original's re-evaluation cadence; a candidate, not a proven misroute)
- **Description**: `ambient_ai_package_system` re-runs `from_package`, and therefore `tree_decision`, each in-game minute, but installs the result only when the winning PACK FormID changes. A leaf whose conditions flip is never swapped. A package whose leaves all failed at install stays with no behaviour after they later pass.
- **Evidence**: `if !changed { … continue; }` at `:1491-1497` discards the recomputed behaviour.
- **Impact**: A stale leaf choice until the next package change.
- **Related**: aee4fab7a, #6660
- **Suggested Fix**: When the winner is unchanged and its template is a tree with conditioned leaves, compare the freshly chosen leaf with the installed one and re-install if it differs.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
