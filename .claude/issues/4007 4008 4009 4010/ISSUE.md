# #4007: REN-2026-09-06-D13-01: `signal_temporal_discontinuity` has phase-dependent semantics — two of its five limbs are inert from the two `record_post_passes` call sites, including `#3605`'s new one, and nothing documents or guards the phase requirement

Labels: bug, renderer, low

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D13-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`signal_temporal_discontinuity`); call sites `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_upscale_pass`); the end-of-frame swap in `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)` immediately after `mark_dispatch_completed`)
- **Status**: NEW — a gap in the fix `c43cb269` shipped, not a regression of it
- **Description**: `signal_temporal_discontinuity` has five limbs:
  `svgf_recovery_frames.max(frames)`, `taa.signal_history_reset()`,
  `fsr_temporal.signal_reset()`, `volumetrics.signal_history_reset()`, and
  `previous_rigid_models.clear()`. Its documented contract includes *"The first
  frame after a discontinuity must not encode object motion against transforms
  from the retired scene/camera history."* That contract holds only for callers
  that run **before** `build_and_upload_instances` — i.e. outside `draw_frame`
  (streaming / save / debug-load / app-step / resize) or at the `camera_cut`
  site inside `assemble_camera_and_lights`. Both `record_post_passes` callers
  run *after* it, and:

  1. **`previous_rigid_models.clear()` is unconditionally undone.** `draw_frame`
     ends with `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)`,
     refilling the map with this frame's models. The clear performed at
     `record_taa_pass` / `record_upscale_pass` time is discarded before the next
     frame's `uses_rigid_motion_history` lookup ever reads it.
  2. **`fsr_temporal.signal_reset()` is phase-fragile.** `reset_pending` is read
     into `fsr_frame` back in `assemble_camera_and_lights`; a reset raised
     afterwards would be cleared by this same frame's `mark_dispatch_completed()`
     (reached via `take_submitted_dispatch()` at the tail of `draw_frame`)
     before the next frame reads it. Today this is saved only by an accident of
     which states can coexist — `#2519` only signals when the dispatch *failed*
     (so `dispatched_this_frame` is false and the reset survives), and `#3605`
     only fires in `UpscalerMode::Taa`, where `fsr_temporal` is `None`.

  Evaluating `#3605`'s call limb by limb: `taa.signal_history_reset()` is inert
  by construction (`taa_failed` has just latched, so `upload_params` and the
  dispatch are both gated off for the rest of the session);
  `fsr_temporal` is `None`; `previous_rigid_models.clear()` is undone per (1);
  `volumetrics.signal_history_reset()` **works** (it runs after
  `record_volumetrics_pass`, so clearing `dispatched_this_frame` correctly stops
  `mark_frame_completed` from validating the history); and the SVGF limb works
  only when the camera is moving (`REN-2026-09-06-D8-01`). Net delivered effect
  of `c43cb269` is one volumetrics history reset plus a conditional SVGF α bump.
- **Evidence**:
  - `signal_temporal_discontinuity`'s own comment: *"The first frame after a
    discontinuity must not encode object motion against transforms from the
    retired scene/camera history."*
  - Call order inside `record_post_passes`: `record_svgf_pass`,
    `record_caustic_splat_pass`, `record_volumetrics_pass`, `record_taa_pass`,
    `record_ssao_pass`, `record_composite_pass`, `record_bloom_pass`,
    `record_upscale_pass`, `record_presentation_pass`.
  - `previous_rigid_models` is read only at `build_and_upload_instances`
    (`self.previous_rigid_models.get(&draw_cmd.entity_id)`) and written only by
    the end-of-frame swap plus the `clear()` in question.
  - `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
    (`post_passes.rs`) asserts the *call* is present; nothing asserts which
    limbs of it survive to the next frame.
- **Impact**: No live visual defect today — both in-frame callers signal on a
  frame where the scene geometry did **not** change, which is exactly the case
  where correct (non-zeroed) motion vectors are wanted anyway. The defect is
  that a documented, load-bearing invariant is silently unenforceable from
  inside `record_post_passes`, and `#3605` has just established that call site
  as a normal place to signal from. The next in-frame caller that signals a
  *real* scene discontinuity gets a partial reset with no diagnostic.
