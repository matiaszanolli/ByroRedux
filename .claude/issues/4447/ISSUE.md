# D1-01: D1-01: FO3/FNV body-condition AV seeding is a raw GameKind branch inside the profile-driven population path

- **Labels**: medium,character,bug,game:fo3,game:fnv
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4447

---

**Source**: docs/engine/charal.md §1/§5 ("the per-game seam is the *data* in the tables, never a branch in the consumer", restated at `crates/core/src/character/ruleset.rs:5-7`); charal-fnv-fo3-ruleset.md has no body-condition row — the base-100 constant's only in-repo citation is the code's own "GECK Stats List" comment.

**Description**

`derive_npc_actor_values` (`crates/plugin/src/esm/records/actor_value_derive.rs:227-239`, added by `2e2f40b23`) selects its stat model through the data seam (`index.character_rules.npc_stat_model()` / `creature_stat_model()`) — and then, six lines under a comment reading "Consumers never branch on game identity", gates a new rule on `index.game == GameKind::Fallout3NV` to seed the 7 body-condition AVs at base 100. A per-game character-population rule expressed as a `game ==` compare in the consumer, where every neighbouring rule is a `CharacterRulesProfile` field.

**Evidence**

The branch gates on the broad `GameKind::Fallout3NV` (cannot distinguish FO3 from FNV — the very reason `CharacterRulesProfile` exists, profile.rs:1-7). No profile field owns the base-100 seeding decision. Unit tests cannot exercise it: the `fnv_index_with_class` fixture authors no body-condition AVIFs, so `derives_special_and_skills_from_class` passes with the branch silently no-op'ing; the only exercising test is the `#[ignore]`d real-master test. Skyrim ingestibles are supported (`crates/plugin/src/consumables.rs:161`) while Skyrim actors are excluded by kind, not by data.

**Impact**

Behaviorally correct on every shipped game today. Structural risk: invisible to the profile (a future kind/profile split inherits or loses the rule silently), and the unit suite cannot catch a regression in the branch. CHARAL's first consumer-side game-identity branch since the doctrine was established.

**Related**

CHARAL audit 2026-09-19 D1-02 (same off-seam shape); convention pinned by `fallout_profiles_keep_roster_health_and_ruleset_in_lockstep` (profile.rs:223).

**Suggested Fix**

Add a profile field (e.g. `body_condition_base: Option<f32>`) set on the FALLOUT3/FALLOUT_NEW_VEGAS rows; gate the seeding on it. Add a plugin-crate unit test whose fixture authors one body-condition AVIF and asserts the seed fires for FO3/FNV profiles and does not fire for FO4/Skyrim.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D1-01, /audit-character 2026-09-19, HEAD `479163836`).*