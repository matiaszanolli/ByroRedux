# CHAR-D4-07: Affliction's doc comment states two different struct sizes (24 vs 40 bytes)

- **Issue**: [#2954](https://github.com/matiaszanolli/ByroRedux/issues/2954)
- **Finding ID**: `CHAR-D4-07`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2954 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: fnv / fo3
- **Location**: `crates/core/src/character/resistance.rs:48-64` (`Affliction` doc comment)
- **Status**: NEW
- **Source**: n/a — internal doc contradiction, no capture-document constant involved.
- **Description**: The comment says "**24 bytes**, `Copy`; the `&'static str`
  EditorIDs are resolved …" and then, two lines later, "**40 bytes** (two
  `&'static str` fat pointers + two `f32`), `Copy`." The pinned test
  `descriptors_are_copy_and_compact` asserts `size_of::<Affliction>() == 40`, so the
  first sentence is stale — a leftover from a pre-EditorID shape.
- **Evidence**: both sentences are live in the same doc block; the 24-byte claim has
  no supporting assertion anywhere.
- **Impact**: Cosmetic, but this crate uses struct-size assertions as real contracts
  (`AvPenalty` 8 B, `ActiveAffliction` 24 B, `FactionRepThresholds` 6 B), so a size
  claim that is wrong in the doc erodes the value of the ones that are right.
- **Related**: —
- **Suggested Fix**: Delete the "24 bytes" sentence.

---

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
