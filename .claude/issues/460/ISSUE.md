# Issue #460

FO3-NIF-L2: BSShader* accessor asymmetry — shader_flags_2() missing; PPLighting has no flags accessors

---

## Severity: Low (API hygiene)

**Location**: `crates/nif/src/blocks/shader.rs:109-112`

## Problem

- `BSShaderNoLightingProperty::shader_flags_1()` accessor exists, but no matching `shader_flags_2()`.
- `BSShaderPPLightingProperty` has neither accessor — callers reach through `.shader.shader_flags_1` directly.

Inconsistent surface complicates per-game flag dispatch (FO3-REN-H1) and shader flag refactors (FO3-REN-L1, #414, #437).

## Fix

Add matching accessors on both:
```rust
impl BSShaderPPLightingProperty {
    pub fn shader_flags_1(&self) -> u32 { self.shader.shader_flags_1 }
    pub fn shader_flags_2(&self) -> u32 { self.shader.shader_flags_2 }
}
impl BSShaderNoLightingProperty {
    pub fn shader_flags_2(&self) -> u32 { self.shader.shader_flags_2 }
}
```

Unify style with `BSLightingShaderProperty`.

## Completeness Checks

- [ ] **SIBLING**: Search for `.shader.shader_flags_` callers — migrate to accessors
- [ ] **LINK**: Precursor refactor for FO3-REN-H1 per-game dispatch

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-NIF-L2)
