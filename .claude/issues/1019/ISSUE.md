# Issue #1019

**Title**: REN-D15-NEW-10: Sun arc lacks per-worldspace latitude tilt — equatorial arc on every Bethesda map

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-10
**Severity**: MEDIUM (deferred — visible on northern-latitude maps like Skyrim)
**File**: `byroredux/src/systems/weather.rs:294-330`

## Issue

Sun arc is computed as a pure half-circle in the X-Z plane (latitude=0 / equatorial). Real-world worldspaces like Skyrim (60°N analog) would have a shallower arc and shorter day; the engine paints a tropical sun-path everywhere.

## Fix

Per-worldspace latitude metadata (or a WRLD record field if one exists) → arc tilt vector applied to the angle-derived direction. Defer until M40 worldspace-metadata pass.

## Completeness Checks
- [ ] **SIBLING**: Co-design with worldspace metadata schema
- [ ] **TESTS**: Skyrim Tundra vs FNV Mojave sun-azimuth comparison

