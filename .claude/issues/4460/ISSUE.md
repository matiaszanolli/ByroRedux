# D6-02: D6-02: post-#3848 regen doc-rot — deleted oblivion_pool_regen_config cited as live; false "when a live CharacterRuleset lands" trigger phrasing

- **Labels**: medium,character,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4460

---

**Source**: `byroredux/src/boot/schedule/update.rs:378-387` (the real registration), `crates/core/src/character/regen.rs:122-134` (the accurate struct-level doc), `git show e13985dfc` (the deletion).

**Description**

Four sites still describe the pre-#3848 regen world: (1) `docs/engine/charal-oblivion-ruleset.md:411-421` cites a `byroredux/src/main.rs` registration that no longer exists (the real one is `add_exclusive_with_access` in boot/schedule/update.rs:378) and "the new `oblivion_pool_regen_config` in tes.rs" — deleted under #3848; (2) `docs/engine/charal.md:265` (§4.7) names the deleted builder as the thing that "builds one, nothing calls it at load"; (3) `byroredux/src/boot/world.rs:41-43` and (4) `crates/core/src/character/regen.rs:153-157` both say `PoolRegenConfig` arrives "when a live `CharacterRuleset` lands" — a live ruleset has landed on FO3/FNV/FO4/Skyrim since #3170/#3848 and no config follows, because the config is Oblivion-shaped and genuinely blocked on #3768.

**Evidence**

Workspace grep: the only `PoolRegenConfig` constructions are inside `mod tests` in regen.rs; `oblivion_pool_regen_config` survives only in comments and audit archives; main.rs retains no scheduler registrations.

**Impact**

A future regen-wiring pass reading these sites would grep for a constructor that isn't there and treat "did the ruleset land?" as the success condition — which is already satisfied on four games with regen still inert. Comment/doc only; no runtime behavior.

**Related**

#4107 (CLOSED, doc-only — its substance "no production insertion" remains true), #4109 (fixed sibling boot.rs refs — the main.rs copy evaded that grep), #4355, #3848, #3768, #3855.

**Suggested Fix**

Reword the two comment sites to the (correct) update.rs phrasing ("arrives only with Oblivion's wiring — blocked on the pre-AVIF resolver, #3768"); repoint the capture's registration path to boot/schedule/update.rs and replace the builder sentence with regen.rs:130-134's "deleted under #3848, `git show` recovers it" wording; fix charal.md §4.7's parenthetical likewise.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D6-02, /audit-character 2026-09-19, HEAD `479163836`).*