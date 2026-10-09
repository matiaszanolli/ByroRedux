# #5428: FO3-2026-10-08-D2-01: #5304's vanilla-master provenance check cannot decline a true override patch — an override keeps the master's FormID, so `script_is_vanilla` accepts it; the doc claim is false and the test pins a different case

**Labels**: low,gameplay,scripting,bug,game:fo3,game:fnv,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5428

**Source**: `docs/audits/AUDIT_FO3_2026-10-08.md` — `FO3-2026-10-08-D2-01` (HEAD `00f580e09`)

- **Severity**: LOW. Same severity as SCR-D6-2026-10-05-01. Vanilla content is unaffected. Only a modded load order that rewrites a kill script's body hits it.
- **Dimension**: ESM Data Slice / authored corpses. The mechanism owner is `/audit-gameplay` (`reference_state.rs`), and the decline invariant belongs to `/audit-scripting`. Labels: `game:fo3`, `game:fnv`.
- **Location**:
  - `byroredux/src/cell_loader/reference_state.rs:392-421` (`VANILLA_KILL_SCRIPT_PLUGINS`, `script_is_vanilla`, and the doc paragraph at `:394-398`)
  - the call sites `:496` and `:519`, plus the load-order variant `:877`
  - `byroredux/src/cell_loader/load_order.rs:315-319` (`plugin_for_form_id`)
  - the test `reference_state.rs:928-1044`
- **Status**: NEW. This is the incomplete half of closed #5304. The scripting audit marks #5304 "Fixed and verified", and no sibling report covers the override case.
- **Description**: #5304, from SCR-D6-2026-10-05-01, named two ways the recognizer could be fooled into marking live actors as corpses:
  - a name-alike script, which is now declined by the exact-EDID list;
  - "a patch or mod that overrides one of these SCPTs with a conditional body".

  The fix gates on "the winning SCPT record is defined by a shipped vanilla master". The doc at `:394-398` claims an override "wins the merged index under the vanilla form id but resolves to the overriding plugin here, so its conditional body is declined".

  That cannot happen. `script_is_vanilla` calls `plugin_for_form_id`, which decodes only the FormID's global slot (its top byte) into the plugin that owns that slot. An override record keeps the master's FormID, so the slot is `fallout3.esm` (or `falloutnv.esm`), and the check returns `true` no matter which plugin's body won the merge. The only case the provenance check declines is a *new* SCPT with the vanilla EDID, which a mod's own NPC_ points at under the mod's slot.

  That new-record case is exactly what the regression test builds: `script(0x0100_3333, "GenericKillSCRIPT")`, a fresh mod FormID, with comments calling it "a same-EDID override". So the test passes while the override case stays open.
- **Evidence**:
  ```rust
  // reference_state.rs:418-421
  fn script_is_vanilla(script_form_id: u32, load_order: &LoadOrder) -> bool {
      super::load_order::plugin_for_form_id(script_form_id, load_order)   // top-byte slot only
          .is_some_and(|plugin| VANILLA_KILL_SCRIPT_PLUGINS.contains(&plugin))
  }
  // load_order.rs:315-319
  let slot = global_slot_of(form_id);
  let position = load_order.slots.iter().position(|s| *s == slot)?;
  ```
  `EsmIndex::merge_from` folds `scripts` last-write-wins under the same global FormID, so the record is replaced and its FormID is preserved.
- **Impact**: Load `Fallout3.esm` plus any patch that edits `GenericKillSCRIPT`, `OnLoadKillSelf` or a `GenericBiped*DismembermentSCRIPT` body, for example to make the kill conditional. The engine still kills every linked target or self-killing placement at load: 85 + 3 placements on FO3, 10 + 2 on FNV. Those actors spawn as lootable, AI-less corpses. That is the failure #5304 was filed to stop. Nothing crashes, and vanilla is correct.
- **Related**: #5304 (closed), #5223, #5248, SCR-D6-2026-10-05-01.
- **Suggested Fix**: Track which plugin's record won. One option is for `merge_from` (or a load-order side table) to record the winning plugin per SCPT FormID, so the check can require "defined **and last written** by a vanilla master". Another is the issue's preferred route: gate on the decoded `SCDA` body (an unconditional `OnLoad` → `KillActor`). Correct the `:394-398` doc and rewrite the test fixture as a true override, with the same FormID and a later plugin.

## Completeness Checks
- [ ] **SIBLING**: Every `script_is_vanilla` call site (the per-cell paths and the load-order variant) gets the same winning-plugin / SCDA-body gate
- [ ] **TESTS**: A regression test pins this specific fix — with a true override fixture (same vanilla FormID, later plugin), not a new mod FormID
