# #5447: PERF-D1-2026-10-08-01: `story_change_location_system` re-derives the CLOC key with String allocations and SipHash every frame, on every game

**Labels**: low,performance,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5447

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D1-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/story_events.rs:29-92` (`story_change_location_system`, `resolve_current_lctn`, `location_hash`); registration `byroredux/src/boot/schedule/update.rs:70-72,408`; cursor install `crates/scripting/src/story_manager.rs:347-359`
- **Status**: NEW. Arrived with `3ba9f1d5c` and `17fed2565` (#5366 Phases 1–2).
- **Description**: the system runs ungated in `Stage::Update` every frame once a player entity exists. Each call:
  - clones the `Arc<EsmIndex>`;
  - on an interior cell, does `cell.cell_editor_id.to_ascii_lowercase()` (a String) and a `HashMap<String, _>::get`;
  - when the cell resolves an LCTN, does `format!("{:08X}", lctn)` (a String);
  - on an exterior cell, does `format!("grid:{:?}", exterior.grid)` (a String) plus two nested std-`HashMap` lookups;
  - SipHashes the result.
  The answer changes only on a cell transition. `install_story_manager` inserts `StoryLocationCursor` unconditionally, so FO3, FNV and Oblivion, which author no SM records, pay the same cost. `emit_change_location_on_key_change` then bails at the cursor compare, after the key has already been built.
- **Evidence**: `story_events.rs:40-55` (no early-out before the key derivation); `asset_provider/script.rs:582` (the install is not game-gated); `story_manager.rs:351`.
- **Impact**: 1–2 small heap allocations and 2–4 SipHash operations per frame for the life of the session. Small and permanent. No quantitative guard exists for this site.
- **Related**: #5366, #5293 (the same "answer changes on a generation, recomputed per frame" shape).
- **Suggested Fix**: return early when the `SmTree` has no nodes. Otherwise cache the derived key on a cell-context generation, or compare cheap inputs (cell identity / grid tuple) before building any String.

## Completeness Checks
- [ ] **SIBLING**: Other Story Manager per-frame systems (`story_manager_dispatch`) early-out when the `SmTree` is empty
- [ ] **TESTS**: A regression test pins this specific fix
