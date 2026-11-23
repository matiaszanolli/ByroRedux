# #7349: SAVE-04: The load's profile diff is computed against a profile the reload just rebuilt from the save's own plugin list, so plugin additions, removals and reorders can never be reported

- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates (profile capture/compare, d3b208048 / #6670)
- **Data-Loss Class**: none (diagnostic only; no saved state is lost by this)
- **Location**: `byroredux/src/cell_loader/load_order.rs:381` (`install_form_resolver` -> `world.insert_resource(session_profile(&load_order))`), reached from `byroredux/src/cell_loader/load.rs:590` (`InteriorCellApplyJob::begin` -> `PreviousFormResolver::install`) with `plugin_paths = masters + esm_path` taken from the save's `CurrentCellContext` (`byroredux/src/save_io/live_load.rs:163,194`); compared at `live_load.rs:995` (`save_profile_lines`); claim at `byroredux/src/profile_fingerprint.rs:10-12` and `save_io/registry.rs:410-411`.
- **Status**: NEW (not covered by #6670 / #6676: #6676 is the LMC-SAV-002..004 *consequences*, not the report that triggers them)
- **Description**: `profile_fingerprint.rs` promises "loading a save compares the record the save carries with the session's and reports every difference ... while the session keeps its own: a load never changes the profile". But the live load (and a `--load` at startup) does not keep the session's profile: it reloads the cell from the *save's* plugin set (`cell_ctx.masters` + `esm_path`), and every load-order install goes through `install_form_resolver`, which overwrites `SessionProfile` with `session_profile(&load_order)` built from that list. The post-reload comparison therefore diffs the saved record against a record whose `plugins` are, by construction, the save's own plugins. `differences()` lines `plugin X: in the save's profile, absent now`, `plugin X: added since the save` and `load order changed` cannot fire; only a plugin's content-hash drift, mod roots, extensions, grants and record policy (which `session_profile` still reads from the current process) can.
- **Evidence**:
  ```rust
  // load_order.rs:377-382
  world.insert_resource(resolver);
  world.insert_resource(crate::profile_fingerprint::session_profile(&load_order));
  // load.rs:565-590: plugin_paths = masters (the save's) + esm_path
  // live_load.rs:995: let profile_lines = save_profile_lines(world, &snapshot);   // AFTER the reload
  ```
  The only test (`a_save_carries_its_profile_and_a_load_keeps_the_sessions`) calls `restore_resources` on a hand-built world; no test drives a reload with a different plugin set, which is why the masking is unpinned.
- **Impact**: A player who adds a mod and loads an older save gets the save's plugin set silently (the new mod is not in the reloaded session) and no "added since the save" notification; a removed plugin fails the load earlier (parse error) rather than through the profile report. The next save then records the save's plugin set as the session profile. LMC-SAV-002..004 will hang their load reports off this diff, so it has to see the pre-load profile.
- **Related**: #6670 (Phase 1), #6676 (Phase 7 consequences), `docs/prd/legacy-mod-compatibility.md` LMC-SAV-001.
- **Suggested Fix**: capture the session's `SessionProfile` before `reload_*_session` (the drain already captures `LiveReloadLedgers`/`identity_before`) and diff the saved record against that; restore it after the reload if "the session keeps its own" is the intended contract. Add a live-reload test with a differing plugin list.
- **Dimension agent ID**: SAVE-D4-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
