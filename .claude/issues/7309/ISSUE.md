# #7309: SCR-D6-04: `ObScriptLedger::forms` is a write-only sink behind eleven "kept" commands

- **Severity**: LOW
- **Dimension**: Legacy ObScript (ledger honesty, #6190)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_ledger.rs:64-67,187-193` (the map and `form()`); writers `obscript_more/state.rs:325-334` (`cmd_111c`: ids 0x111c `SetActorFullName`, 0x10b1 `SetCombatStyle`, 0x1171, 0x11b7), `tail.rs:227,464,510,543,728,740,796`, `actions.rs:252,263,274`
- **Status**: NEW (the closed #6190 covered five other fields; `forms` was not among them)
- **Description**: the ledger module doc says a command "whose value nothing in the engine would read back is not stored here: it declines". `set_form` has 11 production call sites (SetActorFullName/SetCombatStyle/SetTalkingActivatorActor/SetMerchantContainer, PipBoyRadio Tune, SetDisposition, MatchFaceGeometry, SetNoRumors and friends, SetCellOwnership, SetCellFullName, ForceActiveQuest, SetCellImageSpace, SetLastExtDoorActivated, SetNPCRadio). `form()` has no production caller anywhere: `rg '\.form\(' crates byroredux` outside tests finds only the accessor itself, and the field is private. The commands return `done(..)`, so `ObScriptQuestTimers`/the effect log count them as run, and the map is serialised into saves. `docs/engine/scripting.md:2690` lists `SetCombatStyle` and `SetTalkingActivatorActor` under "What stays declined", but the code stores and reports done.
- **Evidence**:
  ```rust
  // state.rs:325
  fn cmd_111c(...) -> Action { match caller(world, ctx) { Some((subject, _)) => {
      let form = form_arg(args, 0).unwrap_or(0);
      ledger_mut(world, |l| l.set_form(cmd, subject, form));
      done(format!("{cmd:#06x} {subject:08X} {form:08X}")) ...
  ```
  The `values` map has the same shape for many ids: readers exist for only eight keys (0x1212, 0x125c, 0x10d3, 0x1151, 0x116c, 0x118b, 0x10e7, 0x1129); for example 0x114e, 0x10bb, 0x10d8, 0x1156, 0x1186 and 0x124b are stored by `store(..)` and appear nowhere outside `obscript_more/` (`EnableLinkedPathPoints` is documented as such; the others are not).
- **Impact**: no gameplay effect, which is the point: a script's `SetCombatStyle` is reported as run and saved while nothing changes, so coverage metrics overstate and the doc table is wrong. Same class as #6190 / SCR-D5-2026-09-14-04.
- **Suggested Fix**: either decline each of these commands (`Action::Declined`, drop the `forms` map and its save column, a `FORMAT_MAJOR` question for `/audit-save`) or wire the consumer; at minimum correct the `scripting.md` table and list the stored-but-unread commands there.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
