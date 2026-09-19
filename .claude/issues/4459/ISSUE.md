# D6-01: D6-01: character/mod.rs docstring still says Skyrim's ruleset builder is "not yet reachable" — stale since #3848

- **Labels**: medium,character,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4459

---

**Source**: `crates/core/src/character/profile.rs:61,140,203` (`RulesetBuilder::Skyrim => skyrim_ruleset(resolve)`), pinned by `skyrim_profile_builds_a_ruleset_and_actually_calls_gmst` (profile.rs:305) and the Skyrim ROSTER_CASES corpus row.

**Description**

The module docstring — the entry point every future CHARAL contributor reads first — ends "(Oblivion's and Skyrim's do, and are not yet reachable: #2961's matrix row)" (`crates/core/src/character/mod.rs:61-65`). The Skyrim half is false since `e13985dfc` (#3848, 2026-09-12): the builder arm is present and matched, `build_character_ruleset` is called from the live cell-reference spawn path (cell_loader/references/mod.rs:342), and the corpus gate pins 2 derived rows against real Skyrim.esm. Only Oblivion remains blocked (#3768). `mod_docstring_indexes_every_sub_module` checks module names only, so the drift is test-invisible. Adjacent (same docstring, same fix pass): the `[`components`]` bullet under-lists — it omits the BUILT FactionReputation/FactionStanding/PerkRank that mod.rs:93-95 re-exports.

**Evidence**

mod.rs:64 text vs profile.rs:199-205 match arms; the sentence was added pre-#3848 and #4355's post-#3848 sweep fixed charal.md:353 and feature-matrix.md but not this docstring.

**Impact**

A contributor reading only the entry doc concludes Skyrim still needs wiring — the exact misreading that cost Skyrim five weeks per #3848's own commit message. Entry-point doctrine contradicts live wiring.

**Related**

#3848, #4355, #2959.

**Suggested Fix**

Reword to "(Oblivion's does and is deliberately blocked on a pre-AVIF resolver (#3768); Skyrim's is wired (#3848))". Add FactionReputation to the components bullet. Consider extending the docstring guard to fail on "not yet reachable" unless the named builder is actually `RulesetBuilder::None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D6-01, /audit-character 2026-09-19, HEAD `479163836`).*