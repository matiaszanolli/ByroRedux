# EXT-D3-2026-09-27-06: Model-tier host path re-derives records and the selection table and allocates about 5 fresh Vecs per frame

**Issue**: #4922
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,performance

**Severity**: LOW
**Dimension**: Ground-cover pipeline
**Tier Violated**: n/a
**Game Affected**: all exteriors with authored cover
**Status**: NEW (the class #4607/#4609/#4798 removed from the blade path)
**Location**:
- `byroredux/src/render/groundcover.rs:877,904`.
- `byroredux/src/app_frame.rs:788`.
- `crates/renderer/src/vulkan/groundcover_models.rs:502,507,692`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Suggested Fix
Gate on an `AuthoredCover` generation, and keep the Vecs as persistent scratch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
