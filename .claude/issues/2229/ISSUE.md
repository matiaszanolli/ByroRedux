# REN-D3-02: Fog-volume cluster-indexing constants are hand-written GLSL literals instead of build-script-emitted

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2229

**Dimension**: 3 (GPU structs)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs:115-116` (`FOG_VOLUME_CLUSTER_DIM = 16`, `MAX_FOG_VOLUMES_PER_CLUSTER = 8`); `crates/renderer/shaders/volumetrics_inject.comp:154-155` (`const uint FOG_CLUSTER_DIM = 16u;`, `MAX_FOG_VOLUMES_PER_CLUSTER = 8u;`)
**Status**: NEW

**Description**: The cluster-grid dimension and per-cluster capacity are hand-duplicated as literals on both the Rust and GLSL sides — the same defect class as two previously-fixed issues (#1190/#1401), where a constant changed on one side and not the other caused silent index-range mismatches.

**Evidence**: both files independently define the same two numeric constants with no shared source (`crates/renderer/build.rs` does not emit these into the shader include chain).

**Impact**: A future tuning change to either constant (e.g. raising cluster capacity) that isn't mirrored on the other side desyncs cluster indexing between CPU-built cluster lists and GPU shader reads — an out-of-bounds/silently-wrong-cluster read, matching the exact prior-fixed defect class.

**Suggested Fix**: emit these constants from `build.rs` into the shared shader include chain (matching the pattern already used to fix #1190/#1401), rather than hand-duplicating literals.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
