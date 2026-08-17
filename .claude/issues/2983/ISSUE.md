# TD9-2026-08-16-01: alias_flags_has_recognizes_every_named_bit is tautological in its main loop and its roster is hand-maintained with no parity check

**Issue**: #2983
**Severity**: LOW
**Dimension**: 9 — Test Hygiene (green-by-construction)
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 9 — Test Hygiene, green-by-construction). Effort: trivial.

**Location**: `crates/plugin/src/esm/records/misc/quest.rs`:1905-1954, against :226-232

## Description

The test's doc comment states its purpose: *"guards against a copy-paste bit-value typo in the catalog (each must be its own distinct, correctly-shifted bit)"*. Two of its three assertions do real work; **the headline one cannot fail**:

```rust
let combined = AliasFlags(ALL_FLAGS.iter().fold(0u32, |acc, &f| acc | f));
for &flag in ALL_FLAGS {
    assert!(combined.has(flag), "bit {flag:#x} not set in the combined mask");
}
```

`(a | b | c) & a != 0` is true for every non-zero `a` **by construction** — the loop is an identity over the OR-fold that produced it. It can only fail if a constant is literally `0`.

The `sorted.dedup()` length check does catch two constants sharing a value, and `!combined.has(0x8000_0000)` catches a bit-31 constant. Neither catches a constant that is a **wrong but distinct** bit (the exact defect the doc names), nor a multi-bit value, which the "correctly-shifted" claim asserts against.

The deeper issue is the roster: `ALL_FLAGS` is a hand-copied list of the 25 constants with **no parity check against the declarations**. A 26th constant added tomorrow is never exercised and the test stays green — while the `#[allow(dead_code)]` block comment at :226-232 asserts *"Every constant is exercised by an `AliasFlags::has` assertion in the test module below"*, a claim that silently becomes false.

The codebase already solved this exact problem: `dbg_bits_catalog_covers_every_dbg_constant` (`crates/renderer/src/shader_constants.rs`:62-80) counts `pub const DBG_` occurrences in the source text specifically so the catalog *"cannot silently drift behind a new constant again"*. **That pattern was not applied here.**

## Evidence

Quoted above; the parity-check counterexample is `shader_constants.rs`:62-80.

No wire-level test anywhere decodes a real `FNAM` payload and asserts a named alias flag, so the 25 values have no external authority behind them either — deliberately **not claimed as wrong** here, only as unverified.

## Impact

A test that reads as a value guard and is a presence guard.

Cost today is confidence, not behaviour: five of the constants drive live alias-fill policy in `crates/scripting/src/scene/quest_alias.rs`:487-568 (dead-actor eligibility, reservation reuse, closest-match), so a wrong-but-distinct bit there **silently changes which references fill a quest alias**.

## Suggested Fix

Add the declaration-count parity assertion (copy the `dbg_bits_catalog_covers_every_dbg_constant` shape), and replace the tautological loop with `assert_eq!(flag.count_ones(), 1)` plus an explicit `assert_eq!(ALIAS_FLAG_X, 0x…)` value pin per constant.

**Do not guess the bit values** — pin them from the existing declarations, and treat "no external authority for the 25 values" as a separate, unresolved question rather than inventing one.

## Related

- TD8-2026-08-16-01 / #2982 (the same block)
- #1482, #1860 (the two prior rounds of exactly this defect on the `DBG_*` catalog)

## Completeness Checks
- [ ] **PARITY**: A declaration-count check fails when a 26th constant is added without a test entry
- [ ] **NON-TAUTOLOGICAL**: The replacement assertion can actually fail on a wrong-but-distinct bit
- [ ] **NO-GUESSING**: Value pins come from the declarations, not from an invented spec
- [ ] **BLOCK-COMMENT**: The `:226-232` claim "every constant is exercised" is true after the fix
- [ ] **TESTS**: A deliberately-wrong bit value fails the suite

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2983 --json state` when live state is needed.*
