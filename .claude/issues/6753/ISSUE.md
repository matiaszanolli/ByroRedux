# REN-D8-05: per-frame `Vec` allocations on two new volumetric host paths (local box sources, sky-aperture candidates)

Labels: low,renderer,performance,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6753

**Severity**: LOW · **Dimension**: Volumetrics

## Location
`LocalBoxSet::update` (`sources: Vec<Source>` collected every frame with an emitter in reach; `crates/renderer/src/vulkan/volumetrics/`); `build_composite_params` (`candidates` vector) and `sky_apertures::nearest_apertures` (a fresh output `Vec`)

## Description
`LocalBoxSet::update` runs from `LocalGrid::begin_frame` every frame and allocates a `Vec<Source>` whenever any transported emitter is in reach. In an interior with marked sky apertures, `build_composite_params` grows a fresh `candidates` vector and `nearest_apertures` collects a new output vector every frame. Neither uses a caller-owned scratch; the repo keeps such paths as persistent scratch (#6485).

## Evidence
`let mut sources: Vec<Source> = volumes.iter()...collect()` in `LocalBoxSet::update`; `candidates.into_iter().map(...).collect()` in `nearest_apertures`.

## Impact
Small and bounded (a handful of elements) but allocator traffic on fire scenes and portal-sky interiors every frame. Not measured.

## Suggested Fix
Keep both as scratch fields cleared and reused per frame.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Other per-frame `collect()` sites in the volumetrics host path checked
- [ ] **TESTS**: Existing guards pass; no behaviour change
