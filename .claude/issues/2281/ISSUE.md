# NIF-D2-02: Bare bsver literals in NifVariant::detect and sequence.rs bypass named-constant doctrine

URL: https://github.com/matiaszanolli/ByroRedux/issues/2281
Labels: bug, nif-parser, low, tech-debt, nif

**Severity**: LOW
**Dimension**: Version Gating
**Game Affected**: None functionally today (values agree with the equivalent named constants) — maintainability/drift-risk only.
**Location**: `crates/nif/src/version.rs:529-548` (`NifVariant::detect`); `crates/nif/src/blocks/controller/sequence.rs:177`

## Description

`version.rs`'s own `bsver` module doc comment (lines 311-314) mandates using named constants over bare decimal literals in raw `bsver` comparisons. `NifVariant::detect`'s match arms hardcode `34`/`83`/`100`/`130`/`155`/`170` instead of `bsver::FO3_FNV`/`SKYRIM_LE`/`SKYRIM_SE`/`FALLOUT4`/`FO76`, defined a few dozen lines above in the same file. Today the literals agree with the constants (both `detect_*` unit tests and `bsver_values()` pin them independently), so there is no live misclassification — but the two are wired independently, so a future correction to e.g. `bsver::FALLOUT4` (the kind of fix #937/#1901/#2168 made elsewhere in this file) would silently desync `detect()`'s routing boundary from the constant every downstream feature gate uses. Separately, `controller/sequence.rs:177` uses `bsver > 0` where three other call sites in the crate use the `bsver > crate::version::bsver::PRE_BETHESDA` idiom (functionally identical, `PRE_BETHESDA == 0`).

## Evidence

```rust
// version.rs:532-538 (current)
(11, uv2) if uv2 < 34 => Self::Fallout3,     // bsver::FO3_FNV == 34
(11, 34) => Self::FalloutNV,
(12, uv2) if uv2 <= 83 => Self::SkyrimLE,    // bsver::SKYRIM_LE == 83
(12, uv2) if uv2 <= 100 => Self::SkyrimSE,   // bsver::SKYRIM_SE == 100
(12, uv2) if uv2 < 130 => Self::SkyrimSE,    // bsver::FALLOUT4 == 130
(12, uv2) if uv2 < 155 => Self::Fallout4,    // bsver::FO76 == 155
(12, uv2) if uv2 < 170 => Self::Fallout76,
```

```rust
// controller/sequence.rs:177
let priority = if bsver > 0 { stream.read_u8()? } else { 0 };
// three other call sites in the crate instead write:
// bsver > crate::version::bsver::PRE_BETHESDA
```

## Impact

No current parse-correctness impact; pure drift-risk between `detect()`'s hardcoded boundaries and the named constants they're supposed to mirror.

## Suggested Fix

Rewrite `detect()`'s match arms to reference the named `bsver::*` constants instead of literal values; change `sequence.rs:177` to `bsver > crate::version::bsver::PRE_BETHESDA`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other bare-literal `bsver` comparisons exist elsewhere in `crates/nif/src/` beyond the two sites named here
- [ ] **TESTS**: Existing `detect_*` unit tests / `bsver_values()` continue to pin the same boundaries after the literal→constant rewrite (no behavior change expected)

