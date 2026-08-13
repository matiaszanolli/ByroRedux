# #2744 — REN-D10-01: cluster_cull.comp differences two ABSOLUTE positions for ray direction — near-plane corners collapse past |world| ~131k

**Severity**: HIGH · **Domain**: renderer (shader) — `crates/renderer/shaders/cluster_cull.comp`
**Location**: `ndcToWorld`, and the `nearCorners`/`rayDir`/`corners` block in `main`

`ndcToWorld` reconstructs from origin-relative `invViewProj` then lifts to absolute (`+ renderOrigin.xyz`) — correct for the AABB (needs absolute space to match the light SSBO). But the very next use is `normalize(nearCorners[i] - camPos)`, a small difference (near plane at z=0.1) formed from two large-magnitude (~10⁵) absolute f32s — the lift throws away precision *before* the difference that needed it preserved. At Markarth-scale origins (|world|≈176000) tile boundaries collapse onto the same f32 (zero-width frustum voxels); residuals reach ~4.5° against a ~5.3° tile angular size. `sphereIntersectsAABB` under-reports lights → tiles silently lose point/spot lights. Directional unaffected; interiors unaffected (small |world|). Degradation: ~10% at 16k, ~42% at 65k, total collapse ≥131k — the recent far-plane LOD bump (400000) pushes exterior content routinely into the affected regime.

**Suggested fix**: take the difference in relative space, lift once — `camRel = cameraPos.xyz - renderOrigin.xyz` (exact, origin is a `RENDER_ORIGIN_SNAP` floor-multiple), `rayDir = normalize(nearCornerRel - camRel)` before building `corners[i] = camPos + rayDir * z` in absolute space (light test still needs absolute). Pure shader arithmetic reordering — no pass/pipeline/barrier/descriptor change. Pin with a static source-check test next to `caustic_writers_rebase_render_origin_before_reprojection`.

---

# #2745 — REN-D11-2026-08-12-01: refractive glass's mesh-ID write is masked off, making it invisible to its own caustic-source gate

**Severity**: HIGH · **Domain**: renderer — `crates/renderer/src/vulkan/pipeline.rs` (`create_blend_pipeline`, `preserve_opaque_gbuffer`), `crates/renderer/src/vulkan/context/draw.rs` (`is_refractive_glass`/`is_caustic_source`, `PipelineKey::Blended`), `crates/renderer/shaders/caustic_splat.comp` (`meshIdTex` gate)

`c615f8de` added `preserve_opaque_gbuffer` to `PipelineKey::Blended`, replacing attachments 1–5 (normal/motion/**mesh_id**/raw_indirect/albedo) with `no_write` when set from `is_refractive_glass(draw_cmd)`. `is_caustic_source` is literally `is_refractive_glass` (same predicate). `caustic_splat.comp` finds sources exclusively via the mesh-ID attachment's bit 31. Both glass arms are now unreachable: alpha-blended glass has `outMeshID` discarded by the write mask (bit 31 never set); non-blended glass (MultiLayerParallax) takes the opaque pipeline (bit 31 never set either way). Producing and consuming sets are disjoint — the glass-side caustic accumulator receives zero splats every frame in every cell, while the compute pass still dispatches at full screen-sized cost. Water-side caustics unaffected. Compounds with a second, independent CPU-gate-asymmetry issue (filed separately) — fixing either alone still leaves the pass dark.

**Suggested fix**: decide which contract survives, single-source it — either (a) keep mesh_id writable for the glass pipeline (only attachments 1/2/4/5 → no_write, 3 stays overwrite) and solve "caustics through walls" in `caustic_splat.comp`'s depth/geometry gate, or (b) retire the alpha-draw mesh-ID representation and give `caustic_splat.comp` an explicit source list — then update `triangle.frag`, `gbuffer.rs::MESH_ID_FORMAT`, `docs/engine/shader-pipeline.md` together. A unit test asserting `is_caustic_source(cmd) ⇒ mesh-ID writable for that cmd's pipeline key` would catch this at `cargo test` time.

---

# #2746 — REN-D1-01: docs/engine/renderer.md's AS section contradicts the code on three points

**Severity**: MEDIUM · **Domain**: docs (renderer, no crate)
**Location**: `docs/engine/renderer.md:382-390`

Three statements describe behavior the code deliberately does NOT have: (1) "TLAS per frame... with frustum culling against the camera" — the only TLAS-eligibility gate (`draw_command_eligible_for_tlas`) has no frustum term, by design (#516: off-screen occluders must stay in the TLAS for shadow/reflection/GI rays); (2) "Per-skinned-entity BLAS: keyed by EntityId, built sync at cell load" — the sync per-NPC builder was deleted under #1141, `build_skinned_blas_batched_on_cmd` is the sole entry point, records on the per-frame command buffer at first sight; (3) "a two-stage barrier chain (HOST_WRITE→TRANSFER_READ→AS_READ)" — the second barrier's `dst_access_mask` is `SHADER_READ` (per #1436, avoiding a sync-validation copy→build RAW hazard). `docs/engine/memory-budget.md`'s AS section was checked and is accurate — drift confined to `renderer.md`. Item 1 is the dangerous one (reads as a design statement; acting on it would delete off-screen RT contributions).

**Suggested fix**: three edits — drop "with frustum culling against the camera", state the #516 rule (frustum gates `in_raster` only); "built sync at cell load" → "built on the per-frame command buffer at first sight"; barrier chain → `HOST_WRITE→TRANSFER_READ→SHADER_READ @ AS_BUILD`, cite #1436.

---

# #2747 — REN-D10-02: getHitTriWorldPositions returns relative positions on the rigid branch, absolute on the skinned branch

**Severity**: MEDIUM · **Domain**: renderer (shader) — `crates/renderer/shaders/include/ray_hit.glsl` (`getHitTriWorldPositions`, consumed by `getHitTriNormal`/`getRayHitTangentFrame`)

Skinned branch (`hi.boneOffset != 0 && hi.skinnedVertexAddress != 0ul`) reads `skin_vertices.comp` output — bakes the ABSOLUTE bone palette (same convention `tlas_instance_transform` relies on for `IDENTITY_VK_TRANSFORM`). Rigid branch multiplies bind-pose vertices by `hi.model`, and `GpuInstance.model` has been render-origin-RELATIVE since the markarth cascade (`rebase_model_matrix` subtracts `render_origin` unconditionally). Header comment claims "World-space positions" as a whole-function guarantee it doesn't hold. **No wrong pixels today** — every consumer (`getHitTriNormal`, `getRayHitTangentFrame`) uses only differences, and a uniform origin offset cancels. Latent exposure: any future absolute consumer (re-projection, distance-to-camera, world-space hash, second-bounce origin) gets a `render_origin`-sized displacement on rigid geometry only. Same failure class as #1488 (caustic writers). MEDIUM not HIGH because latent, not live.

**Suggested fix**: make the contract explicit rather than changing behavior — either rebase the skinned branch to relative and rename to `getHitTriPositionsRel`, or lift the rigid branch to absolute and keep the current name. State which frame is returned in the header either way.
