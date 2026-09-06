# #3978 — REN-2026-09-06-D12-01: #1260's off-frustum `flags = 0` skip rests on a premise `include/ray_hit.glsl` invalidated two months later

**Labels**: high, pipeline, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D12-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: HIGH
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (the `let flags = if skip_batch { 0u32 } else { … }` block and its #1260 rationale comment), `crates/renderer/shaders/include/ray_hit.glsl` (`getHitInterpolatedNormal`, `rayHitHasCoverage`), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`)
- **Status**: NEW
- **Description**: #1260 (`26c60335`, 2026-05-24) skips per-instance flag
  assembly for draws that will not be rasterized —
  `skip_batch = !draw_cmd.in_raster || draw_cmd.is_water` — and ships
  `GpuInstance.flags = 0` for them. Its written justification is explicit:
  > "CAUSTIC_SOURCE is gated by the meshId G-buffer …, which only contains
  > pixels for in-frustum rasterized geometry. The RT hit paths read
  > `hitInst.vertexOffset / indexOffset / materialId / avgAlbedo* /
  > textureIndex` … **but NEVER `hitInst.flags`**."

  That is no longer true. `include/ray_hit.glsl` — `#include`d by
  `raytrace.glsl`, `shadow_transport.glsl`, `triangle.frag` and `water.frag`
  — now reads the **hit** instance's flags at four sites, all added after
  #1260 landed: `INSTANCE_FLAG_FLAT_SHADING` and
  `INSTANCE_FLAG_NON_UNIFORM_SCALE` in `getHitInterpolatedNormal`
  (`9ade7506`, 2026-07-30), and `INSTANCE_FLAG_DIFFUSE_ALPHA` and
  `INSTANCE_FLAG_ALPHA_BLEND` in `rayHitHasCoverage` (`5d8bb982`,
  2026-07-26).
  Off-frustum instances are exactly the population that stays in the TLAS —
  `build_tlas_instances`'s own comment: "frustum culling only gates
  rasterization (`in_raster`) and off-screen occluders stay in" — so they are
  precisely the instances rays land on while carrying `flags == 0`.
- **Evidence**:
  - `ray_hit.glsl`, `getHitInterpolatedNormal`:
    `if ((hitInst.flags & INSTANCE_FLAG_FLAT_SHADING) != 0u) { … } else if ((hitInst.flags & INSTANCE_FLAG_NON_UNIFORM_SCALE) != 0u) { … transpose(inverse(model3)) * localN … } else { worldN = model3 * localN; }`
  - `ray_hit.glsl`, `rayHitHasCoverage`:
    `if ((inst.flags & INSTANCE_FLAG_DIFFUSE_ALPHA) == 0u && mat.alphaThreshold == 0.0) { alpha = 1.0; }` and
    `if ((inst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u && mat.materialKind != MATERIAL_KIND_GLASS) { return alpha >= (1.0 / 255.0); }`
  - Callers of `rayHitHasCoverage`: `raytrace.glsl` (reflection/GI traversal),
    `shadow_transport.glsl` (×2, shadow traversal), `triangle.frag`,
    `water.frag` — all pass a *hit* instance index, not the shading fragment's own.
  - Commit dates: `26c60335` 2026-05-24 (the skip) vs `5d8bb982` 2026-07-26
    and `9ade7506` 2026-07-30 (the flag reads).
- **Impact**: For every frustum-culled instance a ray hits:
  1. **`ALPHA_BLEND` clear** → the "pure blend geometry uses alpha as binary
     coverage for ray traversal" branch never runs, so an off-screen
     alpha-blended surface is a **fully opaque blocker** for shadow,
     reflection and GI rays. A glass pane, foliage card, curtain or FX card
     just outside the frustum casts a solid shadow that vanishes the moment
     it enters the frustum — a camera-dependent lighting change on geometry
     the camera cannot see, which is the exact class of artifact off-screen
     TLAS retention (#516) exists to avoid.
  2. **`DIFFUSE_ALPHA` clear** → `alpha` is forced to `1.0` for these hits
     whenever `mat.alphaThreshold == 0.0`, reinforcing (1) even for materials
     that *do* carry an authored alpha channel (BC3/BC7).
  3. **`NON_UNIFORM_SCALE` clear** → `getHitInterpolatedNormal` takes the
     plain `model3 * localN` branch for non-uniformly-scaled off-frustum
     instances, so the hit normal is skewed. Bethesda cells scale placed
     REFRs non-uniformly routinely; the wrong normal biases the shading and
     the ray-offset of every bounce off that surface.
  4. **`FLAT_SHADING` clear** → flat-shaded off-frustum geometry gets
     smooth-interpolated hit normals.
  The severity is set by the fact that (1) silently changes direct shadowing,
  which is the most visible RT output, and by the blast radius: every cell,
  every game, every frame, for whatever fraction of the loaded set is
  currently out of frustum (typically most of it).
- **Related**: #1260 / PERF-D3-NEW-04-05 (the optimization), #516 (why
  off-frustum entries exist in the SSBO/TLAS at all), #922 (the
  `CAUSTIC_SOURCE` half of the rationale, which *is* still sound — the
  caustic gate really is mesh-ID-driven), #ae285062 (the `DIFFUSE_ALPHA`
  BC1 contract this now half-applies).
- **Suggested Fix**: Narrow the skip to the flags that are provably
  rasterizer-only rather than zeroing the whole word: assemble
  `NON_UNIFORM_SCALE`, `FLAT_SHADING`, `ALPHA_BLEND` and `DIFFUSE_ALPHA`
  unconditionally (the first is three dot products, the last two are one
  branch plus a cached `handle_has_alpha` lookup already paid for every
  in-frustum blended draw), and keep skipping only `TERRAIN_SPLAT` + tile
  index and the `RENDER_LAYER` bits, which no `ray_hit.glsl` / `raytrace.glsl`
  / `shadow_transport.glsl` path reads. Then replace the prose invariant with
  a `shader_constants`-style source-scan test that greps the RT include set
  for `\.flags &` and asserts every constant it finds is in the
  unconditionally-assembled set — the premise rotted silently precisely
  because it was only a comment.

---

---

# MEDIUM

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
