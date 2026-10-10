# #5504: OBL-2026-10-09-D2-01: `parse_pack` drops Oblivion's legacy 4-byte `PKDT` and its 12-byte `PTDT`. 479 packages read as Find (650 placements), and every Oblivion package loses its target, so all 208 Follow packages idle.

**Labels**: ai, bug, esm-plugin, game:oblivion, legacy-compat, medium

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-09.md` — finding `OBL-2026-10-09-D2-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. Ambient AI is wrong for hundreds of placements, with no crash. It is a silent parse-boundary loss.
- **Dimension**: BSA v103 & ESM Data Slice (a TES4-only decode branch). The runtime consumer is in `npc_spawn/ai_package.rs`, which is in the suite's area.
- **Location**:
  - `crates/plugin/src/esm/records/misc/pack.rs:799-810`: the `PKDT` arm, `if sub.data.len() >= 8`.
  - `crates/plugin/src/esm/records/misc/pack.rs:905-921`: the `PTDT` arm, `if sub.data.len() >= 16`.
  - Consumers in `byroredux/src/npc_spawn/ai_package.rs`:
    - `:171-262`: `AmbientBehavior::from_package`.
    - `:743-754`: `select_active_package`.
    - `:763-820`: `apply_ai_package_behavior`, which is reached from the Oblivion runtime spawn at `npc_spawn/resumable/runtime.rs:990`.
- **Status**: NEW.
  - Searched `gh` all-states for "PTDT" and "PKDT". The only hits are #5376, #3332, #3042, #5367, #5390, #3350, #2012 (`PSDT`, Skyrim+) and #446.
  - `AUDIT_ESM_2026-10-09`'s pack.rs finding is D2-02 (`PKDD`), which is a different field.
- **Description**: The length gates are shaped for FO3/FNV.
  - **`PKDT`**:
    - `Oblivion.esm` authors 6,648 `PKDT` at 8 bytes and **561 at 4 bytes**.
    - The 4-byte layout is a legacy form: `u16` flags at 0, a `u8` procedure at 2 and a junk byte at 3. It is validated against EDIDs: 49 of 50 "…Sleep…" EDIDs have byte 2 = 4, 24 of 24 "…Wander…" have 5, and 14 of 19 "…Follow…" have 1.
    - The `>= 8` gate skips the whole sub-record, so `procedure_type` stays 0 (Find) and `package_flags` stays 0.
    - The misread procedures are Wander 179, Travel 158, Eat 73, Sleep 49, Follow 15 and Escort 5 (479 packages), plus 82 real Find.
    - OpenMW's ESM4 reader has the same gap (`loadpack.cpp:50`: `mData.type = 0; // FIXME`). UESP's Oblivion `PACK.wiki` documents only the 8-byte form.
  - **`PTDT`**:
    - Oblivion's `PTDT` is **12 bytes in all 1,776 records**: type `i32`, target `u32`, count `i32`. The FO3/FNV trailing `f32` does not exist.
    - The `>= 16` gate drops every one, so `PackRecord::target` is always `None` on Oblivion.
    - FO3 also authors 26 twelve-byte `PTDT` among 1,163 (`/audit-fo3` owns that). FNV is all 16 bytes.
- **Evidence**:
  - The raw-ESM censuses are `tools/pack_census.py`, `tools/pkdt4.py` and `tools/pack_impact.py`.
  - Runtime trace:
    1. `active_package` picks the first PKID entry that is scheduled and passes its conditions.
    2. A misread entry still passes, because its `PSDT` is 8 bytes and decodes.
    3. `from_package` then matches no `is_*` and no `PROCEDURE_EAT` / `PROCEDURE_SLEEP`, and returns `None`.
    4. `AmbientPackageRuntime.active_package_form_id` still records it as the winner. The actor stands idle, and the next PKID entry gets no fallback.
  - A Follow package whose `target_form_id` is `None` is terminal (`systems/follow.rs:79-84`).
  - An Escort package with no target skips its collect phase (`escort.rs:25`).
- **Impact**:
  - 551 actor bases (423 NPC_, 128 CREA; 650 placements) list a misread package. For 295 of them (338 placements) it is the first PKID entry, so it wins whenever it is scheduled. Examples: the `AnvilBreakfastFlowingBowl8x2`, `LaytheWavrickChorrolSleep1x6` and `CarandialAnutwyllSleep` diners and sleepers, and the SI obelisk priests' `aaaDefaultStayAtCurrentLocationSkipFallout`.
  - 196 actors (248 placements) list a Follow package. 188 of the 208 Follow packages target a specific reference, mostly the player (`ICPrisonFollowPC`, `MS13FollowPlayer`, `FGD05ModrynFollow`). None of them can follow.
  - Escort affects 52 actors (79 placements).
- **Related**:
  - ESM-2026-10-09-D2-02 (`PKDD` byte, same parser).
  - GAME-D5-2026-10-09-02 (#5391 anchoring).
  - FNV-2026-10-09-D5-01: once Eat and Sleep decode, Oblivion creatures with those packages (128 CREA list affected packages) also reach the humanoid seating path.
  - #3332, #2012.
- **Suggested Fix**:
  - In `parse_pack`:
    - Accept a 4-byte `PKDT` as `flags = u16 @0`, `procedure = u8 @2` (game-gated to Oblivion, or by size).
    - Accept a 12-byte `PTDT` (type, target, count).
  - Add an Oblivion real-data pin: 561 legacy `PKDT` decode to procedures 0..=6, with the per-procedure counts above, and every `PTDT` yields a target.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
