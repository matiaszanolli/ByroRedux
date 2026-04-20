# Issue #461

FO3-REN-L1: Shader flag constants module-private, lack game-prefixed naming

---

## Severity: Low (code hygiene)

**Location**: `crates/nif/src/import/material.rs:19-22`

## Problem

Named flag constants (`DECAL_SINGLE_PASS`, `SF_DOUBLE_SIDED`, `ALPHA_DECAL_F2`, etc.) are module-private in `material.rs` and lack a consistent `SF_` prefix discipline. Combined with #414 (no SLSF1_/SLSF2_ names on FO4) and #437 (no GameVariant), the flag vocabulary has no type-system support.

This is the enabler bug behind FO3-REN-H1: `SF_DOUBLE_SIDED = 0x1000` is silently reused across games where it means different things.

## Fix

Promote to `crates/nif/src/shader_flags.rs` with per-game prefixed aliases:
```rust
pub mod fo3nv {
    pub const F1_DECAL_SINGLE_PASS: u32 = 0x04000000;
    pub const F1_DYNAMIC_DECAL_SINGLE_PASS: u32 = 0x08000000;
    pub const F2_ALPHA_DECAL: u32 = 0x00200000;
    // F1 bit 12 reserved — do NOT define (it's Unknown_3, crashes game)
    // No F1_DOUBLE_SIDED — that bit lives on F2 only for Skyrim+
}
pub mod skyrim {
    pub const F2_DOUBLE_SIDED: u32 = 0x10;
    // ...
}
```

Replace `material.rs` constant uses with game-prefixed reads, gated by `GameVariant` (#437).

## Completeness Checks

- [ ] **SIBLING**: Coordinate with #414 (FO4 named bitflags) and #437 (GameVariant enum)
- [ ] **TESTS**: Compile-time: importing `skyrim::F1_DOUBLE_SIDED` should fail (constant deliberately absent)
- [ ] **DOCS**: Per-game flag matrix in `docs/engine/shader-flags.md`

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-REN-L1)
