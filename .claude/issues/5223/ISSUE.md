# #5223 — FO3-D2-01: 49 FO3 authored corpses are killed by linked OnLoad dismember triggers, not base health — they spawn alive and hostile, and #5005's guard pins them alive with a wrong rationale

https://github.com/matiaszanolli/ByroRedux/issues/5223

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: MEDIUM
- **Dimension**: ESM Data Slice. The mechanism belongs to `/audit-scripting` (no FO3 ObScript dialect, no `KillActor`); the consequence belongs to `/audit-gameplay` (spawn-dead). FNV reach is small (3 targets).
- **Location**:
  - `crates/plugin/tests/parse_real_esm.rs:4734-4764` (`fo3_corpses_key_on_base_health_not_xrgd`: its doc comment and the `MS06SuperMutantGunDEAD` assertion message);
  - `byroredux/src/cell_loader/references/attach.rs:263-279` (`obscript_dialect_for` → `None` for FO3);
  - `byroredux/src/cell_loader/references/mod.rs:756`.
- **Status**: NEW.
  - Searched the open issues, and closed issues for "corpse", "dismember", "OnLoad", "KillActor".
  - #5005 (closed) treats these refs as live by design.
  - #5015 is the pose half, not this.
- **Description**: FO3 has a second authored-corpse mechanism beside the base-health rule.
  - A trigger REFR, whose base script is one of the `GenericBiped*DismembermentSCRIPT` family, links (`XLKR`) to an actor placement.
  - Its `Begin OnLoad` block runs `linkedRef.killactor linkedRef <limb>` once, under `doOnce`, with no other condition.
  - The actor's base has positive health and is often leveled (`LvlSupermutantGunDISMEMBER` → TPLT `EncSuperMutantGun`, Use AI Data), so the corpse exists only after the script runs.
  - ByroRedux has no FO3 ObScript dialect and no `KillActor`, so these placements spawn as live, aggressive actors.

  #5005's FO3 guard asserts `!starts_dead` for `MS06SuperMutantGunDEAD`. It justifies this as "a die-at-spawn ability", but two facts contradict that:
  - the base's only SPLO is `RadImmunity`;
  - its quest script `MS06SupermutantScript` calls them "the dead supermutants".

  The test therefore pins the bug as intended behaviour.
- **Evidence** (Python walker over Fallout3.esm):
  - **Triggers.** 85 `XLKR` targets come from REFRs whose base script has an unconditional OnLoad kill. **49** of them sit on positive-health bases:
    - `LvlSupermutantGunDISMEMBER` ×13, `…GunDISMEMBERNOAMMO` ×10, `…MeleeDISMEMBERNOAMMO` ×6, `…MeleeDISMEMBER` ×5;
    - `LvlWastelanderDISMEMBER` ×5, `CrFeralGhoul1A` ×4;
    - `MS06SuperMutantGunDEAD` ×4, `MS06SuperMutantMeleeDEAD` ×1, `DeadSuperMutant2Melee` ×1.

    None of the targets or triggers is Initially Disabled.
  - **Example.** `MS06SuperMutantGunDEAD` 0x0645E0 is targeted by `GenericBipedLeftLegDismembermentSCRIPT`. Its base CREA 0x027FB4 has DATA health 10 and template flags `0x1FD`, so Use Stats is not set.
  - **Example.** Statesman Hotel 0x0AB3EB `StatesmanHotel02Dismember01REF`.
  - **XRGD overlap.** All 6 FO3 "XRGD over a live base" refs that #5005 cites are among the 49, so on FO3 `XRGD` ⇒ corpse holds for 498/498.
  - **Self-scripted bases.** 7 more positive-health bases kill themselves from their own SCRI. 3 do it unconditionally: `FFEU07Corpse`/`GenericKillSCRIPT`, `FFEU255NPC1`/`OnLoadKillSelf`, `FFEU04NPC1`.
  - **No executor.** `grep -i killactor` over `byroredux/src`, `crates/scripting/src` and `crates/plugin/src` returns 0 hits.
  - Orchestrator spot-check: I confirmed the test doc and assertion text at `parse_real_esm.rs:4734-4764`, and found no open or closed issue for the mechanism.
- **Impact**:
  - About 49 placements spawn alive instead of as pre-dismembered corpses. They cannot be looted and they carry AI packages.
  - The super-mutant majority take their AI from the `EncSuperMutant*` templates, so they are hostile and can start combat on sight. This happens at Statesman Hotel, the MS06 "Head of State" site, and similar scenes.
  - The #5005 guard locks the wrong state in place.
- **Related**: #5005, #5015, #4414, #4814, the FO3 scripting gap (Dim 5), `apply_starts_dead`.
- **Suggested Fix**:
  - Recognise the vanilla idiom from the data: a REFR whose base SCPT has an unconditional `Begin OnLoad … killactor` on `GetLinkedRef`/self under `doOnce`. Implement it either as a narrow FO3 translation in `obscript_runtime` or as a load-time "script-killed" stamp through `apply_starts_dead`.
  - Pin 49 linked + 3 unconditional self-kills.
  - Correct the test doc and message so they name the dismember-trigger mechanism.

## Completeness Checks
- [ ] **SIBLING**: The 3 unconditional self-kill bases (`GenericKillSCRIPT`, `OnLoadKillSelf`, `FFEU04NPC1`) and FNV's 3 linked targets are covered by the same recognizer
- [ ] **TESTS**: A real-data pin counts 49 linked + 3 self-killed FO3 corpses, and `fo3_corpses_key_on_base_health_not_xrgd`'s doc/message names the dismember-trigger mechanism
