# Safety Audit — 2026-08-20

Run as part of the `comprehensive` `/audit-suite` sweep (25 audits).
Protocol: `.claude/commands/_audit-common.md` · severity scale:
`.claude/commands/_audit-severity.md` · dimensions:
`.claude/commands/audit-safety/SKILL.md` (all 11).

Delta since the previous sweep (`AUDIT_SAFETY_2026-08-16.md`): **335 commits**,
overwhelmingly session-70 WATAL water work plus volumetric combustion transport,
terrain-LOD streaming and CHARAL wiring. Dimensions were weighted toward
`crates/renderer/src/vulkan/water.rs`, `crates/renderer/src/vulkan/volumetrics.rs`,
`crates/plugin/src/esm/records/misc/water.rs`, `byroredux/src/render/water.rs`,
`byroredux/src/env_translate.rs` and the generated shader-constants pipeline
(`crates/renderer/build.rs`), per the dispatch brief.

## Scope line

All 11 dimensions executed. Coverage notes:

- **Dimension 5 was NOT exercised through the validation-layer channel this
  run.** The suite briefing forbids `cargo build`/`cargo test` (25 agents
  contending on the target lock), and running the engine under `BYRO_VALIDATION=1`
  requires a build; the standing "no parallel engine launch" rule also applies.
  Every Vulkan claim below is therefore derived from static evidence about
  *documented spec limits and unwritten descriptors*, not from a speculative
  barrier/render-pass assertion. Per the No-Speculative-Vulkan-Fixes rule, no
  barrier / layout / pipeline-state claim is made in this report — none was
  warranted, and the two Vulkan findings are both provable by reading the code
  against the spec's Required Limits table rather than by observing a VUID.
- **Dimension 11 (`crates/mod-runtime`)** audited as a contract, not a live path
  (still no engine consumer). Two of its three 2026-08-16 findings are still
  OPEN issues → noted and skipped, not re-reported.
- **`crates/fsr3-sys` and `crates/cxx-bridge` are unchanged since 2026-08-16**
  (`git log --since` returns nothing for either). Their Dimension-1 guards were
  re-verified statically and carried as PASS.
- Committed-`.spv` freshness was checked (see PASS §Dimension 5) but is
  `/audit-renderer`'s dimension — cross-referenced, not re-reported.

Dedup performed against `/tmp/audit/issues.json` (400 issues) and
`docs/audits/` (26 prior safety reports).

## Summary

**5 findings**: 0 CRITICAL · 0 HIGH · 4 MEDIUM · 1 LOW.

Four of the five are in code that did not exist at the last sweep. The report's
centre of gravity is the WATAL water surface: it is the newest large Vulkan +
untrusted-input surface in the tree, and it is where all four MEDIUMs live.

### Prior-report disposition

| 2026-08-16 finding | Status today |
|---|---|
| SAFE-2026-08-16-01 (FaceGen `coeff` overflow) | **FIXED** — #3048, `eval.rs:82-93` now checks `coeff.is_finite()` plus a per-vertex output gate |
| SAFE-2026-08-16-02 (`SandboxConfig::validate` floors only) | **FIXED** — #3049, `MAX_WASM_STACK_BYTES_CEILING` + `MAX_FUEL_PER_ENTRY` added |
| SAFE-2026-08-16-03 (log budget has no drain) | Existing: **#3050** (OPEN) — noted and skipped |
| SAFE-2026-08-16-04 (no hostile-bytes `compile` test) | Existing: **#3051** (OPEN) — noted and skipped |
| SAFE-2026-08-16-05 (`REFRACT_PASSTHRU_BUDGET` doc rot) | Existing: **#3052** (OPEN) — **still live at `SKILL.md:257`**; survived the 2026-08-19 symbol-drift sweep (`0b9a0c9d`) because the value sits inside the backticks, exactly as the finding predicted |

### The finding that matters most

**SAFE-2026-08-20-01.** The water delta hardened the *weather* half of the water
UBO against non-finite input and pinned it with a dedicated test
(`non_finite_weather_gust_keeps_water_params_finite`), then wired the *material*
half — which is the one fed by untrusted plugin binary — straight through with
no equivalent gate. Two of the six WATR decoders read floats through
`SubReader::f32()`, which does not filter non-finite values, and the `.clamp()`
calls that look like sanitisers are NaN-transparent in Rust while their
`.max()` siblings a few lines away are not.

---

## Findings

### MEDIUM

#### SAFE-2026-08-20-01: WATR floats reach the water UBO without a finiteness gate — `f32::clamp` is NaN-transparent, and two of six decoders read through the unfiltered `SubReader::f32()`

- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL boundary — NaN/Inf on the GPU)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:363-400` (`decode_data`) and `:694-730` (`decode_dnam_pre_fo4`); NaN-transparent clamps at `:382`, `:385`, `:388`, `:715`, `:718`, `:721`. Downstream: `byroredux/src/env_translate.rs:715-716`, `:737`, `:753`; assembly into the GPU record at `byroredux/src/render/water.rs:223-330`.
- **Status**: NEW
- **Description**: `GpuWaterParams` is assembled field-by-field from `WaterMaterial` with no finiteness pass, and there is no `WaterMaterial` analogue of `Material::resolve_pbr` anywhere in the tree (`grep -n "fn sanitize\|fn resolve\|fn validate" crates/core/src/ecs/components/water.rs` returns nothing). Whether a non-finite value can reach it therefore depends entirely on the WATR decoders. Four of the six filter correctly; two do not:

  - `read_f32_at` (`water.rs:901-905`) ends with `value.is_finite().then_some(value)`, so every decoder built on it (`decode_dnam_fo4`, `decode_dnam_fo76`, `decode_dnam_starfield`, `apply_skyrim_dnam_tail`) is clean. So is the `NAM1` arm (`:1322`) and the `NAM0` arm (`:1382`), both of which check `is_finite()` explicitly.
  - `SubReader::f32` (`crates/plugin/src/esm/sub_reader.rs:152-155`) is a bare `f32::from_le_bytes` with **no** finiteness check. `decode_data` and `decode_dnam_pre_fo4` read nine floats each through it.

  The `.clamp()` calls on those values are not a rescue. Rust's `f32::clamp` is specified to return `NaN` when `self` is `NaN` (it is `if x < min {min} else if x > max {max} else {x}`, and both comparisons are false for NaN). The `.max()` calls a few lines away in the *same functions* — `fog_near = v.max(0.0)` — *do* filter, because `f32::max` returns the non-NaN operand. The file therefore mixes a NaN-safe idiom and a NaN-transparent one with no visible distinction.

  Four fields are worse still: `wind_speed` (`:369`), `wind_direction` (`:372`), `wave_amplitude` (`:375`, `:709`) and `wave_frequency` (`:378`, `:712`) are assigned **raw**, with neither `clamp` nor `max`.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/misc/water.rs:374-389 (decode_data)
  if let Ok(v) = r.f32() { p.wave_amplitude = v; }          // raw
  if let Ok(v) = r.f32() { p.wave_frequency = v; }          // raw
  if let Ok(v) = r.f32() { p.sun_specular_power = v.clamp(1.0, 2048.0); }  // NaN-transparent
  if let Ok(v) = r.f32() { p.reflectivity = v.clamp(0.0, 1.0); }           // NaN-transparent
  if let Ok(v) = r.f32() { p.fresnel = v.clamp(0.0, 1.0); }                // NaN-transparent
  if let Ok(v) = r.f32() { p.fog_near = v.max(0.0); }                      // NaN-SAFE
  ```
  Carried through unchanged:
  ```rust
  // byroredux/src/env_translate.rs
  715:  mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);   // NaN in → NaN out
  716:  mat.reflectivity = rec.params.reflectivity;
  737:  mat.wave_amplitude = rec.params.wave_amplitude;
  753:  mat.sun_specular_power = rec.params.sun_specular_power;
  ```
  Into the UBO record:
  ```rust
  // byroredux/src/render/water.rs:255-272
  tune:         [mat.uv_scale_a, mat.uv_scale_b, mat.shoreline_width,
                 mat.wave_amplitude * wind_wave_scale],
  misc:         [mat.fresnel_f0, mat.wave_frequency, …, mat.sun_specular_power],
  tint_reflect: [.., .., .., mat.reflectivity],
  ```
  `decode_dnam_pre_fo4` is the **Skyrim** DNAM decoder (`:1352-1357`) and the
  `_ =>` fallback for FO3/FNV/Oblivion (`:1359`) — first-tier target games with
  real water content. Reachability is not hypothetical: `watr_data_layout_shift.md`
  records that these offset maps are field-shifted on ~88% of vanilla WATR (the
  parser's `wind_speed` is a constant 90.0 across the corpus), i.e. these readers
  are already interpreting neighbouring bytes — colour bytes, flags, FormIDs — as
  `f32`. An arbitrary 32-bit word decodes to NaN or ±inf for any exponent-`0xFF`
  pattern.
  The delta explicitly guards the *sibling* input and tests it:
  ```rust
  // byroredux/src/render/water.rs:106  (WindField, an engine resource)
  let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
  // byroredux/src/render/water_wave_params_tests.rs:266-285
  fn non_finite_weather_gust_keeps_water_params_finite() { … assert!(params.tune[3].is_finite()); }
  ```
  The untrusted input received no such treatment.
- **Impact**: A single non-finite WATR field yields a NaN in the water UBO for every visible water plane using that record. `water.vert:188` reads `clamp(water.tune.w, 0.0, 32.0)` for wave amplitude — GLSL `clamp`/`min`/`max` are *undefined* for NaN operands per the GLSL spec, and lower on NVIDIA's `x < y ? y : x` expansion NaN survives — so a NaN amplitude produces NaN vertex positions and an undefined-rasterisation water primitive. On the shading side a NaN `fresnel_f0` / `reflectivity` / `sun_specular_power` writes NaN into the HDR colour target, which is then consumed by the TAA history and the bloom downsample pyramid; both are neighbourhood filters, so a single NaN pixel spreads rather than staying local. Blast radius is per-water-record, on Skyrim/FO3/FNV/Oblivion, i.e. the whole classic-game tier.
- **Related**: **#2687** (OPEN, SAFE-D9-01) is the same class — a renderer-bound producer with no finiteness gate — on the `Material`/save-restore path rather than `WaterMaterial`. **#2489** (OPEN) is the `mat.set` console clamp. This finding is a third, distinct producer. Also adjacent: the FIXED #3048 (FaceGen), which established that "checks the inputs, not the outputs" is the recurring shape of this bug.
- **Suggested Fix**: Cheapest correct fix is one line in `crates/plugin/src/esm/sub_reader.rs` — but `SubReader::f32` is shared by every ESM record type, so scope it instead: add a `f32_finite()` helper (or route `decode_data` / `decode_dnam_pre_fo4` through the existing `read_f32_at`, which already has the filter). Belt-and-braces: give `WaterMaterial` a `resolve()` that clamps every scalar to a finite range and call it at the single `env_translate` exit, mirroring `Material::resolve_pbr`; pin it with a `non_finite_watr_keeps_water_params_finite` test alongside the existing weather-gust one.

---

#### SAFE-2026-08-20-02: the water params UBO is 65,472 B and its guard test names 64 KiB as "Vulkan's portable `maxUniformBufferRange` floor" — the spec floor is 16 KiB, and nothing queries the device limit

- **Severity**: MEDIUM
- **Dimension**: 5 (Vulkan spec compliance) + 6 (GPU table layout soundness)
- **Location**: `crates/renderer/src/vulkan/water.rs:169-172` (`MAX_WATER_DRAWS` + its rationale comment), `:433-434` (buffer size), `:459-466` (the descriptor write that carries the range), `:911-915` (the guard assertion)
- **Status**: NEW
- **Description**: `MAX_WATER_DRAWS = 186` × `size_of::<GpuWaterParams>() = 352` = **65,472 bytes**, uploaded as a `UNIFORM_BUFFER` and bound with `range = param_buffer_size`. Both the constant's doc comment and the guard test justify that figure by calling 64 KiB the *portable* `maxUniformBufferRange` floor. It is not. The Vulkan specification's Required Limits table sets `maxUniformBufferRange` at **16384 bytes**; 64 KiB is the common *reported* value on desktop drivers (and the D3D11 constant-buffer size), not the guarantee. The buffer is therefore **4× the spec-guaranteed maximum**, and `grep -rn "max_uniform_buffer_range" crates/ byroredux/` returns **nothing** — the device limit is never queried, so there is no runtime clamp, no fallback, and no diagnostic.

  This is the renderer's only large UBO. Every other bulk per-draw array in the tree is a `STORAGE_BUFFER` (`scene_buffer/buffers.rs:510`/`:566` are single-record camera/DALC UBOs; volumetrics' fog-volume, cluster and index arrays are all `STORAGE_BUFFER` at `volumetrics.rs:1091-1113`). The water path is the one place that put a 186-element array in a uniform block.
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/water.rs:169-172
  /// Fixed UBO capacity: 186 × 352 B = 65,472 B, below Vulkan's portable
  /// `maxUniformBufferRange` floor while leaving room for the
  /// handful of water bodies normally visible in one cell.
  pub const MAX_WATER_DRAWS: usize = 186;
  ```
  ```rust
  // crates/renderer/src/vulkan/water.rs:911-915
  assert!(
      MAX_WATER_DRAWS * std::mem::size_of::<GpuWaterParams>() <= 64 * 1024,
      "water UBO must fit Vulkan's portable maxUniformBufferRange floor"
  );
  ```
  ```rust
  // crates/renderer/src/vulkan/water.rs:459-466 — the range that must satisfy the limit
  let info = [vk::DescriptorBufferInfo { buffer: buffer.buffer, offset: 0, range: param_buffer_size }];
  let write = write_uniform_buffer(water_caustic_descriptor_sets[frame], 1, &info);
  unsafe { device.update_descriptor_sets(&[write], &[]) };
  ```
  On a conforming device that reports the spec minimum, that write violates
  **VUID-VkDescriptorBufferInfo-range-00342** (`range` must be ≤
  `maxUniformBufferRange`), and the corresponding shader-side block exceeds
  `maxUniformBufferRange` at draw time.
- **Impact**: Latent on the dev GPU (RTX 4070 Ti reports 65,536) and on every mainstream RT-capable desktop part, which is why this is MEDIUM and not HIGH — the `ray_query` gate on water pipeline creation (`context/mod.rs:2233`) narrows the device set considerably. What makes it worth filing is the *headroom arithmetic the wrong comment invites*: the true remaining margin against the real-world 64 KiB ceiling is **64 bytes**. Adding one `vec4` to `GpuWaterParams` (352 → 368) puts the buffer at 68,448 B and breaks essentially every device, and someone reading "below Vulkan's portable floor" reasonably concludes they have room. The assertion would catch the growth — but the reader has been told the wrong reason it exists, which is the same failure mode as `GpuMaterial` being documented at 300 B after it grew to 348.
- **Related**: Dimension 6's `GpuMaterial` pins are the model to copy — `MAX_MATERIALS = 16384` is an SSBO precisely because uniform blocks cannot carry that. #2688 (OPEN) is the sibling "the pin exists but pins the wrong property" finding on `GpuMaterial`.
- **Suggested Fix**: Correct both the constant's doc comment and the assertion message to say 16 KiB is the spec floor and 64 KiB is the assumed-desktop ceiling this design deliberately targets. Then either (a) query `VkPhysicalDeviceLimits::maxUniformBufferRange` at `WaterPipeline::new` and clamp `MAX_WATER_DRAWS` (with the geometry-pass `.take()` already reading the constant, a runtime value threads through cleanly), or (b) move the array to a `STORAGE_BUFFER` like every other bulk per-draw array in the renderer, which removes the limit question entirely and costs one `layout(std430)` change in `water.vert`/`water.frag`.

---

#### SAFE-2026-08-20-03: six `unsafe` blocks carry no SAFETY comment — two of them in the water delta, four in the new `compute_blas_budget` probe

- **Severity**: MEDIUM
- **Dimension**: 4 (unsafe-block discipline)
- **Location**: `crates/renderer/src/vulkan/water.rs:448-453`, `:466`; `crates/renderer/src/vulkan/acceleration/predicates.rs:679-683`, `:684`, `:685`, `:687`
- **Status**: NEW
- **Description**: A mechanised sweep of every `.rs` file under `crates/`, `byroredux/` and `tools/` finds **699 `unsafe {` blocks**. 693 carry a SAFETY comment either immediately above or as the first line inside the block (the house convention). Six do not. All six are correct as written — none of them is unsound — but per `_audit-severity`'s Special Rules an `unsafe` block without a safety comment is a MEDIUM regardless, and both clusters are inconsistent with their *immediate* neighbours, which is what makes them look like oversights rather than a deliberate style choice.

  In `water.rs`, the partial-init cleanup at `:422` (twenty-six lines earlier) carries a full one-line SAFETY rationale and the near-identical cleanup at `:448` carries none; the `update_descriptor_sets` at `:466` has none while the byte-identical call at `:509` does.

  In `predicates.rs`, `compute_blas_budget` (added by #3043, after the last sweep) is a four-call `unsafe` sequence — create a probe buffer, query its memory requirements, destroy it, query physical-device memory properties — with no comment anywhere in the function.
- **Evidence**:
  ```
  $ python3 sweep.py   # unsafe { blocks vs SAFETY comment, 14 lines before / 25 inside
  total unsafe blocks: 699
  missing SAFETY: 9
  ```
  Three of the nine are false positives on manual read and are NOT part of this
  finding: `crates/renderer/src/vulkan/buffer.rs:1093` and
  `crates/nif/src/stream.rs:467` both carry SAFETY comments longer than the
  window (17 and 20 lines respectively), and
  `crates/renderer/src/vulkan/context/draw.rs:3556` is the word `unsafe` in prose.
  The six real sites:
  ```rust
  // water.rs:445-454 — no SAFETY, unlike the identical cleanup at :422
  for buffer in &mut param_buffers { buffer.destroy(device, allocator); }
  unsafe {
      device.destroy_pipeline(pipeline, None);
      device.destroy_pipeline_layout(pipeline_layout, None);
      device.destroy_descriptor_pool(water_caustic_descriptor_pool, None);
      device.destroy_descriptor_set_layout(water_caustic_set_layout, None);
  }
  ```
  ```rust
  // water.rs:466 — no SAFETY, unlike the identical call at :509
  unsafe { device.update_descriptor_sets(&[write], &[]) };
  ```
  ```rust
  // acceleration/predicates.rs:679-687 — four bare unsafe blocks, no comment in the fn
  let probe = unsafe { device.create_buffer(&create_info, None)…? };
  let requirements = unsafe { device.get_buffer_memory_requirements(probe) };
  unsafe { device.destroy_buffer(probe, None) };
  let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
  ```
- **Impact**: No runtime impact — every invariant holds. The cost is that the
  house convention is what makes the *next* audit's mechanised sweep meaningful:
  #2692 already retired one phantom "SAFETY gap" work item, and the value of the
  remaining sweep depends on the miss list being short enough that each entry is
  worth reading. The `predicates.rs` cluster is the one with a real (if small)
  invariant worth stating: the probe buffer must be destroyed before the function
  returns on every path, and it currently leaks on the `?` at `:681`.
- **Related**: #2683 / #2684 / #2692 (all CLOSED) were the previous rounds of this
  same sweep. The prior report's count was 683/683; the six new misses arrived
  with the water UBO (`ed3570ad`) and the BLAS-budget probe (#3043).
- **Suggested Fix**: Add the four missing comments. While in `compute_blas_budget`, note that the `?` on `create_buffer`'s `.context(...)` at `:681` is fine (nothing allocated yet) but a future fallible call added between `:679` and `:685` would leak the probe — worth stating in the comment so the shape is deliberate.

---

#### SAFE-2026-08-20-04: if both the water-caustic accumulator and its 1×1 placeholder sink fail to create, set 2 binding 0 is never written — and `bind_pass` binds it anyway for an `imageAtomicAdd`

- **Severity**: MEDIUM
- **Dimension**: 5 (Vulkan spec compliance) + 3 (resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:2283-2296` (placeholder creation, `None` on failure), `:2355-2372` (the descriptor-wiring block that silently no-ops when `views` is `None`); consumed at `crates/renderer/src/vulkan/water.rs:590-629` (`bind_pass`, unconditional) and `crates/renderer/shaders/water.frag:1245` (`imageAtomicAdd`)
- **Status**: NEW
- **Description**: #2142 closed the case where `WaterCausticAccum::new` fails, by adding a 1×1 `PlaceholderImage` storage sink to rebind to. The comment it left behind states the hazard precisely:

  > `record_draw` binds set 2 unconditionally and the shader now *writes* it via
  > `imageAtomicAdd`, so leaving the descriptor unwritten (init) or pointing at a
  > destroyed view (resize failure) is an atomic write to freed memory, not a
  > harmless no-op.

  The placeholder's own creation is likewise fallible and likewise degrades to
  `None` with only a `log::warn!`. When both are `None`, `views` is `None`, the
  `if let Some(views)` guard skips the descriptor write entirely — and nothing
  downstream disables the water pipeline. `self.water` is still `Some`, so
  `geometry_pass.rs:521-537` binds and draws. The only gate on the water loop is
  `water.params_ready(frame)`, which is about the *UBO* (binding 1, written
  unconditionally in `new()` at `:466`), not the storage image (binding 0).
- **Evidence**:
  ```rust
  // context/mod.rs:2283-2296 — placeholder failure degrades to None
  let placeholder_caustic_sink = match super::placeholder::PlaceholderImage::new_storage_sink(…) {
      Ok(p) => Some(p),
      Err(e) => { log::warn!("Caustic-sink placeholder creation failed: {e} — water set 2 \
                              has no fallback if the accumulator drops out"); None }
  };
  ```
  ```rust
  // context/mod.rs:2356-2372 — both None ⇒ no write, and no compensating action
  let views: Option<Vec<vk::ImageView>> = match water_caustic_accum.as_ref() {
      Some(accum) => Some(…),
      None => placeholder_caustic_sink.as_ref().map(|p| vec![p.view; MAX_FRAMES_IN_FLIGHT]),
  };
  if let Some(views) = views { w.update_water_caustic_descriptors(&device, &views); }
  // <- no `else { water = None; }`
  ```
  ```rust
  // water.rs:621-628 — set 2 bound unconditionally
  device.cmd_bind_descriptor_sets(cmd, GRAPHICS, self.pipeline_layout, 2,
                                  &[self.water_caustic_descriptor_sets[frame]], &[]);
  ```
  ```glsl
  // water.frag:187, :1245 — and written
  layout(set = 2, binding = 0, r32ui) uniform uimage2D waterCausticAccum;
  imageAtomicAdd(waterCausticAccum, q, fixedVal);
  ```
  The warn text itself names the residual: "water set 2 has **no fallback** if
  the accumulator drops out" — accurate, and the code then proceeds as if it did.
- **Impact**: Requires both an accumulator allocation failure and a 1×1 image
  allocation failure in the same session — realistically device-OOM, so this is
  narrow. But the consequence when it fires is a draw against an
  never-written `VkDescriptorSet` slot, i.e.
  **VUID-vkCmdDrawIndexed-None-08114** and an atomic write through an undefined
  descriptor: undefined behaviour, plausibly `VK_ERROR_DEVICE_LOST`, on a machine
  that was merely low on memory. Severity is about impact, not likelihood; the
  narrow trigger is what holds this at MEDIUM rather than HIGH.
- **Related**: #2141 / #2142 (both CLOSED) built the placeholder mechanism this
  finding says has one unhandled arm. The AO placeholder immediately above
  (`:2264-2282`) has the same shape but is benign — scene binding 7 is *sampled*,
  not written, and the shader tolerates a stale bind.
- **Suggested Fix**: In the `views == None` arm, set `water = None` (and log at
  `error!` rather than `warn!`): the water pipeline is already designed to be
  optional — `context/mod.rs:1513` documents "draw site is gated on `Some` so a
  failure simply skips water" — so this reuses the existing degradation path
  rather than adding one. Alternatively make `PlaceholderImage::new_storage_sink`
  failure fatal to `WaterPipeline::new`, which keeps the invariant "if `self.water`
  is `Some`, set 2 is fully written" locally checkable.

---

### LOW

#### SAFE-2026-08-20-05: `VulkanContext::drop`'s hoisting comment still says the water teardown "needs no allocator" — it has owned an `Arc` clone of the shared allocator since the param UBOs landed

- **Severity**: LOW
- **Dimension**: 3 (memory & resource leaks — drop ordering)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:3837-3841` (the stale note), `:3920-3922` (the hoisted call); ground truth at `crates/renderer/src/vulkan/water.rs:255-261` and `:719-727`
- **Status**: NEW
- **Description**: `#1483` moved `water.destroy()` into `Drop`'s allocator-*independent* block, with a comment explaining why that is safe: "its pipeline + caustic descriptor pool need no allocator." That was true then. Commit `ed3570ad` subsequently gave `WaterPipeline` a `Vec<GpuBuffer>` of per-FIF host-visible parameter UBOs, and with it an `allocator: Option<SharedAllocator>` field, precisely so it could still free them from that block. The mechanism is correct and its own field doc says so — but the `Drop`-side comment a reader hits first now describes a subsystem that no longer exists, and it is exactly the comment someone consults before reordering teardown.

  The ordering is currently sound and worth recording as such: `destroy()` frees the UBOs and then sets `self.allocator = None`, dropping its `Arc` clone; that happens at `:3921`, far above the `Arc::try_unwrap` at `:4040`, so the strong count is released in time and the `#665`/LIFE-L1 leak-instead-of-use-after-free fallback is not engaged.
- **Evidence**:
  ```rust
  // context/mod.rs:3837-3841 — stale
  // NOTE: `self.water` teardown hoisted to the
  // allocator-independent block near the top of Drop
  // (#1483) — its pipeline + caustic descriptor pool need no
  // allocator. The per-FIF `water_caustic_accum` images
  // below DO need the allocator and stay here.
  ```
  ```rust
  // water.rs:255-261 — the field that contradicts it
  /// Retained so the allocator-independent context teardown can still
  /// release these buffers before the allocator is unwrapped.
  allocator: Option<SharedAllocator>,
  ```
  ```rust
  // water.rs:719-727 — allocator-dependent work inside the "allocator-independent" destroy
  if let Some(allocator) = self.allocator.as_ref() {
      for buffer in &mut self.param_buffers { buffer.destroy(device, allocator); }
  }
  self.allocator = None;
  ```
- **Impact**: Documentation only today. The hazard it creates is specific: a
  future reader trusting the note could conclude `WaterPipeline` holds no
  allocator reference and (a) skip `destroy()` on some new early-return path,
  stranding an `Arc` clone that makes `Arc::try_unwrap` fail and pushes teardown
  into the leak-the-device fallback, or (b) reorder the hoisted block after the
  allocator is taken, which would strand the UBO allocations outright.
- **Related**: #1483 (the hoist), #665 / LIFE-L1 (the `try_unwrap` fallback the
  stale note could route teardown into), #732 / LIFE-N1 (the same
  "drop the `GpuBuffer` structs so their `Arc` clones release now" pattern,
  correctly applied and commented in `volumetrics.rs:2695-2701`).
