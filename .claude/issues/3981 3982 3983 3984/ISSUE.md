=================== ISSUE #3981 ===================
STATE: OPEN
LABELS: bug, renderer, medium, pipeline, tech-debt, test-gap
TITLE: REN-2026-09-06-D12-02: `taa_failed` / `svgf_failed` can never latch — `c43cb269`'s #3605 fix sits on an unreachable branch, and the one reachable TAA failure is warn-only

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D12-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::dispatch`), `crates/renderer/src/vulkan/svgf.rs` (`SvgfPipeline::dispatch`), `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_svgf_pass`), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (the `taa.upload_params` call site)
- **Status**: NEW
- **Description**: `TaaPipeline::dispatch` and `SvgfPipeline::dispatch` are
  declared `-> Result<()>` but their bodies contain **no error-producing
  construct at all** — no `?`, no `return Err`, no `bail!`, no `.context(`,
  no `map_err` — and both end in an unconditional `Ok(())`. Every statement
  is an infallible `ash` command recording call. Therefore
  `if let Err(e) = taa.dispatch(…)` in `record_taa_pass` and the matching arm
  in `record_svgf_pass` are dead code, and `self.taa_failed` /
  `self.svgf_failed` can never become `true` at runtime (they are only ever
  set to `false`, at construction and on resize).
  Three consequences:
  1. `c43cb269` ("Fix #3605: signal a temporal discontinuity on TAA dispatch
     failure", 2026-09-05) adds `signal_temporal_discontinuity(
     TAA_DISPATCH_FAILURE_RECOVERY_FRAMES)` inside that dead arm. Its
     regression guard,
     `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`,
     is a source-scan that asserts the *text* is present — so it passes while
     the behaviour is unreachable. The hazard #3605 describes is real; the
     fix as landed cannot fire.
  2. The `#1932` un-jitter gate
     (`assemble_camera_and_lights` gates Halton jitter on
     `taa.is_some() && !taa_failed`) and the `fall_back_to_raw_hdr` reroute
     are likewise unreachable.
  3. The TAA failure that **is** reachable is a different one:
     `taa.upload_params` (which does `param_buffers[frame].write_mapped(…)`
     — a mapped-slice/flush operation, the same fallible class #2504
     hardened for `upload_indirect_draws`) is handled with a bare
     `log::warn!("TAA upload_params failed: {e}")` and no latch. On that
     path the dispatch still runs, against whatever the params UBO held
     before (a previous frame's, or uninitialised on a slot's first use),
     while the geometry pass has already rendered jittered. That is exactly
     the "jittered but unresolved" state #3605 exists to protect against,
     and it is the case with no protection.
- **Evidence**: extracting each `dispatch` body by brace matching and
  grepping for `?;`, `return Err`, `bail!`, `Err(`, `.context(`, `map_err`
  yields zero hits for `taa.rs`, `svgf.rs` (and `bloom.rs`, whose arm is a
  harmless `warn!` with no latch); `ssao.rs`, `volumetrics.rs` and
  `caustic.rs` by contrast do contain `?`, so the fallible-dispatch shape is
  genuine elsewhere in the same file's call sequence — this is a
  three-pipeline anomaly, not a blanket convention.
  `crates/renderer/src/vulkan/context/mod.rs` documents `taa_failed` as
  "first `taa.dispatch` error in a session".
- **Impact**: A documented per-pass permanent-failure recovery tier (named as
  such in `record_post_passes`'s own doc: "the per-pass permanent-failure
  latches are preserved exactly") does not exist for SVGF or TAA. No runtime
  misbehaviour today — nothing fails, so nothing is mishandled — but two
  shipped fixes (#1932, #3605) and one reroute are unverifiable dead weight,
  and the real failure mode (a params-upload failure) silently degrades to a
  stale-parameter TAA resolve on a jittered frame. Also a maintenance trap:
  the source-scan guard gives false confidence that the path is exercised.
- **Related**: #3605 / REN-2026-08-30-D13-02 (`c43cb269`), #1932 / TAA-D13-01
  (the jitter gate), #917 / REN-D10-NEW-03 (`dispatched_this_frame`), #2504 /
  D12-2026-08-07-02 (the same fallible-upload class, correctly handled for
  indirect draws), #2146 / #917 (why `record_post_passes` is infallible).
- **Suggested Fix**: Route the reachable failure into the existing latch
  instead of inventing a new one: make the `taa.upload_params` /
  `svgf.upload_params` call sites set `taa_failed` / `svgf_failed` on `Err`
  (they already run in `build_and_upload_instances`, before
  `record_post_passes`, so the `!self.taa_failed` gate at the dispatch site
  picks it up in the same frame and the jitter gate picks it up the next).
  That makes #3605's `signal_temporal_discontinuity` live — but see
  **D12-03**, which must be fixed in the same change. Separately, either drop
  the `-> Result<()>` on the two dispatches or add a comment saying it is
  reserved; a `Result` no producer can populate is what made the dead arm
  look alive to three successive audits.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3982 ===================
STATE: OPEN
LABELS: bug, renderer, medium, shaders
TITLE: REN-2026-09-06-D16-01: #3829's fix closed one of four name-diverging GLSL↔Rust struct mirrors; `ClusterEntry`, `FogClusterEntry` and `CombustionLightMoment` remain outside every lockstep guard

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D16-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (defense-in-depth gap on a class with a demonstrated CRITICAL outcome 32 hours ago; not itself a live drift — all three are in sync at HEAD)
- **Dimension**: Volumetrics (GPU-struct lockstep)
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp` — `struct FogClusterEntry`, `struct CombustionLightMoment`, `struct ClusterEntry`
  - `crates/renderer/shaders/cluster_cull.comp` — `struct ClusterEntry`
  - `crates/renderer/shaders/include/bindings.glsl` — `struct ClusterEntry`
  - Rust counterparts: `crates/renderer/src/vulkan/volumetrics.rs` (`GpuFogClusterEntry`, `GpuCombustionLightMoment`), `crates/renderer/src/vulkan/compute.rs` (`ClusterEntry`)
  - The guards that do **not** cover them: `assert_mirror_list_is_complete` / `shader_sources_declaring` and the four lockstep tests that call them (`gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_light_glsl_copies_stay_in_lockstep`, `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`), all in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`
- **Status**: **NEW.** No matching open issue (`open_titles.txt` searched for `clusterentry`, `combustion`, `fogcluster`, `mirror`, `lockstep`, `stride` — zero hits). Not a finding in `AUDIT_RENDERER_2026-09-05.md`, which explicitly framed `GpuBoundaryInstance` as *"a sixth mirror sitting entirely outside the tracked set, **not a symptom of a wider pattern**"*. That framing was wrong: the pattern has three more members. `fa5c4191`'s own commit message records the sibling check — *"shader `FogClusterEntry` and `CombustionLightMoment` mirror Rust structs under different names too, and share #3829's discovery blind spot, but both are currently in sync (8 B and 32 B)"* — so the class was **seen and verified once, but not guarded**, and `ClusterEntry` (three GLSL copies) was not part of even that check.
- **Description**: The discovery half of the mirror guard, `shader_sources_declaring(decl)`, matches on a **literal declaration string** (`"struct GpuInstance"`, `"struct GpuLight"`, …). A GLSL struct that mirrors a Rust struct under a *different name* is therefore invisible to it, and the hand-written `SOURCES` tables only list what someone remembered. #3829 was exactly that failure — `GpuBoundaryInstance` went 13 days at a 128 B stride against a 160 B `GpuInstance`, with a green suite, because it wore its own name. The fix added `gpu_boundary_instance_stride_matches_gpu_instance` for that one struct and did not generalise.

  Three name-diverging mirrors remain, none of them covered by any test that reads the GLSL side:

  1. **`CombustionLightMoment` ↔ `GpuCombustionLightMoment`** — eight tightly packed `uint`s, 32 B. The Rust struct's own doc says *"Fixed-point ABI mirrored by `CombustionLightMoment` in `volumetrics_inject.comp`"*, i.e. a stated cross-language ABI. `combustion_light_moment_abi_is_eight_std430_words` asserts **only the Rust side** (`size_of == 32`, `align_of == 4`, `COMBUSTION_LIGHT_GRID_COUNT == 256`); it never `include_str!`s the shader. This one is **field-order sensitive, not just stride sensitive**: the shader writes by name (`atomicAdd(combustionLightMoments[binIndex].weighted_x, …)`, `.radiant_r`, `.luminous_volume`, …) while `decode_combustion_light_moment` decodes **positionally** by word index (`weight: word(0) … luminous_volume: word(7)`). A GLSL-side reorder compiles clean, keeps the size at 32 B, and silently swaps a luma-weighted centroid for a radiant channel.
  2. **`FogClusterEntry` ↔ `GpuFogClusterEntry`** — `{offset, count}`, 8 B. Only a Rust-side `assert_eq!(size_of::<GpuFogClusterEntry>(), 8)` exists. This buffer is now read under the **#3834 partial-upload** contract, where a wrong `count` decode is what makes a stale cluster live.
  3. **`ClusterEntry`** — `{offset, count}`, declared **three times in GLSL** (`cluster_cull.comp` writes it, `include/bindings.glsl` and `volumetrics_inject.comp` read it) against one Rust `#[repr(C)] struct ClusterEntry` in `compute.rs` that only ever appears as `size_of::<ClusterEntry>() * TOTAL_CLUSTERS`. Three GLSL copies with **zero** lockstep coverage — the same multi-copy shape `GpuInstance` and `GpuLight` each have a test for.
- **Evidence**:
  ```
  $ grep -rn "^struct " crates/renderer/shaders/*.{comp,frag,vert} crates/renderer/shaders/include/*.glsl
  → ClusterEntry ×3, FogClusterEntry ×1, CombustionLightMoment ×1, GpuBoundaryInstance ×1,
    GpuFogVolume ×1, GpuInstance ×5, GpuLight ×4, GpuMaterial ×1, GpuTerrainTile ×1, Reservoir ×1
  $ grep -rn "ClusterEntry" crates/renderer/src
  → definition (compute.rs:23) + two size_of uses. No test.
  ```
  `crates/renderer/src/vulkan/reflect.rs` cannot substitute: `uniform_block_size_by_name` reflects **uniform blocks** only. Every struct above lives in an SSBO element array, which the reflector does not size — that is precisely why #3829 needed a source-text test rather than a reflection one.
  Counter-check performed: `GpuFogVolume` **is** covered (`gpu_fog_volume_glsl_field_order_matches_rust_struct`, #2228), so this is a specific gap, not a blanket absence.
- **Impact**: No live corruption today — all three were confirmed field-for-field and size-for-size in sync at HEAD, and the workspace suite is green. The exposure is the next edit: any of these five structs can grow or reorder with a fully green `cargo test`, and the failure mode is the #3829 one — silently wrong data, no validation-layer diagnostic. Blast radius per struct: `CombustionLightMoment` → wrong fire/explosion surface lights (the field-order case is the nastiest, because size stays right); `FogClusterEntry` → wrong local-fog cluster walk under the new partial-upload contract; `ClusterEntry` → wrong clustered-light lists in **both** the fragment shader and the volumetrics inject pass simultaneously.
- **Related**: #3829 (the CRITICAL this class produced, and its one-struct fix), #3231 (the growth that triggered it), #2748 / #3564 (the `GpuInstance` mirror guard + its completeness half), #2228 (the `GpuFogVolume` precedent for a GLSL-reading field-order test), #3834 (the partial-upload contract now riding on `FogClusterEntry`), `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Two steps, the second more valuable than the first.
  1. Add three narrow guards on the `gpu_fog_volume_glsl_field_order_matches_rust_struct` / `gpu_boundary_instance_stride_matches_gpu_instance` pattern — `include_str!` the shader, `parse_glsl_struct_fields_typed`, compare against `parse_rust_struct_fields` (name+order) and `std430_struct_size` (stride). `CombustionLightMoment` needs the **field-order** leg, not just the stride leg, because `decode_combustion_light_moment` is positional. `ClusterEntry` additionally needs its three GLSL copies compared against each other.
  2. Close the discovery gap so a *fifth* name-diverging mirror cannot appear unnoticed: extend `assert_mirror_list_is_complete`'s companion walk to enumerate **every** `^struct ` declaration across `crates/renderer/shaders/` and assert each name appears in a registry of "tracked mirror" or "shader-local, no Rust counterpart" (today's shader-local set: `LocalMedium`, `CombustionDifferential`, `DisneyDiffuseSplit`). That converts "someone remembered" into "someone had to classify it", which is the only version of this guard that survives the next struct.

---

### LOW

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3983 ===================
STATE: OPEN
LABELS: bug, renderer, medium, shaders
TITLE: REN-2026-09-06-D17-01: `specularAaRoughness`'s screen-space derivatives run inside per-invocation-divergent control flow at all five call sites, and are recomputed once per cluster light for a fragment-invariant value

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (`specularAaRoughness`), `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`), `crates/renderer/shaders/triangle.frag` (five call sites)
- **Status**: NEW
- **Description**: `shadowableLightRadiance` opens with

  ```glsl
  float aaRoughness = ((dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u)
      ? roughness
      : specularAaRoughness(N, roughness);
  ```

  and `specularAaRoughness` evaluates `dFdx(N)` / `dFdy(N)`. Every one of the function's five call sites in `triangle.frag` is reached only under control flow that diverges *per invocation*, never per quad:

  1. the cluster light loop, past `if (contribution < 0.001) continue;` — a per-fragment N·L / attenuation gate, so lanes in a quad can be on different `i`, or have exited the loop entirely;
  2. the ReSTIR **temporal** reuse gate (`sameSurface && rpLightIndex < lightCount && … && rp.W > 0.0 && !isnan(rp.W)`) — reprojection validity is per pixel;
  3. the ReSTIR **spatial** reuse gate (`rnSurfaceId == surfaceId && spatialDepthCompatible && … && dot(geomN, nGeomN) >= SPATIAL_NORMAL_COS`);
  4. the selected-light block (`if (restirY != 0xFFFFFFFFu && restirW > 0.0 && visibilityMaskNeedsTrace(...))`);
  5. the legacy-WRS shadow subtraction, `if (any(lessThan(transmission, vec3(0.999))))` — gated on a **ray-query result**, the most divergent predicate in the shader.

  GLSL/SPIR-V leave derivative results undefined in non-uniform control flow. This is the same defect class #3622 fixed in `parallaxDisplaceUV`, whose comment states the rule explicitly ("Implicit derivatives are undefined per the GLSL/Vulkan spec when the sample sits inside non-uniform control flow"); the sibling in the BRDF assembly was not swept. It is also aggravated by the alpha-test `discard` earlier in `main()`: a quad that lost a lane to `OpKill` has undefined derivatives for the survivors from that point on, and hoisting above the discard removes that exposure too.

  Independently of the spec question, the call is **redundant**. `N` is final by the terrain-splat/weather block and `roughness` by the weather-puddle/snow block — both hundreds of lines before the first call site, and neither is written inside the light loop. A 16-light cluster therefore executes the `dFdx`+`dFdy`+clamp+two-`sqrt` chain sixteen times per fragment to produce sixteen identical values, and forces the quad into lockstep at each one.
- **Evidence**:
  - `pbr.glsl`: `float specularAaRoughness(vec3 N, float roughness) { vec3 dNdx = dFdx(N); vec3 dNdy = dFdy(N); … }`
  - `lighting.glsl`: `shadowableLightRadiance` calls it unconditionally (modulo the uniform `DBG_DISABLE_SPECULAR_AA` UBO bit).
  - Invariance: `roughness` is assigned only at the gloss-map mix and the two `weatherPuddles`/`weatherSnow` mixes; `N` only in the normal-map / model-space-normal / terrain-splat / snow / glass blocks — all strictly before the lighting section.
  - Precedent: `material_sampling.glsl`'s `parallaxLod` comment (#3622) and `ray_hit.glsl`'s always-explicit `textureLod`.
- **Impact**: Undefined specular-AA roughness on lit fragments — the visible signature would be inconsistent specular-lobe width at light-cluster and ReSTIR-reuse boundaries, and near alpha-tested geometry. Because `shadowableLightRadiance` is *also* the ReSTIR `pHat` scorer and the legacy shadow-subtrahend, a divergent value there desynchronises the "unshadowed accumulation cancels bit-for-bit against the shadowed subtraction" invariant #1369 depends on. All games, every lit fragment. The wasted work is unconditional.
- **Related**: #3622 (the fixed sibling), #1369 (the refactor that moved the BRDF — and this derivative — into the per-light function), #2471 / #2806 (`specularAaRoughness`'s clamp history).
- **Suggested Fix**: Hoist the filter to uniform control flow: compute `float aaRoughness = (dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u ? roughness : specularAaRoughness(N, roughness);` once in `main()` immediately after `N` and `roughness` are final (and ideally before the alpha-test `discard`), and pass `aaRoughness` into `shadowableLightRadiance` in place of `roughness` — keeping the raw `roughness` as a separate parameter for `disneyDiffuseSplit`, which deliberately uses the unfiltered value. Pin with a `shader_contract_tests.rs` assertion that `specularAaRoughness` does not appear inside `lighting.glsl`. **Needs RenderDoc / an A-B capture to demonstrate a visual delta**: because the inputs are branch-invariant, a compiler that hoists the derivative already produces the correct value, so the correctness half is a conformance-and-hardening claim, not an observed artefact. The redundant-work half is a source-level fact and needs no capture.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

=================== ISSUE #3984 ===================
STATE: OPEN
LABELS: bug, renderer, medium, shaders
TITLE: REN-2026-09-06-D17-02: the anisotropic GGX branch is the one TBN builder in the shader tree without the #2815 post-Gram-Schmidt zero guard — `normalize()` on a zero vector poisons `Lo`, the ReSTIR reservoir and the EMA history with NaN

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D17-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`, the `mat.anisotropic > 0.0` branch)
- **Status**: NEW
- **Description**: The anisotropic branch rebuilds a tangent frame from the interpolated vertex tangent:

  ```glsl
  vec3 T = normalize(fragTangent.xyz);
  T = normalize(T - dot(T, N) * N);
  ```

  The guard above it (`dot(fragTangent.xyz, fragTangent.xyz) > 1e-4`) proves only that the **raw** tangent is non-zero, not that it is non-parallel to the shading normal `N`. When `T ∥ N` the Gram-Schmidt projection is the zero vector and `normalize()` on it is `0/0` → NaN. This is precisely the hazard #2815 / REN-D19-04 fixed in `perturbNormal`, and `material_sampling.glsl`'s comment there enumerates the sibling builders that already carry the guard — `parallaxDisplaceUV`'s `if (dot(T, T) < 1e-8 || heightScale <= 0.0) return uv;` and `getRayHitTangentFrame`'s `if (dot(worldT, worldT) < 1e-8) return false;`. This fourth builder is not in that list and does not have the guard.

  `N` here is the *normal-mapped* shading normal (or `glassViewNormal`), not the geometric normal, so a strongly-perturbing normal map is enough to rotate `N` into the authored tangent's direction; `perturbNormal`'s own guard exists on exactly that reasoning.
- **Evidence**: The three guarded siblings vs the unguarded fourth, all in the same `#include` chain:
  - `material_sampling.glsl` / `perturbNormal`: `if (dot(Tproj, Tproj) < 1e-8) { return N; }`
  - `material_sampling.glsl` / `parallaxDisplaceUV`: `if (dot(T, T) < 1e-8 || heightScale <= 0.0) { return uv; }`
  - `ray_hit.glsl` / `getRayHitTangentFrame`: `worldT -= dot(worldT, N) * N; if (dot(worldT, worldT) < 1e-8) { return false; }`
  - `lighting.glsl` / `shadowableLightRadiance`: `T = normalize(T - dot(T, N) * N);` — no post-projection test.
- **Trigger Conditions**: Requires `mat.anisotropic > 0.0`. `translate_material` pins `anisotropic: 0.0` for every source format (guarded by `anisotropic_rationale_matches_what_the_source_formats_carry`), so **no shipped game content reaches this branch** — but it is deliberately reachable through the two producers built for it: `cornell::pbr_bsdf_lobes` (the `--cornell` Disney-lobe probe, `anisotropic = 0.1`) and the live `mat.set <id> anisotropic <v>` console arm (`commands/scene.rs`), both added by #2514 / REN-D21-2026-08-07-02 precisely so the anisotropic lobe could be exercised.
- **Impact**: A NaN emitted here does not stay local. `shadowableLightRadiance` is the shared BRDF for the pass-1 accumulation, the ReSTIR `pHat` score, and the legacy shadow subtraction, so a single bad fragment produces `restirWSum`/`restirPHat` NaN (→ `restirW` NaN, which the `!isnan(rp.W)` reuse gates then propagate to *neighbouring* pixels through spatial reuse) and a NaN `accum` in the EMA history, which the `mix(prevAccum, …)` recurrence can never clear. The result is a persistent, spreading black/white blot rather than a one-frame speck — and it lands first on the Cornell harness built to validate this exact lobe, which is where the false all-clear costs most.
- **Related**: #2815 / REN-D19-04 (the identical fix in `perturbNormal`), #2512 (the sibling `fragTangent.w` ±1 clamp this branch *does* have), #2514 (the constructors that make the branch reachable), #1250 (the anisotropic lobe itself).
- **Suggested Fix**: Mirror `perturbNormal` exactly — project first, test the projected length, and fall back to the isotropic `distributionGGX(NdotH, aaRoughness)` when it collapses:
  ```glsl
  vec3 Tproj = T - dot(T, N) * N;
  bool anisoUsable = dot(Tproj, Tproj) >= 1e-8;
  ```
  gating the existing `distributionGGXAniso` call on `anisoUsable`. Add it to the sibling list in `material_sampling.glsl`'s comment so the four builders stay enumerated, and pin with a `shader_contract_tests.rs` assertion that the branch contains a post-projection length test.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

