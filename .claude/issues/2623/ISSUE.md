# SF-D3-02: LIST/MAPC element counts read as i32-to-usize, negative count panics allocation

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2623
**Finding ID**: SF-D3-02

**Severity**: MEDIUM (escalates to HIGH once #2359/#1289 Phase 2 starts calling `parse` on real archives)
**Dimension**: 3 (CDB Material Database)
**Location**: `crates/sfmaterial/src/reader.rs:372-373,389-390`
**Status**: NEW

## Description
`LIST`/`MAPC` element counts are read as `i32` and cast to `usize`:
`let count = cur.read_i32()? as usize;` — a negative count sign-extends to
~1.8e19, and `Vec::with_capacity` panics. Even a plausible positive
corruption (`count = 100_000_000`) reserves ~5.6 GB before a single element
is read, with no bound against `payload.len()` (which is known and tightly
bounds the count — every element consumes ≥1 byte). Gibbed's
`ConsumeList`/`ConsumeMap` use no reserve and a bounded `for` loop, so a
negative count there just produces an empty collection.

## Evidence
```rust
// crates/sfmaterial/src/reader.rs:372-373, 389-390
let count = cur.read_i32()? as usize;   // negative -> ~1.8e19 after cast
// ... Vec::with_capacity(count) with no bound against payload.len()
```

## Impact
`ComponentDatabaseFile::parse` — the only caller that reaches
`consume_list`/`consume_map` — is not invoked in production today (Phase 1
stops at `probe_header`), hence MEDIUM not HIGH. This is the second half of
"the CDB allocation-safety pair" flagged alongside SF-D3-01 — same root
cause, cheaper to fix in the same patch, and this finding's own report notes
it is a **hard prerequisite** for #2359/#1289 Phase 2 (per-field CDB
extraction), the single highest-value remaining Starfield fidelity item —
fix before or alongside that work starts, not after.

## Suggested Fix
`usize::try_from(cur.read_i32()?).map_err(...)?` plus
`Vec::with_capacity(count.min(payload.len()))`.

## Related
SF-D3-01 (same root cause, live today).

## Completeness Checks
- [ ] **TESTS**: A negative-count and an oversized-positive-count fixture both assert `Err`, not panic
