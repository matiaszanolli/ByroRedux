# Positioning & Decals Audit — 2026-04-13

**Focus**: Floating diamonds, visible markers, and mispositioned objects in FNV interiors.

## Executive Summary

| Severity | Count |
|----------|-------|
| HIGH     | 2 |
| MEDIUM   | 2 |
| LOW      | 2 |
| **Total** | **6** |

The floating diamonds and the red sphere have clear root causes:

1. **Editor markers rendered visibly** (HIGH) — BSXFlags bit 5 (editor marker) is parsed but never consumed. The NIF-level `is_editor_marker()` only catches nodes by name prefix; it misses markers that have generic node names. XMarker, XMarkerHeading, PrisonMarker, etc. render as colored shapes.

2. **APP_CULLED flag not checked** (HIGH) — Only HIDDEN (0x01) is checked in the NIF walker. APP_CULLED (0x20) is ignored, causing nodes that Gamebryo marks as "do not render" to be rendered.

3. **Decal projection volumes rendered as literal geometry** (MEDIUM) — FNV decals flagged with DECAL_SINGLE_PASS are pre-authored flat quads positioned coplanar with walls. The depth bias (-8.0/-2.0) handles coplanar z-fighting correctly, but some decal NIFs appear to be slightly offset from their target surface, creating the "floating diamond" look. These are NOT projection volumes — they are pre-authored geometry.

4. **ESM-level marker model path filtering incomplete** (MEDIUM) — The cell loader filters `fxlightrays`, `fxlight`, `fxfog` but NOT `marker*`, `xmarker*`, or other editor-only model paths.

## Findings

### PD-01: BSXFlags editor marker bit (0x20) never consumed
- **Severity**: HIGH
- **Location**: `crates/nif/src/import/mod.rs:241-244` (parsed), `byroredux/src/cell_loader.rs` (never checked)
- **Description**: BSXFlags bit 5 marks an entire NIF as "editor marker — do not render." It is parsed into `ImportedScene.bsx_flags` but the flat import path (`import_nif()`) does not return it, and `cell_loader.rs` never checks it. Editor marker NIFs (the red/green/blue octahedron shapes) render as visible geometry.
- **Fix**: Return `bsx_flags` from `import_nif()`, check `bsx_flags & 0x20 != 0` in cell_loader, skip the entire NIF if set.

### PD-02: APP_CULLED flag (0x20) not checked in NIF walker
- **Severity**: HIGH
- **Location**: `crates/nif/src/import/walk.rs:123,155,202` (only checks 0x01)
- **Description**: `NiAVObject.flags & 0x01` (HIDDEN) is checked. `& 0x20` (APP_CULLED) is not. Gamebryo's APP_CULLED means "application has marked this node invisible." Some collision helpers, LOD placeholders, and editor visualization nodes use this flag.
- **Fix**: Change `flags & 0x01` to `flags & 0x21` (HIDDEN | APP_CULLED) at all walker check sites.

### PD-03: ESM model path lacks marker prefix filtering
- **Severity**: MEDIUM
- **Location**: `byroredux/src/cell_loader.rs:288-295`
- **Description**: The fxlight filter catches effect meshes but not editor markers whose model paths start with `marker`, `xmarker`, or `defaultsetmarker`. These are STAT records with marker NIFs.
- **Fix**: Add `marker` prefix checks to the model path filter.

### PD-04: NiTexturingProperty decal map layers not imported
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/properties.rs:260-265`
- **Description**: Gamebryo's NiTexturingProperty supports decal map overlay slots (indices 8-10) — extra alpha-blended texture layers composited on the base. These are parsed but not extracted. Low frequency in FNV vanilla content but present in some architectural NIFs.

### PD-05: Decal depth bias values are hardcoded
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:535-544`
- **Description**: Constant -8.0, slope -2.0. May be too aggressive at shallow viewing angles. Gamebryo's D3D9 depth bias maps differently to Vulkan parameters.

### PD-06: is_editor_marker name check misses some patterns
- **Severity**: LOW
- **Location**: `crates/nif/src/import/walk.rs:515-525`
- **Description**: Checks `editormarker*`, `marker_*`, `markerx`, `marker:*` prefixes. Misses plain `"Marker"` (no suffix) and generic node names on marker meshes.

## Transform Pipeline: Confirmed Correct

The Z-up → Y-up conversion is consistent between ESM placement and NIF-internal transforms:
- ESM: `[x, z, -y]` translation swap + `euler_zup_to_quat_yup` rotation
- NIF: `[x, z, -y]` in walk.rs + `zup_matrix_to_yup_quat` for rotation matrices
- Composition: `final = ref_rot * (ref_scale * nif_pos) + ref_pos` — standard quaternion composition
- No double-conversion issues found

## FNV Decal Architecture

FNV decals are **pre-authored flat geometry** (not runtime projections):
- Flat quads positioned coplanar with walls/floors in the NIF file
- Flagged via BSShaderPPLightingProperty shader_flags_1 bits 26-27 or shader_flags_2 bit 21
- Depth bias prevents z-fighting with the surface they sit on
- The "floating" appearance is likely from decals whose NIF local transform has a slight offset from the surface, or from the depth bias slope factor being too aggressive at oblique angles
