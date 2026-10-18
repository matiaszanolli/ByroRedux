# SCR-D3-2026-10-17-01: Authored `GetActorValue` (fn 14) reads an actor-value *enum code* as an AVIF FormID on every pre-FO4 game

Issue: #5989
Labels: high, scripting, bug, character, game:fnv, game:fo3, game:oblivion, game:skyrim

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-01: Authored `GetActorValue` (fn 14) reads an actor-value *enum code* as an AVIF FormID on every pre-FO4 game

- **Severity**: HIGH
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/condition/catalog.rs:282` (row), `crates/scripting/src/condition/eval.rs:367` (`actor_value_view` keyed lookup),
  `crates/plugin/src/esm/records/condition.rs:532` (fn 14 in `param1_is_form_id`)
- **Status**: NEW (the 2026-10-12 report noted the type mismatch in passing - `AUDIT_SCRIPTING_2026-10-12.md:148` - "not verified"; verified here)
- **Untrusted-Input**: No
- **Description**: xEdit types fn 14's `param_1` as `ptActorValue` = `wbActorValue` *enum* for TES4, FO3, FNV and TES5 (`wbDefinitionsTES4.pas:776`,
  `FO3.pas:2149`, `FNV.pas:2240`, TES5 `GetActorValue ptActorValue`); only FO4+ stores an AVIF FormID (`GetValue`, `wbActorValue := wbFormIDCkNoReach`).
  The catalog documents/handles it as "the global-space AVIF FormID" for all games, the plugin remaps it as a FormID, and `ActorValues` is keyed by
  AVIF FormID (FNV `Strength` 0x3E8, Skyrim `AVHealth` 0x3E8).
- **Evidence**: CTDA census of the vanilla masters: FalloutNV.esm fn 14 = 1 576 uses (INFO 1 264, IDLE 162, PACK 37, MESG 41, TERM 36, SPEL 35) with
  `param_1` in {0x2b (540), 0x20 (209), 0x28, 0x25, 0x1d, 0x1e, 0x19, 0x09 ...} - SPECIAL codes 5-11, skill/stat codes - while the FNV AVIF FormIDs
  are 0x3E8+ (Strength 0x3E8, Intelligence 0x3EC, Health 0x450); Fallout3.esm 958 uses; Oblivion.esm 120; Skyrim.esm 543 (INFO 318, PACK 169) with
  `param_1` in {68, 95, 17 (Speechcraft), 73, 74, ...} vs Skyrim AVIF FormIDs from 0x3E8. No translation exists anywhere between `parse_ctda`
  and `actor_value_view` (grep: no code->AVIF table; the only construction sites with real AVIF FormIDs are engine-built conditions
  `barter.rs:361`, `combat.rs:1138`, `encumbrance.rs:143`, `commands/gameplay.rs:400`). The unit test `condition/tests.rs:747` uses FormID stand-ins,
  so it cannot see this. Not verified by running the engine.
- **Impact**: every authored skill/SPECIAL/karma/stat gate on FO3/FNV/Skyrim dialogue, packages, quests and scenes evaluates the *absent-AV default
  0.0* (`actor_value_view` returns 0.0 for a missing key) instead of the stat, and `is_modeled` reports it as modeled so no fail-closed consumer
  declines it: `GetActorValue Speech >= 25` options never unlock, `== 0` / `< N` gates always pass. The plugin remap additionally rewrites the code
  into `masters[0]`'s global slot (identity today only because masters sit at slot 0).
- **Related**: `/audit-character` Dim 4 (ActorValues key space); #1663 (closed, implemented the FormID reading for FO4-style authoring); #2987.
- **Suggested Fix**: translate the enum code to the game's AVIF FormID once at the parser boundary (per-game `wbActorValue` table -> `index.actor_values`
  by EditorID) and drop fn 14 from `param1_is_form_id` for the enum games; until then make `is_modeled` return `false` for fn 14 when the loaded game
  is not Fallout 4 / Starfield so the fail-closed gates decline instead of reading 0.0.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
