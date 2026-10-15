# TD3-2026-10-15-04: Skyrim NPC/player skill population (7c740eca0) is not reflected in `feature-matrix.md`, `charal.md` or the `actor_value_derive.rs` header

Labels: low,character,game:skyrim,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-04`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:268, 296-299`; `docs/engine/charal.md:117`; `crates/plugin/src/esm/records/actor_value_derive.rs:1-14`
- **Status**: NEW
- **Age**: `7c740eca0` (2026-10-14)
- **Effort**: trivial
- **Description**: `feature-matrix.md:296-299` says Skyrim population derives Health/Magicka/Stamina "— no other actor values or skills"; `derive_skyrim_actor_values` now appends `derive_skyrim_skills` (18 skills, `SKYRIM_SKILLS`). Row `:268` lists "Health+Magicka+Stamina" only. `actor_value_derive.rs:11` describes Skyrim as "seeds Health from the race's starting Health plus ... ACBS Health offset" (Magicka/Stamina were already missing, skills now too). `charal.md:117` still gives the derive site as "(today: FNV/FO3)" though Skyrim and FO4-stored paths live there. `charal-skyrim-ruleset.md` itself does not call skill derivation deferred (lead 6 checked: only the Armor-Rating NPC `0.015` constant is named "not modelled", which is still true).
- **Evidence**: `git show 7c740eca0 --stat`; `grep -n "no other actor values" docs/feature-matrix.md`.
- **Impact**: `feature-matrix.md` is the status floor; it contradicts `scripting.md:1395` ("Skyrim's skills are now populated").
- **Related**: #5000-series CHARAL population work; `bribe.info`.
- **Suggested Fix**: one sentence each in the three places: Skyrim = pools + 18 skills (UESP CLAS formula, stored NPC keeps DNAM).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
