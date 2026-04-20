# Issue #478

FNV-3-L3: climate.sun_texture parsed but never consumed by renderer

---

## Severity: Low

**Location**: `crates/plugin/src/esm/records/climate.rs:27,87`

## Problem

FNAM sun texture path stored on `ClimateRecord` but no renderer consumer exists. Feature ships 0% — the parsed value is dead.

Becomes usable once FNV-3-H1 (cloud texture path prefix) is fixed — same normalization needed.

## Impact

FNV sun disc renders as default billboard, not the per-climate authored texture. Minor aesthetic.

## Fix

Thread `climate.sun_texture` into `SkyParamsRes` as sun-disc sprite when `sun_size < 1.0`. Apply the same `textures\` prefix normalization as FNV-3-H1.

## Completeness Checks

- [ ] **TESTS**: Load FNV exterior, assert `SkyParamsRes.sun_texture_handle` is populated
- [ ] **SIBLING**: FNV-3-H1 prefix normalizer should cover this path too
- [ ] **SHADER**: Existing sun render path — verify it accepts a texture handle

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-L3)
