# FNV-2026-08-26-D4-04

**Issue**: #3339
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/esm/records/actor/mod.rs:1537-1541`, field at `:581`.

**Premise verified**:

```rust
let combat = if sub.data.len() >= 12 { r.u32_or_default() as u8 } else { 0 };
```

with `pub combat_reaction: u8` at `:581`. The comment at `:1529-1532` justifies the
4-byte read as guarding against "any future mod that extends the enum past 255" —
but the `as u8` cast truncates to exactly the same 8 bits the pre-#482 code read.
The read *is* needed for cursor alignment; the *stored width* is not what the
comment claims.

**Evidence**: all 1,314 FNV `XNAM` sub-records are 12 bytes, and the reaction
values are `{0: 179, 1: 264, 2: 472, 3: 399}` — vanilla never exceeds 3, so there
is no live data loss. `modifier` is `{0: 1149, ±5…±100: 165}`.

**Impact**: doc/type inconsistency only; a future >255 enum still truncates.

**Fix sketch**: widen `combat_reaction` to `u32`, or amend the comment to say the
4-byte read exists for cursor alignment and the enum is deliberately narrowed.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
