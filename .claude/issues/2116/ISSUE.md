# REN-D14-01: caustic_splat.comp mis-indexes instance SSBO for opaque pixels after stable-surface-ID switch

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2116
**Labels**: bug, renderer, high

**Severity**: HIGH (escalates toward CRITICAL for entity IDs ≥ `MAX_INSTANCES` — a genuine OOB SSBO read with `robustBufferAccess` disabled)
**Dimension**: Caustics / SSBO-Indexing
**Location**: `crates/renderer/shaders/caustic_splat.comp` (the `meshId = meshIdRaw & 0x7FFFFFFFu; if (meshId == 0u) return; ... instIdx = meshId - 1u; instances[instIdx]` block). Upstream: `crates/renderer/shaders/triangle.frag` (mesh-ID encode). CPU source: `crates/renderer/src/vulkan/context/draw.rs` (`surface_id: draw_cmd.entity_id.wrapping_add(1)`).
**Status**: NEW — independently confirmed by two separate audit sub-agents reading the shader from different angles, then re-confirmed directly against the current tree.

**Description**: The shader masks off bit 31 and rejects only `meshId == 0`, then unconditionally derives `instIdx = meshId - 1` and reads `instances[instIdx]`. Before commit `883f57cd` ("stable surface ID for temporal shadowing and caustics"), opaque G-buffer pixels packed `instance_index + 1` in those bits — a valid live per-frame slot, safely rejected downstream by the `INSTANCE_FLAG_CAUSTIC_SOURCE` gate (caustic sources are always alpha-blend). After the commit, opaque pixels instead carry `stableSurfaceId = inst.surfaceId & 0x7FFFFFFF` (`surface_id = entity_id + 1`) — an ECS identity unrelated to the per-frame draw-order index `instances[]` is keyed by. No `(meshIdRaw & 0x80000000u) == 0u` opaque-reject guard was added to compensate; the shader's own in-source comment (near line 172) still states the old, now-false safety premise ("caustic sources are always alpha-blend … without the mask the derived instIdx would overflow the instance SSBO").

**Evidence**:
```glsl
// caustic_splat.comp
uint meshIdRaw = texelFetch(meshIdTex, pixel, 0).r;
uint meshId = meshIdRaw & 0x7FFFFFFFu;
if (meshId == 0u) return;
...
uint instIdx = meshId - 1u;
uint flags = instances[instIdx].flags;
```
```glsl
// triangle.frag
uint stableSurfaceId = inst.surfaceId & 0x7FFFFFFFu;
uint meshIdBase = alphaBlendFrag ? sortedInstanceId : stableSurfaceId;
outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);
```
```rust
// draw.rs
surface_id: draw_cmd.entity_id.wrapping_add(1),
```
`crates/core/src/ecs/world.rs::spawn()` uses a monotonic, never-recycled `next_entity` counter, so `entity_id` is unbounded across a session and unrelated to `MAX_INSTANCES`. `crates/renderer/src/vulkan/device.rs` does not enable `robust_buffer_access`.

**Impact**: (1) Always-on visual corruption in any scene with a caustic source (glass/water — common): opaque pixels read an arbitrary current-frame `instances[entity_id]` slot, splatting spurious caustics onto opaque surfaces whenever the aliased slot happens to have the caustic-source flag set. (2) Conditional OOB read: once cumulative session spawns exceed `MAX_INSTANCES` (262,144) — plausible in a long exterior-streaming session — `instances[entity_id]` reads past the fixed SSBO allocation with no `robustBufferAccess`, which is undefined behavior up to device-lost.

**Related**: Introduced by the same-day commit `883f57cd`. Shares its unbounded-`surface_id` root cause with the ReSTIR-DI reservoir finding (separate issue). `gbuffer_history_uses_stable_surface_id_but_caustics_keep_draw_lookup` is a CPU-side layout test and gives false coverage confidence here — it cannot exercise this shader's runtime path.

**Suggested Fix**: Add `if ((meshIdRaw & 0x80000000u) == 0u) return;` immediately before deriving `instIdx` in `caustic_splat.comp` — caustic sources are always alpha-blend, so rejecting opaque pixels before the SSBO read restores the pre-commit invariant and is strictly cheaper (skips the read for the opaque majority). Refresh the now-stale in-source comment.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other shader derives an instance index from mesh-ID the same way (SVGF/TAA were checked and only do masked equality comparisons — safe)
- [ ] **TESTS**: A regression test pins this fix (a CPU-side test cannot exercise the shader; consider a golden-frame/RenderDoc-capture regression instead)

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
