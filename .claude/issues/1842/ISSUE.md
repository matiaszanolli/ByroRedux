# NIF-D2-04: FLAGS_U32_THRESHOLD doc cites a nonexistent nif.xml token #BS_GTE_26# (GTE vs the actual GT gate)

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1842
**Labels**: documentation, nif-parser, low

**Severity**: LOW (doc rot on a boundary constant)
**Dimension**: Version Gating
**Location**: `crates/nif/src/version.rs:322-325`
**Status**: NEW

## Description

**Game Affected**: none today (code is correct); a future "align to the doc" edit would mis-parse any `bsver == 26` content boundary

The constant's doc reads "Corresponds to nif.xml's `#BS_GTE_26#` predicate". nif.xml defines no such token (verified against the verexpr table + zero grep hits for `#GTE# 26`); the actual gate is the inline `vercond="#BSVER# #GT# 26"` on `NiAVObject.Flags` (nif.xml:3442). Both call sites use the correct operators (`> 26` flags width at `base.rs:82`; `>= 26` as the negation of the different `#BSVER# #LT# 26` ambient/diffuse gate at `properties.rs:44`), but the doc's "GTE" contradicts the "GT" semantics on the exact boundary value the constant exists to pin.

## Evidence

`grep -c "GTE_26\|#GTE# 26" nif.xml` → 0; nif.xml:3442 `<field name="Flags" type="uint" ... vercond="#BSVER# #GT# 26">`. Confirmed live: `version.rs:322-325` currently reads:
```rust
/// BSVER threshold at which NiAVObject `flags` widens from u16 to
/// u32 (`bsver > FLAGS_U32_THRESHOLD`). Corresponds to nif.xml's
/// `#BS_GTE_26#` predicate.
pub const FLAGS_U32_THRESHOLD: u32 = 26;
```

## Impact

Misleading citation on an off-by-one-critical constant.

## Suggested Fix

Reword to "nif.xml gates `NiAVObject.Flags` on the inline `#BSVER# #GT# 26` (no named token); u16 at `bsver <= 26`."

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only fix; no behavior change