- **Related**: `#3605` / `c43cb269`; `#2519` (the FSR sibling);
  `REN-2026-09-06-D8-01`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Either (a) make the in-frame limbs order-independent —
  have `previous_rigid_models.clear()` set a `suppress_rigid_history_next_frame`
  flag the next `build_and_upload_instances` consumes and clears, mirroring the
  existing `!camera_cut` guard in that same loop; or (b) document the phase
  requirement on `signal_temporal_discontinuity` and add a source-scan test in
  the style of `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
  pinning that no limb depends on being called pre-upload. (a) is preferable —
  a doc-only fix leaves the trap armed.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4008: REN-2026-09-06-D13-02: `#3607` closed the discoverability half of the five-copy `octDecode` duplication but not the drift half — every guard is a name/count pin, and the shared copy in `include/math_common.glsl` is declared non-standalone

Labels: bug, renderer, low, tech-debt, shaders, test-gap

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D13-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp`, `crates/renderer/shaders/svgf_temporal.comp`, `crates/renderer/shaders/svgf_atrous.comp`, `crates/renderer/shaders/caustic_splat.comp`, `crates/renderer/shaders/include/math_common.glsl`; guard `taa_comp_octahedral_decoder_is_named_octdecode` (`crates/renderer/src/vulkan/taa.rs`)
- **Status**: NEW (residual of closed `#3607`; the rename itself verified complete — see Coverage)
- **Description**: The rename landed correctly and the maintenance comments in
  all four `.comp` copies now enumerate each other. But `taa.comp`'s own comment
  states the residual hazard verbatim: *"a divergence here is a silent, per-pixel
  difference in a history-rejection predicate, invisible to every existing test
  since all of them are source-scan pins."* That is still true. The two guards
  are `taa_comp_octahedral_decoder_is_named_octdecode` (asserts the string
  `vec3 octDecode(vec2 e)` is present, that `oct_decode` is absent, and that
  `octDecode(` occurs exactly 3×) and
  `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` (asserts the
  reject-list expression). Neither compares the *bodies*. A one-line edit to
  three of the four copies would still pass everything.
  A fifth copy exists that the enumerations do not name:
  `include/math_common.glsl` already defines `octDecode` next to `octEncode`.
  Note the obvious fix is **not** a drop-in: that header opens with *"NON-STANDALONE
  shader fragment … it references symbols (structs, SSBO/UBO bindings, helper
  functions, constants) defined in `shader_constants.glsl` and in earlier
  includes"* (`sampleDalcCube` reads `dalcPosX` &c.), so the compute shaders
  cannot `#include` it as-is; the codec would have to be split into its own
  standalone header first.
- **Evidence**: I extracted the five bodies and compared them — identical apart
  from where the `vec2(...)` argument list wraps. `grep -rn "math_common.glsl" crates/renderer/shaders/`
  returns four prose mentions plus exactly one real `#include`, from
  `triangle.frag`.
- **Impact**: Drift risk only, on the predicate that gates TAA history
  acceptance (`dot(currNormal, prevNormal) < 0.85`) and SVGF's bilinear
  consistency loop (`dot(currN, prevN) < 0.9`). A future correction to the codec
  (precision, dropping the `normalize`, an snorm-range change) applied to some
  copies leaves TAA rejecting history differently from SVGF and from the
  `octEncode` producer, with no test failing.
- **Related**: `#3607` (`20f5f476`); the 2026-08-30 `D13-04`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Cheapest closure is a body-equality source scan next to the
  existing pin: extract the `vec3 octDecode(vec2 e) { … }` span from all five
  files, strip whitespace, and assert all five are equal. Real fix is to split a
  standalone `include/oct_codec.glsl` out of `math_common.glsl` and have all
  five `#include` it.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4009: REN-2026-09-06-D14-01: six rotted `file:NN` cross-references inside the caustic / water / volumetrics sources point at unrelated code

Labels: documentation, renderer, low, sync, water, shaders, doc-rot

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D14-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (comment accuracy; no runtime effect)
- **Dimension**: Caustics / Water / Volumetrics (in-code doc-rot)
- **Location**:
  - `crates/renderer/src/vulkan/water_caustic.rs` — module docstring, *"the caustic pipeline's pre-clear barrier at `caustic.rs:720-735`"*
  - `crates/renderer/shaders/caustic_splat.comp` — *"see draw.rs:268-273"*, *"`INSTANCE_FLAG_CAUSTIC_SOURCE` in `shader_constants_data.rs:86`"*, *"the Rust ↔ define lockstep assertion at shader_constants.rs:313-320"*
  - `crates/renderer/src/vulkan/volumetrics.rs` — two sites, both *"Mirrors `CausticPipeline::write_tlas` (caustic.rs:627)"*
  - `crates/renderer/shaders/water.frag` — *"the same Nperturbed already used by the primary refraction ray above (line ~547)"*
