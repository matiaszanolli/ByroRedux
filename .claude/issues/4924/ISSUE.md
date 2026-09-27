# EXT-D3-2026-09-27-08: An alpha-blend-only GRAS shape would render as an opaque quad

**Issue**: #4924
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,renderer

**Severity**: LOW (latent; not measured on data)
**Dimension**: Ground-cover pipeline
**Tier Violated**: no-render-time-fallback
**Game Affected**: any GRAS model with blend on and alpha test off
**Status**: NEW
**Location**:
`crates/renderer/src/vulkan/groundcover_models.rs:886-904,566-572`; `crates/renderer/shaders/triangle.frag:354`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The tier drops `ALPHA_BLEND` and never sets `DIFFUSE_ALPHA`, so `triangle.frag` pins alpha to 1.0.
- #4413 verified Skyrim 27/27 and FNV 24/24 models; Oblivion, FO3 and FO4 were not checked.

## Suggested Fix
- Survey GRAS model alpha across the five games.
- Warn on blend-only shapes at template spawn, and convert them to alpha test at a documented threshold or skip them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
