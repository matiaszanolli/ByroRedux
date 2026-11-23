# #7354: SAVE-09: Staging temps are never reclaimed after a process death, and `atomic_write`'s cleanup is an `Err` branch, not an unwind guard

- **Severity**: LOW
- **Dimension**: Container & Disk
- **Data-Loss Class**: none (no save is lost; hidden files accumulate)
- **Location**: `crates/core/src/atomic_file.rs:58-66` (cleanup only in `if let Err(error) = stage_and_rename(..)`), `:233` (`fs::File::create(tmp_path)`), `crates/save/src/disk.rs:47-52` (`write_slot` stages `.save_<slot>.ess.<pid>.<n>.tmp`); no reaper anywhere (`rg '\.tmp'`/"sweep" over `byroredux/src` and the four writer crates finds none).
- **Status**: NEW (#5163 removed the temp on every *returned* error; #5247 made the name unique per attempt, which turned the old overwrite-on-next-save behaviour into permanent leftovers)
- **Description**: A power cut or kill between `create` and `rename` (the case the module docs design for: "A power cut mid-write leaves the old save intact") leaves `.save_N.ess.<pid>.<n>.tmp` holding up to a full snapshot. The pre-#5247 fixed `save_<slot>.ess.tmp` was overwritten by the next save of that slot; the unique name never repeats, and `parse_slot_filename` / `slots_by_recency` / `SaveRing::resume` correctly ignore it (pinned by `newest_slot_ignores_newer_tmp_and_empty_directory`), so nothing ever deletes it. The same holds for the settings, boot-request, overrides, terrain-height cache (`asset_provider/terrain_height.rs:205`) and texture-upscale writers. A panic unwinding through `stage_and_rename` would also skip the cleanup (no `Drop` guard), although the same file now has `TempPath` for exactly that class (#7158) and no reachable panic exists on the current path. Separately, `File::create` (not `create_new`) follows a planted symlink at the staging name; the #7143 exclusive-creation work covered `create_temp_file`/`write_new_*` only. With pid + a per-process counter starting at 0, a post-reboot pid reuse can truncate a stale temp, which is harmless.
- **Evidence**: `if let Err(error) = stage_and_rename(final_path, tmp_path, bytes) { let _ = fs::remove_file(tmp_path); return Err(error); }`; `write_slot` has no sweep of `.save_*.tmp` before or after.
- **Impact**: Disk-space leak of one save per crash, invisible to the player. No durability or correctness impact on live slots.
- **Related**: #5163, #5247, #7143, #7158.
- **Suggested Fix**: in `write_slot` (and `SaveState::new`) best-effort delete sibling `.save_*.ess.*.tmp` files whose pid is not alive or older than a minute; optionally stage through `create_new`.
- **Dimension agent ID**: SAVE-D3-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
