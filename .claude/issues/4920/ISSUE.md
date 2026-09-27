# EXT-D3-2026-09-27-04: Model-tier overflow drops whole late records and splits multi-shape plants silently; the 128-record cap also truncates silently

**Issue**: #4920
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug

**Severity**: LOW
**Dimension**: Ground-cover pipeline
**Tier Violated**: n/a
**Game Affected**: all (only past the caps)
**Status**: NEW
**Location**:
`groundcover_models.comp:361-377`; `byroredux/src/render/groundcover.rs:847-851`; `crates/renderer/src/vulkan/groundcover_models.rs:495-498`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- Past 32,768 instances, the layout phase grants shapes in FormID order, so the last DLC and mod records lose every instance.
- A multi-shape plant can keep one shape and lose another.
- No warning or telemetry outside `--bench-*`. The blade tier's equivalent was fixed under #4338.

## Suggested Fix
Grant instances per plant or proportionally. Log once when a cap is hit, and add both counts to `DebugStats`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
