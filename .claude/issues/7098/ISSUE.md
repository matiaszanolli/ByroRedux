# #7098 — ESM-01: Starfield `DOOR` `ANAM`/`BNAM` (Facing Axis enum, `BFCB` strings) read as sound FormIDs; 620 of ESM-03's warn lines

Labels: low,esm-plugin,bug,game:starfield
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-01)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-01**

- **Severity**: LOW (decode defect, no audible effect; its log impact is counted under ESM-03)
- **Dimension**: Sub-Record Byte Accounting / FormID & Load Order / Record Schema Dispatch / CELL Walkers. Found independently as D2-06, D3-02, D4-06 and D5-01.
- **Record / Sub-record**: Starfield `DOOR` / `SNAM`, `ANAM`, `BNAM` (and the `BFCB`…`BFCE` component block)
- **Location**: `crates/plugin/src/esm/cell/support.rs:90-103`. The arm `b"SNAM" | b"ANAM" | b"BNAM" if is_door && sub.data.len() >= 4` has no game gate. Also affected: the `DoorData` doc in `cell/base_objects.rs` ("FO4/Starfield read as Skyrim's layout until checked") and `.claude/commands/audit-esm/SKILL.md` (Dim 5 `DOOR` bullet).
- **Status**: NEW. Introduced by `50abfd008` (#7059, DRT-REC-002). Adjacent to open #7061 (M66 Phase 3 Starfield census), which does not name the decode.
- **Description**: In xEdit `wbDefinitionsSF1.pas:11575-11626`, Starfield `DOOR` sounds are `wbSoundReference` structs `DOSH`/`DCSH`/`DLSH`. `ANAM` is `wbInteger('Facing Axis Override', itU32)` (0 X, 1 Y, 2 Based on Bounds). The `wbBaseFormComponents` block contributes string `ANAM`/`BNAM` sub-records (animation-graph paths). Oblivion, FO3, FNV, Skyrim, FO4 and FO76 all author `SNAM`/`ANAM`/`BNAM` as SOUN/SNDR FormIDs, so the arm is right on those six games. On Starfield it reads the first four bytes of a component string, or the axis enum, as a FormID.
- **Evidence**: `Starfield.esm` holds 339 `DOOR` records:
  - Codes: `DOSH` ×310, `DCSH` ×303, `DLSH` ×1, `SNAM` ×0, `ANAM` ×649, `BNAM` ×310.
  - The trailing 4-byte `ANAM` is 2 on 326 records, 0 on 10 and 1 on 3.
  - Component `BNAM`/`ANAM` payloads begin with ASCII, e.g. `0x70696873` "ship", `0x6D696E41` "Anim".
  - `Starfield.esm` lists no masters. `remap_starfield` (`reader.rs:617-645`) takes an ASCII top byte such as `0x70` as regular-master ordinal 112, cannot resolve it, and logs `warn!("Starfield FormID … names a master this plugin does not list")`. The orchestrator verified this path in source. Dim 3 counted about 620 such sub-records; Dim 8 measured the real warn count (see Part A).
- **Impact**:
  - `StaticObject::door.close_sound` / `loop_sound` / `open_sound` hold fabricated ids on about 329 of 339 doors, and `commands/door.rs` prints them.
  - `systems/door.rs::play_door_sound` misses `index.sounds` and logs at `debug`, so nothing plays wrong today.
  - Every Starfield session start carries hundreds of spurious `warn!` lines, which buries a genuine out-of-range FormID warning.
  - Starfield's real `DOSH`/`DCSH` sounds are never read.
- **Suggested Fix**: Pass `game` into `build_static_object_from_subs` and decode `SNAM`/`ANAM`/`BNAM` only when `game != Starfield`. Leave the three sounds `None` on Starfield until `DOSH`/`DCSH`/`DLSH` are decoded as `wbSoundReference` (#7061). Pin it with a Starfield fixture: a `BFCB` string `ANAM`/`BNAM` plus a trailing `ANAM` = 2 must leave all three sounds `None` and log no warning. Correct the `DoorData` doc and the skill bullet: flags bits 1–5 are verified on FO4/FO76/Starfield, sounds only on FO4/FO76.


Every other item below is reproduced in full in its dimension's appendix section, under the same ID. In brief:

- **ESM-D1-01** (Regression of #5819). `crates/plugin/examples/esm_dim8_coverage.rs:128-153`: `walk` recurses with no depth. Replace it with `EsmReader::for_each_record`.
- **ESM-D1-02**. `crates/plugin/src/esm/reader.rs:1018-1075` duplicates `crates/bsa/src/safety.rs:176-207` (`inflate_bounded_zlib`), which is not exported. The two agree today. Export it and call it; fix the doc reference to `inflate_bounded`.
- **ESM-D1-03**. 279.0 / rv 209 survive in `docs/engine/esm-records.md:228`, `records/tests.rs:2602-2609` and `records/spatial_units_tests.rs:107,236`. Add 280.0 / rv 210 there.
- **ESM-D1-04** (INFO). `records/calendar.rs:52-67`: pass "HEDR present" (`hedr_version > 0.0`) so a defaulted header yields `None`.
- **ESM-D2-07**. `records/actor/race.rs:528` uses `bonus_count = if Oblivion {7} else {8}`. Use 7 plus `skip_or_eof(2)` for the whole TES4-era arm. FNV/FO3/Oblivion `RACE.DATA` bytes 14..16 are `00 00` on every vanilla race (22/22/15). Fix the `:515` comment and the misnamed test `race_oblivion_data_reads_8_skill_pairs_plus_heights`.
- **ESM-D2-08**. Name the lengths (`OBLIVION_NPC_DATA_LEN` 33, `…_CREA_DATA_LEN` 20, `…_ACBS_LEN` 16, `…_AIDT_LEN` 12, `…_ENIT_LEN` 8). Read through `SubReader`. Consider a text guard for `try_into().unwrap()` in `esm/records`, with an allowlist for the older sites.
- **ESM-D2-09**. `misc/magic.rs:849`: add a per-game `EFIT_LEN` (Oblivion 24, FO3/FNV 20, Skyrim+ 12) and decline a short body, as `decode_spit` already does.
- **ESM-D2-10** (merges D4-08). `items.rs:792` says "0=light, 1=clothing, 2=heavy"; xEdit `wbArmorTypeEnum` is 0 Light / 1 Heavy / 2 Clothing, and every consumer agrees with xEdit. `items.rs:195-199` says "None pre-Skyrim", false since `44cd08fdf`. `misc/world/region.rs:154,161-162` says `RDMD` has "no consumer", but `music/translate.rs:408-416` reads it. `docs/engine/esm-records.md` still lists `ALOC` among the stubs and calls `MUST` Skyrim-only. `index.rs` `MANUALLY_MERGED` cites stale `merge_from` line numbers, and `records/tests.rs:1949` says "80 typed maps".
- **ESM-D2-11**. `records/outfit.rs:80`, `cell/helpers.rs:364-370` and `records/common.rs:489-492` follow a different ragged-tail policy than #6782. Pick one policy and state it once. Skyrim.esm has no ragged body in 18 FormID-array kinds.
- **ESM-D3-03**. `misc/magic.rs:1358`: on Oblivion, remap `associated_item` only when flag bits 16–18 are set (xEdit `wbMGEFFAssocItemDecider`, `TES4.pas:818-836`).
- **ESM-D3-04**. `records/condition.rs:542-545,628-641`: pass the game into the CTDA remap and skip `ptActorValue` params before FO4. Distinct from closed #5989, which fixed only the evaluator.
- **ESM-D3-05**. `records/condition.rs:473-479`: delete `parse_condition_list` and its `EXEMPT_NO_U32_READS` row, or give it a `remap`.
- **ESM-D6-01**. `misc/magic.rs:670-678`: gate the `EPF2` lstring read on `EPFT == 4`. xEdit FO76 `wbEPF2Decider` (`FO76.pas:2165-2175`) makes EPFT 1 a `CURV` FormID.
- **ESM-D6-02**. `strings_table.rs:370-386`: normalise an empty parent to `"."` before the `Strings` join and the `read_dir`, as `strings_archive_directory` (#5303) does. Add a bare-name test.
- **ESM-D7-01**. `byroredux/src/systems/projectiles.rs:573-578`: add a handle-keyed lookup beside `FormIdPool::entity_with_local` and a production-text guard against `World::find_by_form_id` in `byroredux/src`.
- **ESM-D7-02**. `byroredux/src/cell_loader/relocated.rs:67,79-103`, plus the same pattern at `music/mod.rs:205` and `systems/story_events.rs:69`, which refresh per frame and so are lower exposure. Key the cache on a load-order generation or a `Weak<EsmIndex>`, never a raw address.
- **ESM-D7-03**. `byroredux/src/systems/npc_dialogue.rs:743-788`: run the follow-up decision once per line, and add an INFO → topic map.
- **ESM-D7-04**. `crates/plugin/src/esm/records/items.rs:190-191` and five writes: delete `slot_mask`.
- **ESM-D8-01**. `records/misc/location.rs:62-112`: use a per-`GameKind` stride (Starfield 16/20, FO4/FO76 12/16). Starfield `LCPR` 517/517 lengths are multiples of 16 and `LCSR` 714/714 multiples of 20; FO4/FO76 are 100% at 12/16.
- **ESM-D8-02**. `records/actor/npc.rs:1154-1171`: use a 12-byte stride for `Fallout76 | Starfield` (or form version ≥ 152, xEdit `wbObjectProperty`, `SF1.pas:5679-5684`) and keep the Curve Table. Check the other `PRPS` readers (RACE, ACTI family) for the same stride.
- **ESM-D8-03**. `records/condition.rs:638-641`: skip `0xFFFFFFFE`/`0xFFFFFFFF` in `remap_condition_form_ids`, or game-gate `param1_is_form_id` (there is no Starfield function catalog).
- **ESM-D8-04**. `cell/helpers.rs:155-198`: add a FO76 `XCRI` arm (`wbDefinitionsFO76.pas:8069-8085`: `u64` counts, 8 B meshes, 16 B refs), or log at `debug` and document FO76 as unsupported.
- **ESM-D8-05** (Regression of #6184). Sites: `equip.rs:2741`; `actor_value_derive.rs:1519,1941`; `records/tests.rs:78` (`load()`, five callers at `:89-181`), `:213,906,1024,1058,1094,1774,1818`; `actor/tests.rs:2175,2226,2351`; `cell/tests/integration.rs` ×12. Route them through `test_paths::require_or_skip`.
- **ESM-D7-05**. `docs/engine/index.md:59,192` and `docs/engine/architecture.md:36`: add "(designed; no production caller, #5609)".

Independent reports: ESM-D2-06, ESM-D3-02, ESM-D4-06, ESM-D5-01 (appendix of the report).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
