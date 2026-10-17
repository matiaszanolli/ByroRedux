# CHAR-2026-10-16-M-01: Skyrim NPC Health/Magicka/Stamina omit the class/level term; the "captured nowhere" premise of that gap is stale

Labels: medium,character,bug,esm-plugin,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` `derive_skyrim_actor_values` (doc "Documented gap (#4454)" + the three-pool loop); `crates/plugin/src/esm/records/actor/class.rs` Skyrim `DATA` arm (reads bytes 6..24 only)
- **Status**: NEW (Related: #4454, closed as a documented gap whose reason no longer holds)
- **Source**: UESP *Skyrim Mod:Mod File Format/CLAS* "Weight Calculations / Attributes" (local `/mnt/data/src/reference/uesp-wiki/Skyrim Mod/Mod File Format/CLAS.wiki`): `Attribute = racial + NPC offset + 10·(L−1)/(sum of attribute weights)·weight`; `Health = racial + offset + 5·(L−1) + 10·(L−1)/sum·healthWeight`; leftovers one at a time by weight desc, ties in *reverse* actor-value order (Stamina, Magicka, Health). Weights are `CLAS DATA` bytes 0x20/0x21/0x22. `charal-skyrim-ruleset.md` ~605-609 records only "0-10/level from class".
- **Description**: The function emits `race.starting_X + offset` and nothing else. The deferral said the class pool-growth numbers are "captured nowhere"; the commit that added skills (`7c740eca0`) reads the same UESP CLAS page, which documents this term two paragraphs below the skills formula. Only `CLAS DATA` 0x20..0x22 needs parsing.
- **Evidence**: Over 929 non-templated, auto-calc, non-level-multiplier vanilla NPCs compared against the baked `DNAM` calculated pools (UESP *NPC_*, bytes 36-41), the documented formula reproduces **929/929** (all three pools exact). The code matches Health for 302/929, Magicka 542/929, Stamina 355/929. Health shortfall median 25 HP, max 1242 HP; 627 of 929 (67 %) spawn with too little Health.
- **Impact**: Every leveled Skyrim NPC spawns under-HP and under-Stamina: time-to-kill, bleedout thresholds and `GetActorValue Health` conditions read the low value. The level-1 player is unaffected.
- **Related**: L-01 (same allocator), L-08.
- **Suggested Fix**: Parse the three attribute weights into `ClassRecord`; add the weighted `10·(L−1)` share (shared leftover allocator with `skyrim_class_skill_points`, reverse-index tie) and `5·(L−1)` for Health; update the #4454 comment; pin with the Saadia (0x13BA2, level 4) numbers.
- **Note**: Follow-up to closed #4454 (documented gap whose stated reason no longer holds).

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
