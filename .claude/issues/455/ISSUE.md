# Issue #455

FO3-5-02: TileShaderProperty aliased to BSShaderPPLighting drops 20-28 B per block

---

## Severity: Medium

**Location**: `crates/nif/src/blocks/mod.rs:264` (dispatch), `crates/nif/src/blocks/shader.rs:56-86` (aliased parser)

## Problem

`TileShaderProperty` dispatch at `blocks/mod.rs:264` routes to `BSShaderPPLightingProperty::parse`. PPLighting parser stops after refraction/parallax (≈54 B), but `TileShaderProperty` has a FO3-specific trailer with `UnknownByte` + `TextureTransforms` + per-slot uses block (≈20-28 B).

## Evidence

`nif_stats` per-block warnings on probes:
- `stealthindicator.nif`: 16× blocks `expected 82 bytes, consumed 54. Adjusting position.`
- `airtimer.nif`: 17× blocks `expected 74 bytes, consumed 54`.

The outer loop self-heals via `block_sizes` so the parse rate stays at 100%, but the trailing fields land zero-initialized in the struct.

## Impact

Tile shaders drive HUD overlays (stealth meter, airtimer, quest markers). The uninitialized texture-transform fields cause overlays to render with zero-matrix transforms — usually invisible or mislocated.

## Fix

Add dedicated `TileShaderProperty::parse` extending PPLighting with the 20- or 28-B trailer (gated on `user_version_2 == 34` for FO3 vs 11 for Oblivion). Register in `blocks/mod.rs`.

Short-term log hygiene: bump dispatch severity at the `blocks/mod.rs` recovery site from `trace` to `warn` only when `consumed < size` by more than 16 B — surfaces real parser drift in CI.

## Completeness Checks

- [ ] **TESTS**: Probe `stealthindicator.nif` + `airtimer.nif` parse with 0 `warn!` lines post-fix
- [ ] **SIBLING**: Audit `blocks/mod.rs` dispatch table for other aliased stubs (`match` arms pointing at a sibling's parser)
- [ ] **DOCS**: nif.xml `TileShaderProperty` reference

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-5-02)