- **Status**: **NEW.** No open issue matches (searched `line number`, `line-number`, `doc-rot` + the file names). The adjacent `crates/renderer/src/vulkan/context/resize.rs` *"matches init behaviour at mod.rs:1422-1426"* is the same class in a Dim-16-adjacent file and is included in the fix scope below.
- **Description**: Each of these was correct when written and now names a different construct. Verified individually against HEAD:

  | Cited | Claimed to be | What is actually there |
  |---|---|---|
  | `caustic.rs:720-735` | the pre-clear barrier | the `write_combined_image_sampler` / `write_storage_buffer` block inside `write_descriptor_sets` |
  | `draw.rs:268-273` | the `sceneFlags.x` RT gate | a `morph_slot_backs_mesh` unit-test assertion |
  | `shader_constants_data.rs:86` | `INSTANCE_FLAG_CAUSTIC_SOURCE` | `VERTEX_STRIDE_FLOATS` (the real definition is ~380 lines further down) |
  | `shader_constants.rs:313-320` | the Rust↔`#define` lockstep assertion | the GLSL tokenizer's `while index < bytes.len()` loop |
  | `caustic.rs:627` (×2) | `CausticPipeline::write_tlas` | the image-view-creation error arm inside `create_slot` (`write_tlas` is ~166 lines later) |
  | `water.frag` "line ~547" | the primary refraction ray | `foamShoreline`'s `sceneFlags.x` early-out |
  | `context/mod.rs:1422-1426` | the bloom-init hard-fail | the `skin_first_sight_builds_scratch` field declaration |

  All seven still resolve to *plausible-looking* code, which is what makes them costly: a reader who follows one lands somewhere real and draws the wrong conclusion rather than noticing the reference is dead.
- **Evidence**: `sed -n` at each cited range, reproduced in the table above. `grep -n "pub fn write_tlas" crates/renderer/src/vulkan/caustic.rs` → line 793, not 627. `grep -n "INSTANCE_FLAG_CAUSTIC_SOURCE" crates/renderer/src/shader_constants_data.rs` → line 469, not 86.
- **Impact**: Auditor and maintainer time only — but this is the exact class the audit skill's *"Symbols, not line numbers — line anchors rot on every refactor"* rule exists to prevent, and the rule is currently enforced only on audit skill files (`_audit-validate.sh`) and not on production comments. Two of the seven sit in `caustic_splat.comp`, a file that changed three times in two days.
- **Related**: #1114 (the path-reference convention), the `_audit-validate.sh` gate, #3866 / #3842 / #3846 (the same doc-rot family in the acceleration and bindings docs).
- **Suggested Fix**: Replace each with the symbol it means — `CausticPipeline::clear_for_skip` / the moving-camera arm of `CausticPipeline::dispatch`; the `patch_camera_rt_flag` site in `draw_frame`; the bare constant name `INSTANCE_FLAG_CAUSTIC_SOURCE` plus the two tests that actually pin it (`caustic_splat_comp_uses_named_instance_flag_constant` and `instance_flag_bits_match_scene_buffer_consts`, both in `crates/renderer/src/shader_constants.rs`); `CausticPipeline::write_tlas`; `traceWaterRay`'s refraction call; `VulkanContext::new`'s bloom-init arm. Cheap, and it is the same rule the audit tooling already enforces one directory over.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix


---

# #4010: REN-2026-09-06-D15-01: `water.frag`'s caustic refraction hardcodes `1.0 / 1.33` while its own primary refraction ray uses the authored `WaterMaterial::ior`

