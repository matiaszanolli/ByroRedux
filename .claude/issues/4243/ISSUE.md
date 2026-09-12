# FO3-2026-09-11-D2-02: BSShaderPPLightingProperty.refraction_fire_period decoded, zero consumers workspace-wide

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4243
**Labels**: bug, nif-parser, low, legacy-compat, game:fo3
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-2026-09-11-D2-02

**Severity**: LOW
**Dimension**: NIF v20.2.0.7 Parser (FO3 Block Subset) — `/audit-fo3` Dimension 2
**Location**: `crates/nif/src/blocks/shader.rs:55` (field `refraction_fire_period`), `crates/nif/src/import/material/legacy_properties.rs:559-561` (forwards `refraction_strength` but not `refraction_fire_period`)

**Description**: `BSShaderPPLightingProperty.refraction_fire_period` is decoded but has zero consumers workspace-wide.

**Evidence**: `refraction_fire_period` is defined and populated in `shader.rs` (field at `:55`, populated at `:119`); `legacy_properties.rs:559-561` forwards the sibling `refraction_strength` field into `info.refraction_strength` but has no equivalent line for `refraction_fire_period`.

**Impact**: FO3 heat-haze proxies (fire/flamer/plasma FX, 27 `BSRefractionStrengthController` blocks present) distort statically rather than scrolling. Cosmetic only.

**Suggested Fix**: Wire as a scroll-phase offset in the fire-refraction warp, or document the time-invariant model as deliberate.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If wired, forward at the NIF import → `Material`/effect boundary, not re-derived at render time.
- [ ] **TESTS**: N/A until a consumer is added.
