# #5408: SKY-D3-2026-10-08-01: #5095's FaceGen-miss head fallback loads only the NPC's own `PNAM` parts — the vanilla Skyrim player gets a hair mesh with no face, mouth or eyes, because RACE base head parts and HDPT extra parts are never decoded

**Labels**: medium,esm-plugin,character,bug,game:skyrim,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5408

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-08.md` — `SKY-D3-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `prebaked_head_fallback_paths` still maps only `npc.face_morphs.head_parts` (PNAM); `parse_hdpt` reads only DATA[0] into `flags` (no PNAM/HNAM); `RaceRecord.head_parts` doc still says "empty on Skyrim+"; `p3-player-body.sh` still gates on `head_parts > 0`. Not a duplicate of #5095 (closed) — incomplete fix, filed fresh.

- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen
- **Location**:
  - `byroredux/src/npc_spawn/resumable/prebaked.rs:174-185`: `prebaked_head_fallback_paths` reads PNAM only.
  - `:296-357`: the `HeadParts` phase.
  - `crates/plugin/src/esm/records/actor/race.rs:55-66`: the doc says "empty on Skyrim+, which moved head parts out to standalone `HDPT` records". The head section's `HEAD` sub-record has no arm; only `MODL` is collected.
  - `crates/plugin/src/esm/records/misc/character.rs:12-21`: `HdptRecord` has no `PNAM` type and no `HNAM` extras. Its flags doc is wrong.
  - `byroredux/src/player_body.rs:636`: the `head_parts` count.
  - `docs/smoke-tests/p3-player-body.sh`: the `head_parts > 0` gate.
- **Status**: NEW. #5095 (CLOSED 2026-10-06) is an incomplete fix, not a regression: its fallback never covered race base parts. No sibling report or issue matches. `gh` "head part race default" across all states finds only #5095, #591 and #967.
- **Description**:
  - **The fix's premise.** The commit and the code doc rest on two claims: "Skyrim RACEs carry no head-part table at all", and the player's authored PNAM list is "ManHead / eyes / hair / brows".
  - **Both claims are false on the shipped data.**
    - A TES5 RACE carries per-gender **Head Data**: a `NAM0` marker, then `MNAM` / `FNAM`, then `wbHeadParts` = `INDX` + `HEAD` → HDPT (xEdit `wbDefinitionsTES5.pas:9282`, `:9470-9497`). The CK Race page lists these as "Base Head Parts", and the CK Character Gen Parts tab lists the base head parts as Face, Eyes, Hair, Facial Hair and Eyebrows.
    - The vanilla player record authors a single hair part.
    - The parser decodes no RACE `HEAD`, no HDPT `PNAM` type and no HDPT `HNAM` "Extra Parts" (xEdit `:5296-5309`).
  - **Result.** The fallback loads only what PNAM names. For the player that is one hair NIF, so the third-person player still has no face.
- **Evidence**: byte walk of the installed SE masters.
  - **Player record.** `Skyrim.esm` NPC_ `0x00000007` (`Player`, ACBS `0x30`, male, race `NordRace` `0x13746`) has PNAM = `[0x051507 HairMaleNord01]` (type 3 Hair, `Hair\Male\Hair01.nif`, HNAM extra `HairLineMaleNord01`). No DLC overrides the player record.
  - **Race defaults.** `Update.esm` overrides NordRace.
    - Male base head parts: `HairMaleNord10` (Hair), `MaleHeadNord` (Face), `MaleMouthHumanoidDefault` (Misc), `MaleEyesHumanLightBlue` (Eyes).
    - Female base head parts: 5, including `FemaleBrowsHuman01`.
    - In `Skyrim.esm`, 32 of 99 RACEs author head parts.
  - **What the fix itself reported.** Its live check reports `head_parts=1 (meshes 8→9)`, and that single part is the hair.
  - **Extra parts.** 230 of 766 HDPTs author `HNAM` extras (271 refs). Of the 2,912 NPC_ records that author PNAM, 2,443 reference an extra part that is absent from their own PNAM list. Every facegeom-less NPC (modded NPCs, any record without baked facegeom) loses those parts too.
  - **Why the gate misses it.** `p3-player-body.sh` fails only on `head_parts == 0`. The count is a world-wide `PrebakedHeadPart` query, not one scoped to the player root, so a hair-only player passes.
  - **The flags doc is wrong.** `HdptRecord.flags` says Skyrim "bits 0-2 encode the head-part type slot". xEdit has DATA = Playable / Male / Female / Is Extra Part / Use Solid Tint, and the type is the separate `PNAM` u32.
- **Impact**:
  - **The player.** On every Skyrim session, the third-person player shows a floating hair shell over an empty neck: no face, eyes, mouth or hairline. This is the case #5095 was filed to fix, and the issue is now closed.
  - **Other NPCs.** Any facegeom-less humanoid NPC gets the same partial head.
  - **Detection.** The fallback is silent and the gate stays green.
- **Related**: #5095 (closed, incomplete). The CHARACTER 2026-10-08 note (the fallback reads the shell's PNAM, not the TPLT terminal) is a separate axis. Also related: #3409 (hidden-partition handling, reused correctly) and #5359 (NPC_ `WNAM`, same "Skyrim body source" family).
- **Suggested Fix**:
  1. Decode RACE Head Data `HEAD` per gender section into `RaceRecord`, and decode HDPT `PNAM` (type) and `HNAM` (extras).
  2. Build the fallback list from the race's base parts for the NPC's gender. For each type the NPC's PNAM authors, use the NPC's part instead. Then expand `HNAM` extras recursively, with a cycle guard.
  3. Source the exact per-type override rule before coding it (for example SKSE's `TESNPC::GetCurrentHeadPartByType` race fallback). Do not guess it.
  4. Tighten the P3 gate to require a Face-type part on the player root, and fix the `race.rs` / `character.rs` docs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO4 RACE head data / HDPT extras on the FO4 prebaked-FaceGen path; the TPLT-terminal PNAM axis noted by AUDIT_CHARACTER_2026-10-08)
- [ ] **TESTS**: A regression test pins this specific fix (player root carries a Face-type part; HNAM extras expanded with a cycle guard)
