# D2-02: D2-02: RoundMode doc overgeneralizes "Bethesda floors Health" beyond the one sourced case (FO4)

- **Labels**: low,character,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4451

---

**Source**: FO4 only — charal-fo4-ruleset.md:87-88 (`TotalHP = floor(…)`); the FO3/FNV capture (charal-fnv-fo3-ruleset.md:93) states no rounding mode.

**Description**

The `RoundMode` doc comment (`crates/core/src/character/derived.rs:96-98`) states "Bethesda floors Health (`TotalHitPoints = floor(...)`)" unqualified, but only FO4 Health carries a sourced floor; the FO3/FNV Health rows are correctly unfloored (their capture is silent, and integer END/level inputs make floor an identity in practice).

**Evidence**

`fallout.rs:159-163,198-202` (FO3/FNV Health rows) ship `RoundMode::None`. The comment is the nearest reference an implementer of a new row reads, so an overgeneralization here teaches the wrong default — the same footgun class as the #3766 cap-doc fix in this very struct.

**Impact**

A future FO3/FNV-style row could inherit an unsourced floor (wrong for fractional `current()` reads), or a reader could "fix" the FO3/FNV rows to match the comment, believing the doc comment is the sourced form. Behavior-neutral today.

**Related**

#3766 (same class, fixed); #2936/#2939 (convention-documentation precedents in the same module).

**Suggested Fix**

Scope the sentence: "Bethesda floors FO4 Health (`TotalHP = floor(…)` in its own source); the FO3/FNV capture states no mode and those rows ship `None` (exact for their integer domain)."

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D2-02, /audit-character 2026-09-19, HEAD `479163836`).*