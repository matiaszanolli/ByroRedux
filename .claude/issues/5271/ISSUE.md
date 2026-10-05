# #5271 — GAME-D2-2026-10-05-01: FO3/FNV dialogue ignores each INFO's own owning quest — a multi-quest topic can speak an INFO whose quest is not running

- **Labels**: low,gameplay,dialogue,quests,game:fnv,game:fo3,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5271

- **Severity**: LOW (mostly masked by `GetIsID`; misroutes the residue only)
- **Dimension**: 2 — NPC dialogue (P4 route, extended to FO3/FNV by #5224)
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:112-128` (`owned_topic_records`: any running bound quest in the DIAL's `quest_refs`)
  - `:226` (`select_first_info` walks every INFO of the DIAL)
  - `:240` (`owning_quest = quest_refs.first()`)
  - `crates/plugin/src/esm/records/misc/dialogue.rs:249-` (`InfoRecord` has no quest field)
- **Status**: NEW. The decode half belongs to `/audit-esm`, and the INFO/CTDA semantics to `/audit-scripting`.
- **Trigger**: FNV or FO3. Activate an NPC bound by a running quest that owns a shared Top-level topic, for example `DoctorMedical` `0x1DCEE` or `FollowersHired` `0x37084`.
- **Description**: in FO3/FNV, each INFO names its own quest (`QSTI`), and one DIAL can list several. In the GECK, an INFO counts only while its quest is running. That quest's priority orders the INFOs, and its dialogue conditions gate them all ("Quest conditions are checked first; only if those are true are the conditions on the infos evaluated", GECK *Quest Data Tab*). None of this is decoded or applied: once any one running bound quest owns the DIAL, INFOs from every listed quest compete in file order.
- **Evidence** (Python census of `FalloutNV.esm`):
  - All 23,247 INFOs carry `QSTI`.
  - 138 DIALs list more than one quest. In 28 Topic-type DIALs, the INFOs span more than one quest.
  - The Top-level (`DATA[1] & 0x02`) ones are `DoctorMedical`, `FollowersHired`, `FollowersFired`, `DoctorSupplies`, `DoctorRadiation` and four Gomorrah/Primm topics. Most of their INFOs have `GetIsID` (fn 72) conditions, which mask the problem: `DoctorMedical` has 34 INFOs, 5 without `GetIsID`; `FollowersHired` 26 and 2; `FollowersFired` 11 and 1.
  - `GREETING` (`0xC8`, 209 quests, 5,300 INFOs) has flags `0x00`, so #5224 makes it link-only, and it never opens a list.
- **Impact**: on these topics, an NPC can speak a line that belongs to a stopped quest, or skip a higher-priority quest's line. The NPC-side log also names the wrong owning quest. Skyrim (one `QNAM` per DIAL) is unaffected by the per-INFO half, but Skyrim quest dialogue conditions are not applied either.
- **Suggested Fix**:
  - Decode the INFO `QSTI` (FO3/FNV/Oblivion) into `InfoRecord.quest`.
  - In `select_on_topic` / `top_level_menu`, filter INFOs to running quests that bind or own the speaker, and order them by quest priority.
  - Have `/audit-scripting` decide on applying quest-level dialogue conditions.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D2-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
