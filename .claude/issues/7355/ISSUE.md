# #7355: SAVE-10: Two `atomic_write` callers carry no clobber-fallback pin, and the SKILL's "three non-save callers" is stale

- **Severity**: INFO
- **Dimension**: Container & Disk
- **Data-Loss Class**: none
- **Location**: `byroredux/src/asset_provider/terrain_height.rs:203-208` (`write_atomic`), `tools/texture-upscale/src/lib.rs:208-209` (`write_manifest`, #7192 `f561ceedf`); pins exist only in `crates/settings-io/src/lib.rs:800`, `crates/boot-request/src/lib.rs:349`, `crates/game-detect/src/overrides.rs:185`, `crates/save/src/disk.rs` (`write_slot_has_no_clobber_fallback`).
- **Status**: NEW (guard-ledger gap, not a defect)
- **Description**: both files call `atomic_temp_path` + `atomic_write` correctly and have no fallback today, but nothing asserts it; a fallback re-grown there would be the #5143 hazard (copying a partial temp over a good file) with no test to catch it. `assert_no_clobber_fallback` is `include_str!`-per-file, so it has to be added per caller.
- **Suggested Fix**: add the one-line `assert_no_clobber_fallback(include_str!(...))` test to both (the `terrain_height.rs` writer binds `temp_path` once and passes it once, so the guard applies as-is), and update the SKILL's caller list.

---
- **Dimension agent ID**: SAVE-D3-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
