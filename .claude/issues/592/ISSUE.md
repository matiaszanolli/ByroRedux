# #592: FO4-DIM3-01: fo4_slsf1 / fo4_slsf2 constants dead in production code

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/592
**Labels**: nif-parser, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 3)
**Severity**: LOW
**Location**: `crates/nif/src/shader_flags.rs:91-193` (definitions); `crates/nif/src/import/material.rs:30-66` (consumers)

## Description

`pub mod fo4_slsf1` and `pub mod fo4_slsf2` define ~40 named bit constants (SPECULAR, SKINNED, CAST_SHADOWS, ALPHA_TEST, GLOW_MAP, REFRACTION, MODEL_SPACE_NORMALS, etc.) added by closed #414 / FO4-D3-M1. The only references outside the definition site are test assertions inside `shader_flags.rs` itself (lines 223-243), which assert cross-game bit equality:

```
$ rg -n "fo4_slsf" --type rust
crates/nif/src/shader_flags.rs:91:pub mod fo4_slsf1 {
crates/nif/src/shader_flags.rs:144:pub mod fo4_slsf2 {
crates/nif/src/shader_flags.rs:223:        assert_eq!(fo4_slsf1::DECAL, skyrim_slsf1::DECAL);
crates/nif/src/shader_flags.rs:224:        assert_eq!(fo4_slsf1::DECAL, fo3nv_f1::DECAL);
crates/nif/src/shader_flags.rs:225:        assert_eq!(fo4_slsf1::DYNAMIC_DECAL, skyrim_slsf1::DYNAMIC_DECAL);
crates/nif/src/shader_flags.rs:237:        assert_eq!(fo4_slsf2::ANISOTROPIC_LIGHTING, 0x0020_0000);
crates/nif/src/shader_flags.rs:243:        assert_eq!(fo4_slsf2::DOUBLE_SIDED, skyrim_slsf2::DOUBLE_SIDED);
```

Production consumers read FO4 shader flags via Skyrim-module aliases (`DECAL_SINGLE_PASS` / `DYNAMIC_DECAL` from `skyrim_slsf1`, `SF2_DOUBLE_SIDED` from `skyrim_slsf2`, `ALPHA_DECAL_F2` from `fo3nv_f2`). Works today by accident — the bits happen to share positions.

## Impact

None today — the bits align. Any future FO4-specific flag addition, or a drift when a newer format reshuffles bits, would silently route through a Skyrim-labelled alias. Adds review load for new shader-flag work.

## Suggested Fix

Pick one:
1. Add `const FO4_SF2_DOUBLE_SIDED: u32 = fo4_slsf2::DOUBLE_SIDED;` next to `SF2_DOUBLE_SIDED` in `material.rs:86`; route FO4 call sites through FO4 aliases. Zero bit-level change, reader clarity +1.
2. Delete the `fo4_slsf1` / `fo4_slsf2` modules; keep a comment "FO4 bits identical to Skyrim for bits touched in production; see shader_flags.rs:223-243 for equivalence proofs." Less code, same outcome.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same review for `fo3nv_f1` / `fo3nv_f2` modules — actually used via `ALPHA_DECAL_F2`, but audit for consistency
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Keep the cross-game equivalence assertions (move into `#[cfg(test)]` if module deleted).
