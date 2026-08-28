# #3485 — CHAR-2026-08-27b-D6-02: audit-character SKILL.md pins `DerivedStatFormula` at 32 B; it has been 36 B since the #2939 floor field landed

**Labels**: documentation, low, character, doc-rot
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: LOW
**Dimension**: Coverage, Documentation & Doctrine Drift
**Game**: all
**Location**: `.claude/commands/audit-character/SKILL.md:188`
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D6-02), HEAD `969d81c8`

## Description

Dimension 2's checklist instructs the auditor to verify that `DerivedStatFormula` *"is still `Copy` + 32 B"*. It is **36 B**, and has been since `clamped_below`'s `floor: f32` and the `base_reads: u8` bitfield landed under #2939.

`crates/core/src/character/derived.rs:23` states *"[`DerivedStatFormula`] is `Copy` and 36 bytes"*, pinned by the live test `formula_is_thirty_six_bytes_and_copy` (`derived.rs:340-345`, `assert_eq!(std::mem::size_of::<DerivedStatFormula>(), 36)`).

An auditor following the checklist literally would report a **false positive** against a struct-size contract the code already pins with a live test. `_audit-common.md`'s own symbol-advisory rule exists precisely because *"`GpuMaterial` still being documented at 300 B after it grew to 348 B … is a wrong number in a GPU layout contract, not a typo."* The same standard applies to a `Copy` formula struct held by the thousand in a flat `Vec`.

Same class as CHAR-2026-08-24-D6-01 / #3271 (a stale fact in the skill file that directs the audit rather than in the code audited), one dimension over.

## Evidence

```
$ grep -n "32 B" .claude/commands/audit-character/SKILL.md
188:  identity crept in, and that `DerivedStatFormula` is still `Copy` + 32 B.

$ grep -n "36 bytes\|size_of::<DerivedStatFormula>" crates/core/src/character/derived.rs
23://! [`DerivedStatFormula`] is `Copy` and 36 bytes; a per-game
345:        assert_eq!(std::mem::size_of::<DerivedStatFormula>(), 36);
```

## Impact

Bounded — one checklist line that manufactures a false finding. The path/symbol validate gate cannot catch it (`32 B` is neither a path nor a backticked symbol).

## Related

- #3271, #3236, #3143 (the same skill-file-drift family)

## Suggested Fix

Change `32 B` to `36 B` and cite `formula_is_thirty_six_bytes_and_copy` by name so the next size change fails the symbol advisory instead of silently re-rotting.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other numeric size/count pins across `.claude/commands/*/SKILL.md` that no symbol advisory covers)
