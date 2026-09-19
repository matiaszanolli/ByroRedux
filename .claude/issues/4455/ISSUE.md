# D3-03: D3-03: charal.md §5 cites profile.rs:82-87 for the OBLIVION const — the const now lives at 92-98

- **Labels**: low,character,documentation,doc-rot,game:oblivion
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4455

---

**Source**: `crates/core/src/character/profile.rs:92-98` (the live const; shifted down by #3390's `creature_stats` field insertion, 2026-08-28).

**Description**

`docs/engine/charal.md:348` reads "`CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None` (`crates/core/src/character/profile.rs:82-87`)" — lines 82-87 today span a blank line, the `impl` header and the `NONE` const; the cited field is now at :97. The claim itself is still true and pinned by `oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate`; only the line-range rotted.

**Evidence**

profile.rs:92-98 vs the cited range; `git log` attributes the shift to #3390.

**Impact**

Citation rot — a reader auditing reachability lands 10 lines off.

**Related**

#4355 (the post-#3848 sweep this pointer survived).

**Suggested Fix**

Re-point to `profile.rs:92-98`, or drop line ranges in favor of symbol names (ranges rot; `CharacterRulesProfile::OBLIVION` doesn't).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D3-03, /audit-character 2026-09-19, HEAD `479163836`).*