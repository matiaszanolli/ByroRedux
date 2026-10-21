# #6285: CHAR-2026-10-21-L-03: Prose still says Skyrim NPC pools are race base + ACBS offset only, and that `with_gmst` handles only `SkillXp` (stale since #5914 / #5915)

**Labels**: low,character,documentation,doc-rot,game:skyrim

**Source**: docs/audits/AUDIT_CHARACTER_2026-10-21.md · **Severity**: LOW · **Dimension**: Coverage & Doctrine · **Location**: - `crates/core/src/character/profile.rs:353-365` (`SKYRIM` row comments: "#4454 … the class term is … uncoded, so leveled NPCs get flat pools", and "`LevelingModel::with_gmst`'s one handled variant (`SkillXp`) … without this arm `with_gmst` executed only inside its own unit test"). - `crates/plugin/src/esm/records/actor_value_derive.rs:11-15` (module doc) and `:138-140` (`derive_npc_actor_values` doc). - `docs/feature-matrix.md:303-305`.

**Audit status**: NEW. #6082 corrected the Skyrim capture and `derive_skyrim_actor_values`' own doc, not these.

- **Game**: skyrim
- **Source**: `derive_skyrim_actor_values` doc (`actor_value_derive.rs:308-316`, #5914: the `10·(level−1)` class term and Health `5·(level−1)` are implemented); `LevelingModel::with_gmst` `XpCurve` arm (#5915).
- **Impact**: Doc rot only, but in exactly the places the next audit and the feature matrix read first.
- **Suggested Fix**: Update the three Skyrim-pool descriptions to name the class/level term. Drop the "one handled variant" comment, or reword it to say Skyrim was the first family wired to `with_gmst`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other prose describing the Skyrim pool derivation (ROADMAP, skill docs))
