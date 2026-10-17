# CHAR-2026-10-16-L-01: `skyrim_class_skill_points` assigns leftover points lower-index-first; the page's own worked example and the baked data say higher-index-first

Labels: low,character,bug,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (±1 on a few skills, but 273 of 929 sampled NPCs)
- **Dimension**: Population Boundary / Derived Formulas
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs` `skyrim_class_skill_points` (`order.sort_by_key(|&i| (Reverse(weights[i]), i))`) and its test comment "then lower index first"
- **Status**: NEW
- **Source**: UESP *CLAS* Skills prose says "lower indices are ordered first", but the same page's worked example (TrainerRestorationMaster Priest) lists the weight-2 skills Enchanting (23), Illusion (21), Alchemy (16), Smithing (10) — descending — and its Attributes paragraph states "reverse actor value index order". The page is internally inconsistent; the master settles it.
- **Description**: The implementation took the prose. `skyrim_class_points_follow_the_documented_assignment_order` uses level 2 / weights (4,3,2,2,2,2), where leftovers always complete a round, so it cannot distinguish the two orders.
- **Evidence**: Oracle over 929 auto-calc NPCs: ascending tie-break reproduces all 18 baked skills for 643, descending for 916. Where order matters (280 NPCs) descending is right for 273, ascending for 0, neither for 7 (all reach skill 100, the documented unmodelled saturation regime).
- **Impact**: ±1 on a few skills of ~29 % of auto-calc Skyrim NPCs (Speechcraft feeds the bribe; Lockpicking/Pickpocket feed their checks).
- **Related**: M-01 (same allocator), L-08.
- **Suggested Fix**: Sort ties by `Reverse(i)`; add a case whose leftover cuts a tied round mid-way (e.g. weights (3,3,3,…), 2 leftover points) and cite the Skyrim.esm DNAM check.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
