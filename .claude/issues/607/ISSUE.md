# D4-NEW-01: NiFogProperty / NiWireframeProperty / NiDitherProperty / NiShadeProperty parsed but never consumed

## Finding: D4-NEW-01

- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`
- **Game Affected**: Oblivion (rare), FO3/FNV (rare)
- **Location**: [crates/nif/src/blocks/mod.rs:336-345](crates/nif/src/blocks/mod.rs#L336-L345) (parsed), [crates/nif/src/import/material.rs](crates/nif/src/import/material.rs) (no consumer for any of these four)

## Description

Four legacy `NiProperty` types reach `parse_block` and are stored on `NifScene`:

- `NiFogProperty` — full struct with `flags`, `fog_depth`, `fog_color` ([properties.rs:1203-1239](crates/nif/src/blocks/properties.rs#L1203-L1239))
- `NiWireframeProperty` — reduced to `NiFlagProperty` (mod.rs:340)
- `NiDitherProperty` — reduced to `NiFlagProperty` (mod.rs:344)
- `NiShadeProperty` — reduced to `NiFlagProperty` (mod.rs:345)

None is checked in `extract_material_info`. Searching `crates/nif/src/import/material.rs` for these names returns zero hits.

Companion to closed #558 (which added the NiFogProperty parser to the NIF-13 tail-types bucket).

## Impact

Rare in shipped content (`NiFogProperty` ≈ 1 occurrence in vanilla Oblivion; the other three are mostly editor/debug). Per-mesh fog override falls back to global fog. Wireframe and flat-shading visual styles for debug content render as solid-smooth.

## Suggested Fix

Defer until a target asset surfaces a visible gap. If addressed:

- Extend `MaterialInfo` with `fog_overrides: Option<(f32 /*depth*/, NiColor, u16 /*flags*/)>`, `wireframe: bool`, `flat_shaded: bool`.
- Add Vulkan pipeline variants for `polygonMode = LINE` (wireframe) and flat-shading via vertex-attribute provoking-vertex toggle.

## Related

- #558 (closed): NIF-13 tail types — added the parser.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: All four properties share the same import-side gap; one PR.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Round-trip test per property — scene with the property → MaterialInfo carries the field.

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md`._
