# #2403 — CHAIN2-D2-01: Skinned-vertex fragment read has no dedicated COMPUTE→FRAGMENT barrier — visibility rides the cluster-cull pass's trailing barrier

- **Severity**: MEDIUM
- **Domain**: vulkan
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2403


- **Severity**: MEDIUM
- **Dimension**: 2 — Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:398-405`; `crates/renderer/src/vulkan/context/draw.rs:2186-2220`; consumer at `crates/renderer/shaders/include/ray_hit.glsl:73-82`
- **Status**: NEW

**Description**

`#2219` added a fragment-stage consumer of the `skin_vertices.comp` output buffer (`GpuInstance.skinnedVertexAddress`, a raw `GL_EXT_buffer_reference` device address dereferenced by `getHitTriWorldPositions` in `triangle.frag`/`water.frag`). The renderer's own skin-chain barrier for that buffer only publishes to the acceleration-structure-build stage (`skinned_blas_refit.rs:398-405`, `COMPUTE_SHADER/SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_KHR/SHADER_READ`). The follow-on AS barriers are `AS_WRITE → AS_READ` and their access scopes do not overlap the compute `SHADER_WRITE`, so no barrier in the chain makes that write visible to `FRAGMENT_SHADER/SHADER_READ`. The only barrier that happens to cover it is the cluster-cull pass's trailing global `VkMemoryBarrier` (`draw.rs:2186-2220`), emitted only inside `if let Some(ref cc) = self.cluster_cull` — and `ClusterCullPipeline::new` failure is a graceful degrade to `None` (`context/mod.rs:1943-1972`) that does not gate the RT path, so `cluster_cull == None` + skinned RT actors is a reachable configuration where the visibility guarantee silently disappears.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`):

```rust
// skinned_blas_refit.rs:398-405 — the only publish of the skin output
memory_barrier(
    &self.device, cmd,
    vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
    vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
    vk::AccessFlags::SHADER_READ,   // AS-build INPUT only — no FRAGMENT dst
);
```
```glsl
// ray_hit.glsl:73-76 — the fragment-stage consumer
if (hi.boneOffset != 0u && hi.skinnedVertexAddress != 0ul) {
    SkinnedVertexRef ref = SkinnedVertexRef(hi.skinnedVertexAddress);
```
```rust
// draw.rs:2186-2220 — the incidental publish, inside a conditional
if let Some(ref cc) = self.cluster_cull {
    memory_barrier(&self.device, cmd,
        vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
        vk::PipelineStageFlags::FRAGMENT_SHADER, vk::AccessFlags::SHADER_READ);
}
```

**Impact**

With cluster-cull absent, every secondary-ray hit on a skinned actor (glass refraction, reflections, GI bounce) reconstructs face normals/tangents from a buffer whose compute write was never made visible to the fragment stage — symptom class is incoherent/garbage shading on skinned actors seen through or reflected in glass and water, varying by driver. Raster is unaffected (`triangle.vert` inline skinning is covered by the palette barrier's `VERTEX_SHADER` dst bit). In the default (cluster-cull-present) configuration this is currently correct by accident, not by a documented or tested dependency.

**Trigger Conditions**: `ClusterCullPipeline::new` returns `Err` while `device_caps.ray_query_supported` is true and a skinned actor with a live `SkinSlot` is visible to a secondary ray. Also latent against any future reorder of the cluster-cull dispatch relative to the geometry pass, or an early-out on "no lights this frame".

**Verification Path**: Validation layer. Run with `BYRO_VALIDATION=1` (Khronos + sync-validation) on a scene with a skinned actor beside glass, with cluster-cull forced to `None` (temporary `Err` injection at `context/mod.rs:1943`) — sync-validation should report `SYNC-HAZARD-READ-AFTER-WRITE` on the skin slot output buffer at `FRAGMENT_SHADER`. Not observable via `cargo test`; not observable in the default configuration at all.

**Related**: `#2219` (added the fragment consumer); prior `/audit-renderer` passes (2026-08-03, 2026-08-07) flag `#2219` generically as "needs a RenderDoc capture on an animated actor beside glass" — this finding names the specific code-verifiable reason.

**Suggested Fix**: Widen the existing `skinned_blas_refit.rs:398-405` barrier's dst to `ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` (dst access already `SHADER_READ`) and document that `#2219` made the skin output a fragment-stage consumer. Purely additive synchronization — no reordering — but per the anti-speculation policy it should land together with the `BYRO_VALIDATION=1` confirmation run above, not on reasoning alone.

## Completeness Checks
- [ ] **LOCK_ORDER**: N/A — this is a barrier-scope fix, not a `RwLock` change; confirm no reordering side-effect on the AS-build dependency
- [ ] **SIBLING**: Check other compute→AS→fragment chains (bloom, volumetrics, SSAO) for the same "AS-build-only barrier + incidental cluster-cull coverage" shape
- [ ] **TESTS**: Run the described `BYRO_VALIDATION=1` + cluster-cull-forced-`None` repro before AND after the fix per the anti-speculation policy; a `cargo test` regression test is not possible for this class

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
