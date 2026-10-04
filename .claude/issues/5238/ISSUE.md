# #5238: CHAR-2026-10-03-D4-01: FO3/FNV NPC Health is seeded with the *player* curve; the authored `NPC_ DATA` Base Health and the masters' `fAVDNPCHealth*` GMSTs are ignored, so every auto-calc NPC's Health is wrong (Three Dog 180 vs 25, Easy Pete 180 vs 65)

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: high,character,bug,game:fnv,game:fo3
- **Source report**: docs/audits/AUDIT_CHARACTER_2026-10-03.md

- **Severity**: HIGH
- **Dimension**: Population Boundary (with a Coverage & Doctrine component: the capture self-contradiction)
- **Game**: fo3, fnv
- **Location**:
  - `crates/core/src/character/profile.rs`: the `FALLOUT3` / `FALLOUT_NEW_VEGAS` `npc_stats: NpcStatModel::ClassAutoCalc { health: NpcHealthCurve { … } }` rows, and the `NpcHealthCurve` doc, which reads "A sourced linear END + level curve".
  - `crates/plugin/src/esm/records/actor_value_derive.rs`: `derive_autocalc_actor_values`, at `health_curve.evaluate(endurance, level)`.
  - `crates/plugin/src/esm/records/actor/mod.rs`: `NpcRecord::data_base_health`, which is parsed and has zero readers.
  - `docs/engine/charal-fnv-fo3-ruleset.md`: the derived-table Health row.
- **Status**: NEW. The curve dates from `b434e4c0e`. It was graded PASS by `AUDIT_CHARACTER_2026-08-27b` rows 14–15, `2026-09-11` rows 7–8 and `2026-09-19`, each against capture row 93, which is the "reading the capture as confirmation" trap. `de6bdb381` (#5005, 2026-09-30) began parsing `DATA` Base Health for the corpse rule only.
- **Source**:
  - (1) GECK *Stats Tab - NPC*: "**Base Health:** Health is calculated with Endurance and level. This value is then added to that result."
  - (2) The capture contradicts its own Health row. Under the table it says: "Health: `fAVDHealthLevelMult` = 10 (FO3) / 5 (FNV); base 90 → 100. **Player formulas (NPCs derive separately).** Source: fandom *Hit Points*". The fandom article likewise says "The formulas given below generally only apply to the player character."
  - (3) Both masters author dedicated NPC GMSTs, read with the probe: `fAVDNPCHealthEnduranceMult` (0xAE66A) = **5.0** and `fAVDNPCHealthLevelMult` (0xAE66B) = **5.0**, in both `Fallout3.esm` and `FalloutNV.esm`. The player GMSTs the seed copies are `fAVDHealthEnduranceMult` = 20 and `fAVDHealthLevelMult` = 10 (FO3) / 5 (FNV).
- **Description**: `NpcHealthCurve` is the player Health formula with "sourced" in its doc. The capture marks the row "BUILT (player ruleset + NPC auto-calc seed)". The only sources state, in turn:
  - the player formula does not apply to NPCs;
  - NPC Health adds the record's Base Health;
  - the engine carries separate NPC multipliers.

  The NPC path consumes none of the three. The `data_base_health` field doc says "a positive value stays here for the actor-value seeding path", which describes a consumer that does not exist.
- **Evidence**:
  - **Population.**
    - `FalloutNV.esm`: 3,643 of 3,816 `NPC_` carry `DATA` Base Health > 0. Top values: 50 (1,590), 10 (947), 100 (202), 40, 25, 30 …
    - `Fallout3.esm`: 1,526 of 1,647 carry Base Health > 0. Top values: 10 (612), 50 (531) …
  - **Seed vs in-game totals** (fandom infobox `hp`). Base, END and L come from the probe; Seed is `derive_resolved_actor_values`:

| Game | NPC | Base | END | L | Seed | Wiki HP | Base+5·END+5·L−10 |
|---|---|---|---|---|---|---|---|
| FNV | GSEasyPete | 50 | 4 | 1 | 180 | 65 | 65 |
| FNV | GSSunnySmiles | 70 | 4 | 2 | 185 | 90 | 90 |
| FNV | GSRingo | 75 | 6 | 5 | 240 | 120 | 120 |
| FNV | CraigBoone | 190 | 5 | 5 | 220 | 230 | 230 |
| FNV | Benny | 150 | 5 | 8 | 235 | 205 | 205 |
| FNV | Veronica | 190 | 6 | 5 | 240 | 235 | 235 |
| FNV | RoseofSharonCassidy | 175 | 4 | 5 | 200 | 210 | 210 |
| FNV | RaulTejada | 200 | 4 | 14 | 245 | 280 | 280 |
| FO3 | ThreeDog | 10 | 4 | 1 | 180 | 25 | 25 |
| FO3 | Gob | 10 | 4 | 3 | 200 | 35 | 35 |
| FO3 | Nova | 10 | 4 | 4 | 210 | 40 | 40 |
| FO3 | MoiraBrown | 50 | 4 | 4 | 210 | 80 | 80 |
| FO3 | LucasSimms | 10 | 6 | 6 | 270 | 60 | 60 |
| FO3 | Charon | 200 | 5 | 5 | 240 | 240 | 240 |
| FO3 | Clover | 225 | 4 | 1 | 180 | 240 | 240 |

  - **How to read the table.**
    - The 5·END and 5·L slopes are exactly the authored NPC GMSTs. The same coefficients hold in FO3, whose player level mult is 10. This is the strongest evidence that NPCs run on their own GMST pair.
    - The `−10` constant is an empirical fit only: no document gives it.
    - Two samples were excluded as non-discriminating. Doc Mitchell's page says 50 against a computed 65. Jessup's page lists a different level (10→15 ×1.3) from the record (L8, no PC-mult).
- **Impact**: Every FO3/FNV auto-calc NPC spawns with the wrong Health. Most are 2–7× tankier than in game; the Goodsprings and Megaton populations triple or worse. High-Base companions are under-statted: Clover is seeded at 180 against 240. This affects combat, death, the HUD's target bar, and every `GetActorValue Health` / `GetHealthPercentage`-style CTDA, across the two reference titles of the playable slice. No crash and no failing test. `CREA` creatures are unaffected, because they read `DATA` Health verbatim.
- **Related**:
  - #5005 (closed), which introduced the unread field.
  - #2937 / #4450, the same "NPC scope unsourced" class for AP/Crit.
  - D2-01, which uses the same GECK page.
  - The Skyrim `with_gmst` precedent: `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst`.
- **Suggested Fix**:
  1. Add the Base Health term from the `Use Stats` terminal (`resolved.stats.data_base_health`).
  2. Read the NPC multipliers from `fAVDNPCHealthEnduranceMult` / `fAVDNPCHealthLevelMult`. `index.game_setting_float` is already plumbed.
  3. Source the constant offset before encoding it. The exe default tables or xNVSE are the candidates; the `−10` fit is not a source.
  4. Correct capture row 93 and the `NpcHealthCurve` doc.
  5. Add a real-master `#[ignore]` pin over a few of the samples above.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (FO3 *and* FNV profile rows; `CREA` path unaffected; capture row 93 + `NpcHealthCurve` doc corrected)
- [ ] **TESTS**: A regression test pins this specific fix (real-master `#[ignore]` pin over sampled NPCs)

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-10-03.md` (finding CHAR-2026-10-03-D4-01, /audit-character 2026-10-03, HEAD `2c36c29d8`).*