Labels: bug, renderer, low, water, shaders

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D15-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (dormant today — nothing in the cell loader currently overrides the 1.33 default; becomes a visible divergence the moment a WATR record or a tuning pass sets one)
- **Dimension**: Water (water-side caustics)
- **Location**: `crates/renderer/shaders/water.frag` — the caustic block's `refract(-sunDir, causticNormal, 1.0 / 1.33)`, versus the primary refraction's `float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);` where `float ior = push.timing.w;` (`push` is the `#define push waterParams.params[drawPush.waterIndex]` alias for one `WaterParams` SSBO record, **not** a Vulkan push constant). CPU side: `WaterMaterial::ior` (`crates/core/src/ecs/components/water.rs`, default `1.33`) → `GpuWaterParams::timing[3]` (`crates/renderer/src/vulkan/water.rs`), filled from `mat.ior` in `byroredux/src/render/water.rs`. Sibling writer: `crates/renderer/shaders/caustic_splat.comp`.
- **Status**: **NEW.** No open issue (searched `ior`, `1.33`, `caustic`). Not raised by the 2026-09-04 `water-deep` run, which examined this block for its bounds guard (`REN-WD-D2-01`, now fixed as #3820) rather than its eta.
- **Description**: The two caustic writers were deliberately aligned on everything else — the `CAUSTIC_FIXED_SCALE` fixed-point basis, the normalised 5×5 footprint, the `sunDirection` points-to-the-sun convention (`#1635`/`#1459`), `offsetRayOriginForDirection`'s zero-`tMin` origin contract, and (as of #3820) the `imageSize`-based bounds rule. They are **not** aligned on where the refractive index comes from, and the glass side is the one that does it correctly:

  ```glsl
  // caustic_splat.comp — reads the per-draw value, falls back to the pipeline default
  float instanceIor = instances[instIdx].ior;
  float ior = instanceIor > 1.0 ? instanceIor : causticTune.y;

  // water.frag — primary refraction ray, authored value
  float ior = push.timing.w;
  float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);

  // water.frag — caustic refraction ray, literal
  vec3 refractDir = refract(-sunDir, causticNormal, 1.0 / 1.33);
  ```

  `WaterMaterial::ior` is canonical, authorable WATAL state whose own doc says *"1.33 = clean water; bumping up to 1.5 for stylised reads or thick visc fluid"* — i.e. the field exists precisely to be varied. The block's comment (*"η = 1.0/1.33 (air → water)"*) reads as a restatement of the default, not as a deliberate decision to ignore the authored value; nothing nearby argues for independence, and the same block already reuses `sunVisibility` computed further up rather than recomputing it.
- **Evidence**: `grep -n "ior\|1\.33" crates/renderer/shaders/water.frag` → `float ior = push.timing.w;` and `1.0 / max(ior, 1.0)` in the refraction block, `1.0 / 1.33` in the caustic block. `grep -n "ior" crates/core/src/ecs/components/water.rs` → `pub ior: f32` with `ior: 1.33` in `Default`. `grep -rn "ior" byroredux/src/render/water.rs` → `mat.ior` into `timing`. No cell-loader or EXAL site currently writes `WaterMaterial::ior`, so the two agree at runtime today. The Rust↔GLSL agreement of the record itself is separately pinned by `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep` — the *record* is guarded, only its consumer diverges.
- **Impact**: Latent. Any authored or tuned water IOR ≠ 1.33 makes the caustic pattern on the lake bed refract at a different angle than the visible refraction of the same surface — the caustic focus and the seen-through geometry disagree, which reads as the caustic being registered to the wrong place rather than as a colour/intensity error. This is also exactly the failure mode `86976f56` (#3912) swept for the glass defaults one day earlier: a canonical constant plumbed to one consumer and hardcoded at another.
- **Related**: #3912 / `86976f56` (the named-default doctrine this violates), #3745 (`887c5d18`, which consolidated `water.frag`'s three RT reach budgets into `shader_constants_data.rs` — the precedent for removing literals from this shader), #1210 / #1255 (the water caustic phases), #3820 (the last time the two writers were brought into agreement).
- **Suggested Fix**: One-line change — `refract(-sunDir, causticNormal, 1.0 / max(ior, 1.0))`, reusing the `ior` local already in scope from line ~624, and drop the `1.33` from the comment. Pinnable by the `water.rs` source-assertion test style already used for the `#3820` bound (`crates/renderer/src/vulkan/water.rs` has a matching test for the `imageSize` rule); a negative assertion that `water.frag` contains no `1.0 / 1.33` would be the direct guard. Requires a `.spv` recompile.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix


---

