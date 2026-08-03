# SF2D2-06: #1232 tangent-synthesis guard is vacuously true — synthesis can run against a fabricated up-normal

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2363
**Labels**: bug,nif-parser,low,legacy-compat

---

**Severity**: LOW
**Dimension**: 2 — BSGeometry Mesh Extraction (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:147-158`, `:192`
**Status**: NEW, CONFIRMED against current code

## Description

The synthesis-branch guard `!normals.is_empty() && !uvs.is_empty() && !positions.is_empty()` is meant to require "otherwise-populated" geometry, but `normals` is already filled with a `[0,1,0]` placeholder upstream whenever authored normals are absent (`vec![[0.0, 1.0, 0.0]; positions.len()]` when `mesh_data.normals_raw.is_empty()`) — so the guard reduces to `!uvs.is_empty()`, and Gram-Schmidt tangent synthesis can silently run against a constant fabricated normal.

## Evidence

Confirmed by reading `bs_geometry.rs:147-158` and `:192` directly: `normals` is unconditionally non-empty whenever `positions` is non-empty (the `else` branch fills the `[0,1,0]` placeholder at exactly `positions.len()` size), making the later `!normals.is_empty()` check in the tangent-synthesis guard always true whenever positions/uvs are populated.

## Impact

Empirically unreachable on vanilla (0 of 4,000 sampled `.mesh` files lacked normals/tangents/UV0). Correctness-of-intent hardening only; latent trap for modded/tool-exported content that omits authored normals.

## Suggested Fix

Track `normals_authored` explicitly (set only when `mesh_data.normals_raw` was non-empty) and gate tangent synthesis on that instead of `!normals.is_empty()`.

## Completeness Checks
- [ ] **SIBLING**: Check the Oblivion/Skyrim `sse_recon.rs`/`tangent.rs` synthesis guards for the same placeholder-vs-authored conflation
- [ ] **TESTS**: A regression test pins a mesh with fabricated (placeholder) normals and populated UVs producing NO tangent synthesis (or a documented safe fallback) once `normals_authored` gating lands
