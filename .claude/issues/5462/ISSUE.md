# #5462: SKY-D3-2026-10-08-02: #5079's `is_child_race` test pins "Skyrim+ child races do not use the flag", but TES5 RACE flag `0x4` is `Child` and is set on exactly the 5 child races

**Labels**: low,legacy-compat,bug,test-gap,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5462

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-08.md` — `SKY-D3-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `npc_spawn/tests.rs` still asserts `!is_child_race(GameKind::Skyrim, Some(0x04))` with the "Skyrim+ child races do not use the flag at all (#2455 unverified)" comment; `is_child_race` is now at `byroredux/src/npc_spawn.rs:606-608`.

- **Severity**: LOW
- **Dimension**: 3 — NPC equip + FaceGen (Skyrim race data)
- **Location**: `byroredux/src/npc_spawn/tests.rs:2884-2904` (`is_child_race_keeps_oblivion_beast_race_distinct_from_fo3_fnv_child`, which asserts `!is_child_race(GameKind::Skyrim, Some(0x04))` with the comment "Skyrim+ child races do not use the flag at all (#2455 unverified)"); `byroredux/src/npc_spawn.rs:597-608`.
- **Status**: NEW.
- **Description**: xEdit `wbDefinitionsTES5.pas:9136` defines the TES5 RACE DATA flag `0x00000004` as `'Child'`. A byte walk of `Skyrim.esm` RACE DATA (flags at offset 32) finds the bit set on exactly `NordRaceChild`, `ImperialRaceChild`, `RedguardRaceChild`, `BretonRaceChild` and `BretonRaceChildVampire`, with no false positives. The helper's game gate itself is harmless today: the only consumers are the KF-era body-path and walk-clip ladders, and Skyrim never takes them (`humanoid_body_paths` returns `&[]`; the walk comes from HKX). The problem is that the test asserts a false data claim as a contract.
- **Impact**: None at runtime today. The next Skyrim consumer of "is child" (for example child-specific scale, dialogue conditions or combat exclusion) would find a pinned test telling it the bit is meaningless on TES5.
- **Suggested Fix**: Correct the comment to cite xEdit TES5 `Child = 0x4`, and either keep Skyrim out of this KF-era helper explicitly ("no Skyrim consumer") or admit `GameKind::Skyrim` once a consumer exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO4 RACE DATA child bit; other per-game race-flag tests)
- [ ] **TESTS**: A regression test pins this specific fix
