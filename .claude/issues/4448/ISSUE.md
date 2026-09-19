# D1-02: D1-02: CharacterRulesProfile used as a game-identity oracle outside the character layer (xNVSE dialect, CTDA fn-586)

- **Labels**: low,character,scripting,bug
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4448

---

**Source**: docs/engine/charal.md §2 (runtime reads canonical state, no per-game branches downstream); profile.rs:64-68 ("This is deliberately data: consumers do not branch on game identity").

**Description**

Two non-character consumers `==`-compare `CharacterRulesProfile` as a game identity: (1) `attach_scpt_script`'s script-dialect selection — `GameKind::Fallout3NV if index.character_rules == CharacterRulesProfile::FALLOUT_NEW_VEGAS => ObscriptDialect::Xnvse` (`byroredux/src/cell_loader/references/attach.rs:273-282`); (2) `restoration_plan`'s condition whitelist — `cond.function_index == 586 && cond.param_1 == 0 && index.character_rules == CharacterRulesProfile::FALLOUT_NEW_VEGAS` (`crates/plugin/src/consumables.rs:92-94`).

**Evidence**

Both are `game ==` compares through the profile. The data branched on (script-extender dialect; CTDA function-586 semantics) is not character-ruleset data — the profile is being borrowed as the codebase's only FO3-vs-FNV discriminator.

**Impact**

No wrong behavior today (classification pinned against real headers by `esm_header_selects_one_canonical_character_profile`). But the character layer silently becomes load-bearing for the scripting and consumable layers: a future profile restructure changes script-dialect selection and condition gating as an untested side effect.

**Related**

CHARAL audit 2026-09-19 D1-01 (same off-seam shape, character-data version).

**Suggested Fix**

Either promote the dialect/condition-function data onto explicit per-game policy rows of their own, or document on `CharacterRulesProfile` that it doubles as the canonical FO3/FNV discriminator and pin both consumers with tests. Cheapest honest step: the doc note + tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D1-02, /audit-character 2026-09-19, HEAD `479163836`).*