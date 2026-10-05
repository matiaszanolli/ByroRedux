# #5338 — CHAR-2026-10-05-D4-01: FO3/FNV non-auto-calc NPCs (~40% of actors) are seeded with their class SPECIAL, not their authored NPC_ DATA SPECIAL — #5005 already parses it, so the #2957 blocker is stale (Jason Bright 120 vs 135)

- **Labels**: medium,character,esm-plugin,game:fnv,game:fo3,bug
- **Filed from**: `docs/audits/AUDIT_CHARACTER_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5338

- **Severity**: MEDIUM. It is the same class as #5238 (HIGH), but it hits a disclosed deferral, and the per-actor error is mostly ±1–3 SPECIAL points.
- **Dimension**: Population Boundary
- **Game**: fo3, fnv
- **Location**:
  - `crates/plugin/src/esm/records/actor_value_derive.rs`: `derive_autocalc_actor_values`, which reads `class.base_attributes` for every actor and never consults `acbs_flags & 0x10`. Also the module doc's "Non-auto-calc NPCs" deferral, whose "unparsed … DNAM-era layout" claim is stale.
  - `crates/plugin/src/esm/records/actor/mod.rs`: the FO3/FNV `NPC_` `b"DATA"` arm in `parse_npc`, which decodes only the i32 and discards the 7 SPECIAL bytes it already holds.
- **Status**: NEW. It is related to #2957, which was closed as documented on 2026-08-21. That issue only corrected the note's stated *scale*; its blocker ("the stored values are … unparsed") was true then and stopped being true for SPECIAL when #5005 (`de6bdb381`) added the 11/25-byte `DATA` arm.
- **Source**:
  - GECK *Stats Tab - NPC*, from the local dump: "The Attributes and skills will be automatically calculated if [Autocalc] is checked … **Attributes may not be changed if Autocalc is checked.**" When the box is clear, the attributes are authored on the NPC.
  - The `parse_npc` comment of #5005 itself: "`NPC_` `DATA`: i32 Base Health + the 7 SPECIAL attributes (11 bytes)".
  - fandom infobox (*Jason Bright*): `special = 6,5,7,2,6,6,3`, `level = 6`, `Hit Points: 135`.
  - fandom infobox (*Martina Groesbeck*): `special = 4,6,4,5,4,7,3`, `level = 6`, `Hit Points: 80`.
- **Description**: The population path assumes every FO3/FNV `NPC_` is auto-calculated. For a record with the ACBS "Auto-calc stats" bit clear, the engine uses the SPECIAL the record authors in `DATA` bytes 4–10, and that SPECIAL differs from the class in nearly every such record. #5238's curve is correct: fed the authored END, it reproduces both samples below exactly. The defect is that its END input, and all seven seeded SPECIAL values, come from the class.
- **Evidence**:
  - Census (raw `NPC_`/`CLAS` walk of the masters; ACBS bit `0x10`):

    | | FNV | FO3 |
    |---|---|---|
    | auto-calc OFF | 1,533 | 712 |
    | of which `DATA` SPECIAL ≠ class `ATTR` | **1,495** | **708** |
    | auto-calc ON, `DATA` ≠ class | 159 / 2,283 | 0 / 935 |

    FO3's 0 / 935 confirms that the GECK writes the class attributes into `DATA` when auto-calc is on. The `DATA` bytes are therefore the engine's SPECIAL, and the class is only the source the GECK copies from.
  - Seed vs wiki. The probe (`npchp`) runs the current tree's `derive_resolved_actor_values`:

    | NPC | auto-calc | `DATA` END | class END | Base | L | Seeded | Wiki HP | `base+5·(DATA END−1)+5·(L−1)` |
    |---|---|---|---|---|---|---|---|---|
    | GhoulJasonBright | off | 7 | 4 | 80 | 6 | **120** | 135 | 135 |
    | MartinaGroesbeck | off | 4 | 6 | 40 | 6 | **90** | 80 | 80 |
    | GSEasyPete (control) | off | 4 | 4 | 50 | 1 | 65 | 65 | 65 |

  - All 15 of the #5238 vanilla samples are auto-calc, or (Easy Pete) carry a `DATA` SPECIAL equal to the class's. The pin set therefore cannot detect this.
- **Impact**:
  - About 1,500 FNV and 700 FO3 actors, disproportionately the hand-authored named NPCs that quests and dialogue target, spawn with the class's SPECIAL.
  - That error flows into everything SPECIAL drives: Health (via END), the 13 auto-calc skills, Carry Weight, Crit, the resists, and every `GetActorValue <SPECIAL>` / skill CTDA.
  - Skills for these actors are also the class-derived formula rather than their authored `DNAM` values. That half is still genuinely unparsed.
- **Related**: #2957 (the closed deferral); #5005 (parses the sub-record); #5238 (now half-reads it); `/audit-esm` Dim 4 owns the parse.
- **Suggested Fix**:
  1. In the FO3/FNV `DATA` arm, store the 7 SPECIAL bytes, for example as `NpcRecord::data_attributes: Option<[u8; 7]>`.
  2. In `derive_autocalc_actor_values`, use them when `acbs_flags & 0x10 == 0`. Read the flag from the resolved `Use Stats` record, which is the record the attributes come from.
  3. Correct the module doc so it says only the `DNAM` skill half remains unparsed.
  4. Add a real-master `#[ignore]` pin for GhoulJasonBright = 135 and MartinaGroesbeck = 80.

_Source: `AUDIT_CHARACTER_2026-10-05.md` (CHAR-2026-10-05-D4-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
