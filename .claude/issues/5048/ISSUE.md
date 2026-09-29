# #5048 — ESM-2026-09-29-D2-03: DIAL quest-ownership docs still name only QSTI — Skyrim and FO4 author QNAM (100% of DIALs)

**Labels**: low, documentation, doc-rot, esm-plugin, dialogue

**Source**: `docs/audits/AUDIT_ESM_2026-09-29.md` — finding `ESM-2026-09-29-D2-03`

**Severity**: LOW

**Dimension**: Record Schema Dispatch / ESM→ECS Handoff (doc-rot)

**Record / Sub-record**: `DIAL` / `QSTI`, `QNAM`

**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:19-20` (field doc: "one per QSTI sub-record"), `:183-190` (arm comment: "QSTI (Oblivion/FO3+/FO4 DIAL "Quest")"); `byroredux/src/systems/npc_dialogue.rs:3` (module doc: "`DialRecord::quest_refs` (QSTI)")

**Status in report**: NEW

## Description

The 766e1746e decode, which accepts `QSTI | QNAM`, is correct. The documentation around it is not:
- Real-data census: Oblivion has QSTI on 2,987 of 3,817 DIALs, and FNV on 11,576 of 18,215.
- Skyrim has QNAM on 15,037/15,037 and FO4 on 35,443/35,443, with no QSTI on either.
- xEdit agrees: TES5/FO4/FO76/SF1 all declare `wbFormIDCkNoReach(QNAM, 'Quest', [QUST])` (TES5:4687, FO4:6319).
- The arm comment attributes QSTI to FO4, and the field and consumer docs describe the edge as QSTI-only.
- FO4 DIAL ownership was fixed by the same QNAM arm as a side effect, and nothing records that.

## Impact

Doc only. The next reader of the P4 dialogue path is told the edge is QSTI, which is the exact misreading that starved Skyrim's ownership edge before 766e1746e.

## Related

766e1746e, ab31cfefe, `docs/engine/p4-quest-fixture.md`.

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D5-01 (#5035), ECS-2026-09-29-D7-01 (#5038), SCR-D3-2026-09-29-01 (#5041), LC-D3-01 (#5045). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

Say "QSTI (Oblivion/FO3/FNV) or QNAM (Skyrim/FO4/FO76/Starfield)" in all three places, and note that FO4 is covered.

Validated at HEAD 9fcfdc3fc: `DialRecord::quest_refs` doc still says "one per QSTI sub-record"; the arm comment still reads "QSTI (Oblivion/FO3+/FO4 DIAL \"Quest\")"; `byroredux/src/systems/npc_dialogue.rs` module doc still says "`DialRecord::quest_refs` (QSTI)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`docs/engine/p4-quest-fixture.md`, `esm-records.md`)
