# #7347: SAVE-02: A successful load whose profile differs from the save's tells the player "Load failed."

- **Severity**: MEDIUM
- **Dimension**: Live Load-Apply
- **Data-Loss Class**: none (misleading failure notice on a load that succeeded; no state lost)
- **Location**: `byroredux/src/save_io/live_load.rs:993-1001` (the call), `:113-120` (`notify_player`), `:123` (`LOAD_FAILED`)
- **Status**: NEW (introduced by d3b208048, Refs #6670, which reused a helper that f452ec0e4 / #6825 had made failure-only)
- **Description**: `notify_player` was changed by #6825 to ignore its text and push the fixed line `LOAD_FAILED`. d3b208048 then calls it from the profile-difference report on the success path, to "raise a player notification" naming the changed plugins / mod roots / grants. The text is dropped; the player sees "Load failed." for a load that completed (the drain goes on to `end_sting_layer`, `sync_body_after_load`, `apply_player_pose`).
- **Evidence**:
  ```rust
  // live_load.rs:117-120
  fn notify_player(world: &World, diagnostic: impl Into<String>) {
      let _logged_by_the_caller = diagnostic.into();
      crate::notifications::push(world, LOAD_FAILED);          // "Load failed."
  }
  // live_load.rs:995-1001
  let profile_lines = crate::profile_fingerprint::save_profile_lines(world, &snapshot);
  ...
  if let Some(summary) = profile_lines.first() {
      notify_player(world, format!("{summary} (see the log or save.info)"));   // success path
  }
  ```
  `profile_fingerprint::tests::a_save_carries_its_profile_and_a_load_keeps_the_sessions` covers the lines, not the notification; nothing pins what the player sees.
- **Impact**: any load of a save made with a different plugin set, load order, mod roots, extensions or grants (the case LMC exists for, and the common one for a modded install) shows "Load failed." over a working game. A player who believes it failed may reload an older slot or overwrite the loaded one. No data is lost by the code itself.
- **Related**: #6670 (LMC Phase 1), #6825. Open issues searched (`gh issue list --state all`: "profile notification load failed", "SessionProfile load notify"): none.
- **Suggested Fix**: push the summary through `crate::notifications::push(world, ...)` directly (engine text, add it to the native-page string inventory the `LOAD_FAILED` doc names), or drop the notification and keep the WARN + `save.info`. Add a test that a mismatching profile pushes no `LOAD_FAILED`.
- **Dimension agent ID**: SAVE-D5-01
- **Also found by**: SAVE-D2-01 (Dim 2), independently.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
