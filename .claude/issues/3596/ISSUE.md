# #3596: OBL-D4-01: #3530's APPLY_HILIGHT2 parallax route is unreachable on every vanilla Oblivion mesh — the feature ships inert (0 of 1,430 properties carry a normal/bump slot)

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 4 (Rendering Path for Oblivion Shaders)
**Severity**: HIGH
**Location**: `crates/nif/src/import/material/legacy_properties.rs` (the `APPLY_HILIGHT2` block and the `info.normal_map` population above it); downstream boundary `byroredux/src/asset_provider/texture.rs` (`derive_normal_map_path`)

## Description

#3530's `APPLY_HILIGHT2` → parallax route — landed in `19813460`, "wire Oblivion
APPLY_HILIGHT2 parallax" — **cannot fire on any vanilla Oblivion mesh**. The feature ships
inert. The guard it depends on (`info.normal_map` being `Some`) is never satisfied on this
title, because Oblivion does not put normal maps in NIF texture slots at all.

## Evidence

Current code (`legacy_properties.rs`, verified 2026-08-30):

```rust
if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() {
    if let Some(normal) = info.normal_map {       // <-- never Some on Oblivion
        info.parallax_map = Some(normal);
        info.parallax_height_in_alpha = true;
        ...
```

`info.normal_map` is populated a few lines above from `tex_prop.normal_texture` — a
v20.2.0.5+ slot that does not exist on Oblivion — `.or_else(bump_texture)`.

MEASURED over `Oblivion - Meshes.bsa` + `DLCShiveringIsles - Meshes.bsa`:

```
NiTexturingProperty total        = 34,850    with bump_texture = 14
APPLY_HILIGHT2 properties        =  1,430  across 741 files   <- the exact #3530 population
   ...of which carry bump_texture   =  0  (in 0 files)
   ...of which carry normal_texture =  0
```

**Zero of the 1,430 `APPLY_HILIGHT2` properties carry a normal or bump texture slot**, so
the `if let Some(normal)` guard never passes. Confirmed independently at the import
boundary: `parallax_height_in_alpha = true` on **0** of 35,322 imported Oblivion meshes, and
`textures.height = Some(_)` on **0**.

(The `NiTexturingProperty::apply_mode` doc comment records the population as 1,433 across
741 meshes from a wider 9,537-NIF sweep; the 1,430 above is the same population measured
over the two mesh archives this run covered. The finding does not turn on the 3-property
difference.)

Root cause is structural, not a typo: Oblivion resolves normal maps by **filename
convention**, not by NIF slot — `derive_normal_map_path` in
`byroredux/src/asset_provider/texture.rs` (`<base>_n.dds`, landed under #1303). That
derivation happens **downstream** of `MaterialInfo`, so at the point #3530 tests
`info.normal_map` the Oblivion normal map does not exist yet. Corroborated by a real-data
trace: `normal_slot = None` on all 11 meshes traced through `import_nif_scene` this run.

## Impact

The one Oblivion-specific material feature that shipped is a no-op on 100% of the corpus:
1,430 properties across 741 files (cave and stone architecture, rock clutter) that were
authored with the parallax convention render without it. Because this sits on the exterior
blocker chain, it also means an "Oblivion parallax works" claim cannot currently be
substantiated by any vanilla asset.

## Suggested Fix

Move (or re-evaluate) the `APPLY_HILIGHT2` decision to the boundary where the derived
`_n.dds` path is known: carry `apply_mode == APPLY_HILIGHT2` forward as an **intent flag** on
`MaterialInfo`, and let the asset provider bind it to the derived normal map when one
resolves. No new constant is required — the existing `0.04 / 4.0` engine defaults and the
`PARALLAX_ALPHA_HEIGHT_BIT` transport already exist and are correct.

**Fix together with the alpha-presence gate.** A sibling audit this run reported "#3530's
parallax bit set without an alpha-presence gate". On vanilla Oblivion the bit is never set at
all, so that gate is moot *today* — it becomes live the moment this finding is fixed.

## Related

#3530 (commit `19813460`), #1303 (`derive_normal_map_path`). POM bit-31 masking verified
intact in both marchers (`include/material_sampling.glsl`, `include/ray_hit.glsl`) — not part
of this defect.

## Completeness Checks
- [ ] **SIBLING**: check the slot-7 `NORMAL_ALPHA_SPEC_BIT` branch that installs the same `0.04 / 4.0` defaults — the intent flag must not double-apply with it
- [ ] **CANONICAL-BOUNDARY**: the fix touches `MaterialInfo` → `translate_material`; per-game logic must stay at the NIFAL parser→`Material` boundary, never pushed into shaders/renderer and never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins an `APPLY_HILIGHT2` property whose normal map is supplied by `derive_normal_map_path` actually reaching `parallax_height_in_alpha = true` — the current tests in `import/tests/material_texture.rs` pin only the pre-derivation half
