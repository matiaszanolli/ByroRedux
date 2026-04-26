# Issue #703: O4-05: NiWireframeProperty / NiDitherProperty / NiShadeProperty parsed (NiFlagProperty) but never read

**Severity**: LOW
**File**: `crates/nif/src/blocks/properties.rs:1246-1320` (NiFlagProperty wraps all four), `crates/nif/src/import/material.rs:1018-1022` (only NiSpecularProperty branch)
**Dimension**: Rendering Path

`NiFlagProperty` is a shared struct for the four trivial enable-toggle properties. The import-side `block_type_name()` match at `material.rs:1019` only fires on `NiSpecularProperty`. The other three drop on the floor:

- **NiWireframeProperty.flags & 1**: forces `polygon_mode = LINE`. Oblivion does not ship this on production content but a non-trivial number of FO3/FNV mod meshes do.
- **NiDitherProperty.flags & 1**: legacy 16-bit color dithering hint, no Vulkan analogue. Safe to ignore but should be acknowledged with a comment.
- **NiShadeProperty.flags & 1**: when `0`, forces flat shading (no per-vertex normal interpolation). Used on a handful of Oblivion architectural pieces to fake a faceted look.

**Fix**: Cleanup-tier: cost is one `match` arm and (for shade) a `flat` qualifier on the fragment shader's normal input or a per-instance flag bit.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
