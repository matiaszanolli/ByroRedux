# CHAR-2026-10-16-L-05: `SKYRIM_SKILLS` is a second hand-typed 18-name roster with no non-ignored pin to `SkillSet::SKYRIM`

Labels: low,character,bug,test-gap,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:343-362` (`SKYRIM_SKILLS`) vs `crates/core/src/character/skill.rs:128-160` (`SkillSet::SKYRIM`)
- **Status**: NEW
- **Description**: Since `7c740eca0`/`a8dc9e8e6` the Skyrim skill derivation keys off a plugin-side array rather than the CHARAL roster. Both hold the same 18 EditorIDs (including the `Mysticism` rename that bit #3169) in different orders (AV-index order vs school order). An unresolved name is dropped silently (`index.actor_value_form_id(name)?`), so a roster edit would stop deriving that skill with no diagnostic.
- **Evidence**: `grep SKYRIM_SKILLS` → only `actor_value_derive.rs`; the only guard is the `#[ignore]`d `vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them`; `ROSTER_CASES` (`crates/plugin/tests/parse_real_esm.rs:567`) pins the core roster only.
- **Impact**: None today (the two lists are set-equal); latent silent skill loss.
- **Suggested Fix**: Add a plain unit test asserting `SKYRIM_SKILLS` sorted == `SkillSet::SKYRIM` editor ids sorted (the plugin crate already depends on `byroredux-core`), or build the plugin array from the core roster plus an explicit index map.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
