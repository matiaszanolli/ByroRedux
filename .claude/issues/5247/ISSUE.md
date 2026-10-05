# #5247 — SAVE-D3-2026-10-05-01: write_slot is the one durable writer #5143/#5164 left on the old shape — fixed save_<slot>.ess.tmp temp shared across processes, no assert_no_clobber_fallback pin

- **Labels**: low,save-load,bug
- **Filed from**: `docs/audits/AUDIT_SAVE_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5247

- **Severity**: LOW
- **Dimension**: Container & Disk Durability
- **Data-Loss Class**: irrecoverable-write (multi-process only; the CRC rejects the result on load)
- **Location**:
  - `crates/save/src/disk.rs:38-46` (`write_slot`: `final_path.with_extension("ess.tmp")`), `:5-8` (module doc: "a stray `.tmp` that the next save overwrites");
  - `crates/core/src/atomic_file.rs:19-25` (`atomic_temp_path`), `:93-121` (`assert_no_clobber_fallback`, used by `settings-io`, `boot-request` and `game-detect` only).
- **Status**: NEW. Related to #5143, #5163 and #5164 (closed), which hardened the other three writers, and #3472 (closed).
- **Description**:
  - #5143 gave every durable writer a unique hidden `.{name}.{pid}.{n}.tmp`. #5164 pinned each writer's production text against a clobbering fallback.
  - The save ring is the most valuable file these writers handle, yet `write_slot` still stages through a fixed sibling name and carries no pin.
  - Two engine processes that share a save directory (the default `<cwd>/saves`, with both ring cursors resumed independently from the same mtimes) can quicksave into the same slot and share one temp path:
    - A's `stage_and_rename` can pass read-back, then rename the inode that B has just re-created and is still writing;
    - the slot then holds B's bytes, or a torn file if B fails mid-write.

  The CRC refuses a torn file on load, and quickload falls back past it. The previous good save in that slot is gone either way.
- **Impact**: a lost slot when two instances run against one save directory. The memory note *no parallel engine launch* records that this happens in practice; the smoke gates avoid it only through `BYROREDUX_SAVE_DIR`. A future fallback branch in `write_slot` would also go uncaught.
- **Suggested Fix**: stage through `atomic_temp_path(&final_path)` and add an `assert_no_clobber_fallback(include_str!("disk.rs"))` test beside the other three. Update the module doc, since the temp is now removed on failure and no longer overwritten by the next save.

_Source: `AUDIT_SAVE_2026-10-05.md` (SAVE-D3-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
