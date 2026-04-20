# Issue #441

FO3-REN-H1: SF_DOUBLE_SIDED (0x1000 bit 12) is Unknown_3 on FO3 flags1 — wrong backface culling

---

## Severity: High

**Location**: `crates/nif/src/import/material.rs:22,643,657`

## Problem

`SF_DOUBLE_SIDED = 0x1000` (bit 12) is applied to `shader_flags_1` for both `BSShaderPPLightingProperty` (line 643) and `BSShaderNoLightingProperty` (line 657). The FO3/FNV flag vocabulary does not match Skyrim/FO4:

- On FO3/FNV `Fallout3ShaderPropertyFlags1` bit 12 is **`Unknown_3`** — a debug bit that crashes the original game.
- FO3 Double_Sided lives on `flags2` bit 4 (0x10).
- On FO3 `flags2` bit 4 is actually `Refraction_Tint`, NOT Double_Sided — that name is Skyrim/FO4 (`SkyrimShaderPropertyFlags2.Double_Sided`) on the same bit position.

There is no shared mapping. The flag vocabulary is per-game.

## Impact

Every FO3/FNV BSShaderPP mesh that set Unknown_3 renders back-face-culled even when the author marked Double_Sided. Foliage, hair, glass, banner cloth, and NPC eyebrows all fall through to `NiStencilProperty` at lines 666-672 — which only helps if the exporter added one (most vanilla FNV/FO3 meshes did not).

## Fix

Per-game dispatch:
```rust
fn is_two_sided(game: GameKind, flags1: u32, flags2: u32) -> bool {
    match game {
        GameKind::Fallout3NV => flags2 & 0x10 != 0,   // flag2 Refraction_Tint bit reused; verify
        GameKind::Skyrim | GameKind::Fallout4 => flags2 & 0x10 != 0,
        _ => false,
    }
}
```

Apply at both PPLighting (line 643) and NoLighting (line 657) branches. Never test `flags1 & 0x1000`.

Coordinates with #437 (GameVariant enum) and #414 (FO4 SLSF1_/SLSF2_ named bitflags).

## Completeness Checks

- [ ] **SIBLING**: Audit every `SF_*` / `SLSF*` constant usage in `material.rs` — most likely have the same per-game ambiguity
- [ ] **TESTS**: Regression tests for FO3/FNV/Skyrim/FO4 two-sided detection per game
- [ ] **DOCS**: Shader flag table (per-game bit → meaning matrix) in `docs/engine/shader-flags.md`

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-REN-H1)