- **Suggested Fix**: Rewrite the note to say the water teardown is hoisted because
  `WaterPipeline` carries its **own** `SharedAllocator` clone and can therefore
  free its param UBOs without the context's, and that `destroy()` must stay ahead
  of the `Arc::try_unwrap` at `:4040`. One sentence.

---

## Verified-intact regression guards (PASS — not findings)

Recorded so a future run does not re-derive them. Per the skill's procedure
step 8, a confirmed-intact guard is a PASS, not a NEW finding.

### Dimension 1 — FFI lifetime
- **`crates/fsr3-sys`**: **unchanged since 2026-08-16** (`git log --since=2026-08-16 -- crates/fsr3-sys/` is empty). Both `pub unsafe fn` still carry `# Safety` sections (`lib.rs:365`, `:403`) and `Drop` still cross-references `create`'s Vulkan-idle contract (`:485`). No new FFI lifetime issue; nothing to escalate to the CRITICAL class.
- **cxx scope guard**: `crates/cxx-bridge/src/lib.rs` still exposes exactly `native_hello() -> String`. No `*const`, no `&[u8]`, no `Box<…>`, no fn taking a Rust reference. Still a placeholder; the dimension stays dormant.
- **Ruffle/wgpu (`crates/ui`)**: changed in the delta (#2964, #2966, #2967) but only in the host-bridge diagnostics/catalog surface. `crates/ui` still contains **zero** `unsafe` code (its single `grep` hit is prose), and #2964/#2967 both *bound* previously-unbounded guest-keyed collections — the opposite of a regression. The capture-buffer ownership model (`into_raw()` → copy into player-owned `pixel_buffer` → `Option<&[u8]>` bounded by `&mut self`) is untouched.

### Dimension 2 — Memory corruption / UB
- **ECS cached-pointer contract (#35/#1367)**: `crates/core/src/ecs/query.rs` and `world.rs` are **unchanged since 2026-08-16** — the guard-first field layout, the `&mut self` gate on `&mut *self.storage`, and the deref-free custom `Drop` impls all stand as verified last sweep.
- **`#[repr(C)]` GPU structs**: `scene_buffer/gpu_types.rs` still forbids `[f32; 3]` in its module doc and contains no such member (the only `[f32; 3]` hits in the file are inside that prohibition). `GpuWaterParams` (`water.rs:65-140`) follows the rule — 22 flat `[f32; 4]`/`[u32; 4]` slots, no bare vec3 — and is pinned by a `const _: () = assert!(… == 352)` plus a runtime assertion.
- **NIF bulk POD reads**: `read_pod_vec` (`crates/nif/src/stream.rs:439-469`) keeps the `checked_mul` byte-count guard, `check_alloc`, the `T: AnyBitPattern` bound, the big-endian compile gate and its 17-line SAFETY comment.
- **pex opcode transmute / sfmaterial decode**: unchanged; both crates untouched in the delta.

### Dimension 3 — Leaks & drop ordering
- **Rapier release on cell unload (#1520)**: `byroredux/src/cell_loader/rapier_release_tests.rs` present; `crates/physics/src/world.rs` `remove_*` live.
- **Deferred-destroy drain (#418/#732)**: `context/draw.rs:1623-1640` still ticks the mesh/texture/accel queues **after** `wait_for_fences`, with the `texture_registry.begin_frame` ordering note intact.
- **`AllocatorResource` removal (#1406)**: `byroredux/src/app_events.rs:59` removes, `:126` re-inserts on `resumed`.
- **BLAS-scratch shrink SAFETY (SAFE-2026-08-07-04)**: still commented at `byroredux/src/cell_loader/unload.rs:276-283`.
- **`VolumetricsPipeline` teardown**: every Vulkan-handle field on the struct is covered by `destroy()` (`volumetrics.rs:2676-2765`) — all six froxel volume vectors plus both noise volumes are drained through one chain, all six buffer vectors are destroyed **and cleared** (the #732 `Arc`-release pattern), both pipeline/layout/pool/set-layout pairs and all three samplers are null-guarded. Field-by-field diff against the struct definition (`:723-800`) shows no miss.
- **`WaterPipeline` teardown**: complete and idempotent (`water.rs:697-728`); every partial-init failure path in `new()` destroys in reverse dependency order (`:352`, `:376-379`, `:410-412`, `:424-427`, `:445-452`).
- **CPU-side growth in the delta**: no unbounded per-cell/per-frame `Vec` or `HashMap` in the new water/volumetrics code. `combustion_light_candidates` is `.clear()`ed each frame (`volumetrics.rs:2376`) and bounded by `COMBUSTION_LIGHT_GRID_COUNT`; `fog_cluster_entries`/`fog_cluster_indices`/`fog_volume_upload` are `Box`ed fixed-size arrays; `param_scratch` is cleared and `.take(MAX_WATER_DRAWS)`-bounded (`water.rs:539-543`).

### Dimension 4 — Unsafe-block discipline
Mechanised sweep of `crates/` + `byroredux/` + `tools/`: **699 `unsafe {` blocks, 693 with a SAFETY comment**, six without → SAFE-2026-08-20-03 above. Per-crate token recount (`grep -ro unsafe <crate>/src | wc -l`):
renderer 791 · nif 11 · fsr3-sys 11 · byroredux 11 · core 6 ·
ui/plugin/pex/facegen/cxx-bridge 1 each ·
audio/bgsm/bsa/debug-protocol/debug-server/debug-ui/hkx/mod-runtime/papyrus/physics/platform/save/scripting/sfmaterial/spt **0**.
Note `byroredux`'s jump 2 → 11 is a **token-counting artefact**, not new unsafe:
ten of the eleven hits are prose (`render/skinned.rs:214`) or the
`serde_attr_declares_unsafe_default` test helper name
(`save_io/serde_default_guard_tests.rs`). `byroredux` still has exactly **one**
real `unsafe` block, at `cell_loader/unload.rs:283`, and it is commented.
`hkx` and `mod-runtime`'s zeros re-verified — for both, the absence is the safety property.

### Dimension 5 — Vulkan spec compliance
Not exercised through the validation-layer channel this run (see Scope line). Static verification:
- **TLAS resize wait (#1390)**: `acceleration/tlas.rs:992` still calls `device.device_wait_idle()` before freeing the old allocation.
- **Volumetrics dispatch gate**: `VOLUMETRIC_OUTPUT_CONSUMED` is `true` (`volumetrics.rs:496`) and the **single** live dispatch site gates on the constant by name (`context/post_passes.rs:494`, dispatching at `:705`). The prior report's "two callers" reflected doc-comment mentions; there is one call site and it is gated.
- **`initialize_layouts` coverage**: present on all seven storage-image passes — `taa.rs`, `water_caustic.rs`, `gbuffer.rs`, `bloom.rs`, `svgf.rs`, `caustic.rs`, `volumetrics.rs`.
- **Water UBO indexing**: `water_index` cannot escape the UBO. `upload_params` fills the buffer from `commands.iter().take(MAX_WATER_DRAWS)` (`water.rs:539-543`) and the draw loop enumerates the *same* prefix, `water_commands.iter().take(MAX_WATER_DRAWS).enumerate()` (`geometry_pass.rs:544-545`), so a `continue` inside the loop cannot desynchronise the index from the upload order. `params_ready(frame)` gates the whole loop.
- **Water push-constant range**: `cmd_push_constants` uses `VERTEX | FRAGMENT` (`water.rs:674-680`) and the layout's `PushConstantRange` declares exactly `VERTEX | FRAGMENT`, offset 0, size 16 (`:391-394`) — stage-flag mismatch VUIDs do not apply. (The `bind_pass` doc at `:574` still says "water has a 16 B `WaterPush` at FRAGMENT", which is stale prose about a correct mechanism; not filed.)
- **Water descriptor pool sizing**: one `STORAGE_IMAGE` + one `UNIFORM_BUFFER` per FIF, `max_sets = MAX_FRAMES_IN_FLIGHT`, matching the `MAX_FRAMES_IN_FLIGHT` sets allocated (`water.rs:336-345`, `:363-374`).
- **Combustion light readback race**: `append_combustion_surface_lights` documents "the caller must have waited the frame slot's fence" (`volumetrics.rs:2327-2333`) and the sole caller honours it — the fence wait is at `context/draw.rs:1480`, the readback at `:1833`. No host/device race on the mapped moment buffer.
- **Committed `.spv` freshness** (cross-ref `/audit-renderer` §3, which owns this): all five shader sources whose last-change commit is newer than their `.spv` were diffed against the `.spv`'s commit — `triangle.frag`, `bloom_upsample.comp`, `ui.vert` and `volumetrics_inject.comp` differ by **comments only**, and `skin_palette.comp`'s one code change (`local_size_x = 64` → `= SKIN_WORKGROUP_SIZE`, where `SKIN_WORKGROUP_SIZE == 64`) is value-preserving. The two delta commits that touched `shader_constants.glsl` **and** `volumetrics_inject.comp` without touching a `.spv` (`1393896c`, `20839b28`) are literal→macro refactors at identical values. No stale `.spv`.

### Dimension 6 — R1 material table
`MAX_MATERIALS = 16384` (`scene_buffer/constants.rs:191`); `upload_materials` keeps the release-visible `assert!` plus `.min(MAX_MATERIALS)` (`upload.rs:646-655`) in lockstep with `intern`'s cap. `GpuMaterial` is still 348 B with a matching test name (`material.rs:40-44`, `:69`, `:85`). Not re-reported: **#2688** (OPEN — the GLSL scalar *type* is not pinned).

### Dimension 7 — RT IOR refraction
`MAX_REFRACT_PASSTHRUS = 8` loop cap live (`triangle.frag:1857` + `:1900`); seven `MATERIAL_KIND_GLASS` gates present; `GLASS_RAY_BUDGET = 2_097_152` in lockstep between `shader_constants_data.rs:260` and the generated `shader_constants.glsl:107`; `DBG_VIZ_GLASS_PASSTHRU = 0x80` (`shader_constants_data.rs:583`) uncollided and present in the `DBG_BITS` catalog (`:846`). Not re-reported: **#2686** (OPEN). The skill's own `REFRACT_PASSTHRU_BUDGET` doc rot is **#3052** (OPEN) and is still unfixed at `SKILL.md:257`.

### Dimension 8 — NPC / animation spawn
FLT_MAX pose-fallback sentinel live throughout `crates/nif/src/anim/bspline.rs` (7 sites) — #772 intact. `AnimationClipRegistry` still interns lowercased (`registry.rs:212`) — #790 intact. `SkinSlotPool` keeps its one-shot `overflow_warned` + `overflow_attempt_count` with bind-pose fallback (`skin_slot_pool.rs:99`, `:129`). Not re-reported: **#2689** (OPEN — the slot vector grows monotonically).

### Dimension 9 — NaN/Inf on the GPU
`translate_material` still calls `resolve_pbr()` on both exit paths. Not re-reported: **#2687**, **#2489** (both OPEN). Within the water delta the *geometry* side is guarded — `cell_loader/water.rs:104` skips non-finite plane vertices and `:114` falls back on a non-finite bbox; `components/water.rs:440-445` guards the flow direction and speed; `render/water.rs:47`, `:106`, `:114`, `:200`, `:209`, `:317` guard the weather/flowmap/angular/rain inputs. The *material* side is SAFE-2026-08-20-01 above.

### Dimension 10 — debug-ui / egui overlay
`EguiPass::new` still destroys `render_pass` on every constructor failure path (`egui_pass.rs:108-150`). The one-frame `pending_free` defer is intact (`:233`, `:300`) and drained again in `destroy()` (`:309`). `VulkanContext::drop` takes and destroys `egui_pass` first, ahead of every other subsystem and the device teardown (`context/mod.rs:3877-3879`). Unchanged in the delta.

### Dimension 11 — Sandboxed mod runtime
- **Absence, not promise**: still no WASI linked; crate still contains **zero** `unsafe`.
- **Resource limits**: SAFE-2026-08-16-02 is **FIXED** (#3049). `validate()` now enforces ceilings as well as floors — `MAX_WASM_STACK_BYTES_CEILING = 1 MiB` (derived from the smallest commonly-documented default OS thread stack, with the residual-margin caveat documented honestly in the constant's own doc) and `MAX_FUEL_PER_ENTRY = 1e12`. The doc comment correctly frames the fuel ceiling as an availability concern rather than a host-abort one.
- Still OPEN and skipped per dedup: **#3050** (log budget is a lifetime total with no drain — `logs()` at `runtime.rs:187` is still read-only, no `take_logs`), **#3051** (no `compile`-hostile-bytes test — `crates/mod-runtime/src/tests.rs` still builds every fixture through `compile_wat`).

---

## Report finalization

Report: `docs/audits/AUDIT_SAFETY_2026-08-20.md`
No GitHub issues were created. To file:

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=4 LOW=1
