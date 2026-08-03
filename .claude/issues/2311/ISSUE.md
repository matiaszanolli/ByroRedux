# TD1-083: crates/nif/src/import/tests.rs newly crossed 2000 LOC

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/nif/src/import/tests.rs` (2030 LOC)
**Labels**: low, nif-parser, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
Crossed via a single +112-line commit (#2206, 3 new `NiBillboardNode` mode
regression tests). 100% `#[cfg(test)]` content, zero production code — same
low-risk shape as prior resolved crossings (`anim/tests.rs`, `material.rs`).

## Suggested Fix
Split into per-topic siblings: `transform.rs`, `material_texture.rs`,
`bs_subclass.rs`, `particle.rs`, `furniture.rs`, `billboard.rs`, mirroring the
already-closed `anim/tests/` precedent.

## Age / Effort
Crossed today. Effort: small (mechanical, low-risk).
