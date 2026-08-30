# Renderer Audit — 2026-08-30

**Scope**: Full `/audit-renderer` run as part of an `--preset comprehensive`
audit-suite sweep. All 23 dimensions in
`.claude/commands/audit-renderer/SKILL.md` were walked; no `--focus` filter.
**Depth**: deep (data-flow tracing + invariant validation).
**Repo state**: `main`, HEAD `64f64480`. Delta base `969d81c8` (the
2026-08-27 sweep), 45 commits.
**Method**: 21 parallel dimension agents writing to per-dimension scratch
files, consolidated and cross-checked by the orchestrator against the files on
disk rather than the agents' returned summaries (per the project's recorded
nested-agent relay caveat). Dimensions 12, 14, 15, 21 and 22 were audited by
the orchestrator directly. Every HIGH and every MEDIUM whose premise was
load-bearing was re-verified against source by the orchestrator before
inclusion.
**Dedup**: against all 159 OPEN GitHub issues
(`gh issue list --limit 200`) and the four most recent renderer reports
(`AUDIT_RENDERER_2026-08-27.md`, `…08-24.md`, `…08-20.md`, `…08-16.md`).

**Verification performed**
- `cargo test -p byroredux-renderer --lib` — **777 passed, 0 failed** (up from
  768 at the last sweep).
- Full 22-shader `glslangValidator -V` recompile and byte-compare against
  every checked-in `.spv` — **all 22 byte-identical**. GLSL ↔ SPIR-V lockstep
  is intact at HEAD.
- **No Vulkan device, no RenderDoc capture, and no `BYRO_VALIDATION` run were
  available**, and no on-disk game data was mounted. Every barrier, layout and
  visual verdict below is source-read confidence only. Per the project's
  standing no-speculative-Vulkan-fix policy, **no barrier, render-pass or
  pipeline edit is proposed anywhere in this report**; the two observations
  that would need a capture are collected under *Needs-RenderDoc*.

## Executive Summary

**63 findings after dedup: 0 CRITICAL, 2 HIGH, 13 MEDIUM, 48 LOW.**
A further **14 raw findings were merged away** as cross-dimension duplicates —
an unusually high rate, and itself the most informative signal in this run
(see below).

Two commits dominate the delta and account for most of what follows:

* **`19813460` / `#3530`** wired Oblivion's `APPLY_HILIGHT2` parallax by
  binding the *normal* map into the height slot and flagging "height lives in
  alpha" as bit 31 of `parallaxMapIndex`. The bit-31 masking itself is clean
  in all four shader readers. The two defects around it are both real and both
  were found by more than one dimension independently: the flag is set with
  **no alpha-presence gate** (HIGH), and the claimed channel now **collides**
  with the pre-existing normal-alpha-as-specular-mask mechanism (MEDIUM).
* **`b28acb0c` / `#3426`** moved the Scaleform overlay out of the geometry pass
  into a new presentation pass. The relocation is functionally correct
  everywhere it was checked — but it invalidated a large amount of prose.
  **Twelve of the 48 surviving LOW findings, plus eleven of the fourteen
  merged-away duplicates — 23 of the 77 raw findings — trace to `#3426`
  alone**, spread over two authoritative FSR documents,
  `shader-pipeline.md`'s submission table, `renderer.md`, four in-code
  `# Safety` contracts still naming composite as the swapchain writer, and
  three "see also" pointers to the retired `pipeline_ui` field.

**The duplicate rate is the headline process finding.** Seven distinct defects
were each reported by two to four independent dimension agents. That is strong
corroboration for the findings themselves, but it also means the current
dimension boundaries do not partition the `#3426` and `#3530` blast radii — a
single pass relocation shows up in the sync, pipeline, denoiser, FSR, memory
and telemetry dimensions at once. Filing those as seven issues rather than
twenty-one is the correct outcome and is what this report does.

**No CRITICAL, and no HIGH in the structural core.** Dimensions 1 (AS
correctness), 3 (GPU-struct layout), 4 (synchronisation) and 12 (command
recording) produced **zero** correctness defects between them. Dimensions 12,
14, 15, 21 and 22 produced **no findings at all**.

## RT Pipeline Assessment

Dimensions 1–3 are clean of new CRITICAL/HIGH structural defects.

* **Acceleration structures.** No wrong-geometry, wrong-address or
  missing-barrier defect. 95/95 acceleration tests pass. Deferred BLAS
  destruction, deferred BLAS-scratch destruction, the `built_flags` refit
  assertion, the `instance_custom_index` ↔ draw-index contract and the
  `MAX_INSTANCES < 1 << 24` const-assert are all intact. The two documented
  `#1793` gaps (no recovery path for a permanently-missing rigid BLAS; the
  `--grid` false-evict) remain gated behind `static_blas_bytes > budget` and
  unreachable on the 12 GB dev card — recast, not re-reported.
* **GPU-struct layout is fully in lockstep.** All three sizes re-derived, not
  trusted: **`GpuInstance` = 160 B**, **`GpuCamera` = 368 B**,
  **`GpuMaterial` = 432 B**. `GpuMaterial` carries **108 scalar fields**, with
  **108/108** offset assertions, **108/108** covered by
  `hash_gpu_material_fields`, and a **108/108** name-and-order match against
  the single GLSL mirror in `crates/renderer/shaders/include/bindings.glsl`.
  `GpuInstance` has exactly **5** real mirror sites and every one carries
  `surfaceId`; `ui.vert` is still a live mirror after `#3426`. Two premises
  supplied to the auditor were checked and **dropped as stale** — a sixth
  apparent `GpuInstance` mirror (a comment, not a declaration) and a suspected
  `INSTANCE_FLAG_*` duplication gap that `instance_flag_bits_match_scene_buffer_consts`
  already pins.
* **Ray queries.** RT gating, the ReSTIR-DI 25° geometric-normal cone, the
  stable-surface-ID reuse tag, the thin-glass zero-ray gate, the BC1
  punch-through alpha gate and the `#3530` bit-31 masking in all four readers
  are correct and test-pinned.
* **The two `#3530` defects** are the substantive RT-path findings of this run
  and are described in full below.

## GPU-Struct & Memory Assessment

* **Rust ↔ GLSL ↔ SPIR-V lockstep intact**; all 22 `.spv` byte-identical to a
  fresh recompile. Every GPU-struct *code* problem found this run is a missing
  **guard** rather than a live desync (`D3-01`, `D3-02`, `D7-01`), plus
  documentation drift.
* **`#3443` is fixed and verified.** The chunked geometry rebuild's
  device-headroom bypass that the 2026-08-27 sweep filed as its lone HIGH was
  gated by `fa511bbf`; the two-phase `plan_geometry_compaction` /
  `apply_compaction_plan` offset-publish contract and its four pins are intact.
* **No per-frame leak was found.** The two memory MEDIUMs are ledger accuracy,
  not leaks: three `memory-budget.md` Scene-Buffers rows contradict test-pinned
  constants (`MAX_LIGHTS` by 2×), and a second retained 128 MB `CpuToGpu`
  `StagingPool` has no row at all.
* **Teardown is complete for both new subsystems.** `PresentationPipeline` and
  the depth-capture staging buffer are each created and destroyed in the right
  order; the retired `pipeline_ui` left no orphaned handle (pinned by
  `presentation.rs`'s own `!geometry.contains("pipeline_ui")` assertion).

## Findings

### CRITICAL

*(none)*


### HIGH

#### REN-2026-08-30-D18-01: `--cornell-sun`'s fixed directional sun is overwritten on frame 1 by `apply_neutral_exterior_fallback`, desynchronising `CellLightingRes.directional_dir` from `SkyParamsRes.sun_direction`


- **Severity**: HIGH
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `byroredux/src/systems/weather.rs` (`weather_system`, `apply_neutral_exterior_fallback`), `byroredux/src/cornell.rs` (`install_cornell_lighting`, `sun_dir`), `byroredux/src/scene.rs` (`setup_scene`)
- **Status**: NEW
- **Description**: `cornell.rs`'s module doc states the exterior harness's premise
  verbatim: *"No `WeatherDataRes` is inserted, so `weather_system` stays inert and
  the direction does not drift with TOD."* That is no longer true. `weather_system`
  is registered unconditionally (`boot.rs:765-779`, `Stage::Early`, exclusive), and
  its only early-out before the `WeatherDataRes` branch is the `GameTimeRes` guard
  at `weather.rs:436` — but `setup_scene` calls `world_setup::ensure_game_time(world)`
  at its very top, *for every scene kind*, before any `--cornell` branch
  (`scene.rs:662-665`). So on `--cornell-sun` the clock guard passes, `WeatherDataRes`
  is absent, and control reaches:

  ```rust
  let Some(wd) = world.try_resource::<WeatherDataRes>() else {
      if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
          apply_neutral_exterior_fallback(&mut cell_lit);
      }
      return;
  };
  ```

  `apply_neutral_exterior_fallback` skips only *interior* cells
  (`weather.rs:279-281`), and `--cornell-sun` installs
  `procedural_fallback_cell_lighting(sun_dir())` with `is_interior: false`
  (`env_translate.rs:1264`, `cornell.rs:1403`). It therefore fires and does
  `*cell_lit = procedural_fallback_cell_lighting(compute_sun_arc(6.0, DEFAULT_TOD_HOURS).0)`
  — replacing the harness's authored `SUN_DIR_RAW` direction with a hardcoded
  hour-6.0 sun.
- **Evidence**:
  - `cornell.rs:285` — `const SUN_DIR_RAW: Vec3 = Vec3::new(0.6, 0.84, 0.4);`
    → `sun_dir()` = `[0.530, 0.742, 0.353]` (≈48° elevation).
  - `weather.rs:282` — `let (sun_dir, _intensity) = compute_sun_arc(6.0, DEFAULT_TOD_HOURS);`
    `DEFAULT_TOD_HOURS = FB_TOD_HOURS = [6.0, 10.0, 18.0, 22.0]` (`env_translate.rs:1254`).
    In `compute_sun_arc` (`weather.rs:121-138`), `hour == sunrise_begin` ⇒
    `solar_hour = 0` ⇒ `angle = 0` ⇒ `[cos 0, sin 0, SUN_SOUTH_TILT] = [1, 0, 0.15]`
    normalised = `[0.989, 0.0, 0.148]` — a horizon-grazing due-east sun.
  - `weather_system` `return`s at that branch **before** the `SkyParamsRes` write
    block (`weather.rs:707-722`), so `SkyParamsRes.sun_direction` keeps `sun_dir()`
    while `CellLightingRes.directional_dir` becomes the hour-6 vector.
  - The existing pin `cornell.rs::sun_variant_drives_directional_and_sky_paths`
    (`cornell.rs:2133-2166`) asserts exactly the invariant this breaks
    (*"SkyParamsRes and CellLightingRes must carry the same direction"*), but calls
    `install_cornell_lighting` directly and never runs the scheduler — it pins the
    install-time state only, so the frame-1 clobber is invisible to it.
- **Impact**: The `--cornell-sun` RT oracle — the harness whose stated purpose is
  that *"the sun is then the only light in the scene, so any sign flip / axis swap /
  dropped term in the directional chain shows up as a moved or missing shadow rather
  than a plausible-looking image"* — renders with the shading directional and the
  painted sun disc pointing ~48° apart, from frame 1 onward. Every shadow-direction
  and sun-axis conclusion drawn from that harness is measured against the wrong
  reference. It also silently substitutes the sunrise intensity ramp's geometry for
  the mid-sky vector the probe set was laid out for.
- **Suggested Fix**: Either (a) have `apply_neutral_exterior_fallback` preserve the
  installed `directional_dir` instead of rebuilding the whole `CellLightingRes` from
  a hardcoded `hour = 6.0` (it already receives `&mut CellLightingRes`; the hardcoded
  hour is also inconsistent with the live `GameTimeRes` hour this same function has
  in scope at the call site), or (b) have `install_cornell_lighting(world, true)`
  install a `WeatherDataRes` — the harness's own doc says the intent is for
  `weather_system` to be inert, and its absence is what makes it *not* inert. Extend
  `sun_variant_drives_directional_and_sky_paths` to run `weather_system(&world, 0.0)`
  before its assertions so the pin covers the live path.

---

#### REN-2026-08-30-D19-01: `#3530` sets `PARALLAX_ALPHA_HEIGHT_BIT` without the `normal_has_alpha` gate its sibling mechanism uses — an alpha-less normal map yields a constant height of 1.0 and the marcher walks the FULL parallax slide


- **Severity**: HIGH
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-285` (`APPLY_HILIGHT2` route), `byroredux/src/render/static_meshes.rs:306-311` (bit transport), `crates/renderer/shaders/include/material_sampling.glsl` (`parallaxDisplaceUV`)
- **Status**: NEW
- **Description**: The `APPLY_HILIGHT2` route binds the **normal** map into the height
  slot and sets `parallax_height_in_alpha` on the sole conditions
  `tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none()` and
  `info.normal_map.is_some()`. Nothing checks whether that normal texture actually
  *has* an alpha channel. Its own sibling mechanism — `NORMAL_ALPHA_SPEC_BIT`, which
  the `#3530` comments cite as the pattern being reused "verbatim" — is gated on
  exactly that signal (`normal_alpha_spec_binding_applies(mat, normal_has_alpha, …)`,
  `material_translate.rs:795-813`; the value comes from
  `texture_registry.handle_has_alpha` → `dds::format_has_alpha`,
  `scene/nif_loader.rs:1100-1103`). The parallax half at
  `static_meshes.rs:306-311` reads `normal_has_alpha` into scope two dozen lines
  earlier (`:291-293`) and does not consult it.
- **Evidence**:
  - `dds::format_has_alpha` (`crates/renderer/src/vulkan/dds.rs:126-140`) returns
    `false` for every BC1/BC4/BC5 variant. DXT1 is decoded as
    `BC1_RGBA_SRGB_BLOCK` (`dds.rs:575`) — 1-bit punch-through, `A == 1.0` on every
    opaque 4-colour block; `ATI2`/BC5 maps to `BC5_UNORM_BLOCK` (`dds.rs:578`), for
    which the sampler returns `A = 1.0` by format.
  - Trace the constant through the raster marcher (`material_sampling.glsl`,
    `parallaxDisplaceUV`): with `sampledHeight == 1.0` the loop guard
    `if (currentDepth >= sampledHeight) break;` never fires, so it runs all `steps`
    iterations and exits with `currentUV = uv - planarSlide`, `currentDepth = 1.0`.
    The secant step then computes `afterDepth = 1.0 - 1.0 = 0.0`,
    `beforeDepth = 1.0 - (1.0 - layerDepth) = layerDepth`,
    `weight = 0 / (0 - layerDepth + 1e-6) ≈ 0`, so
    `mix(currentUV, prevUV, 0) == currentUV`. The returned UV is displaced by the
    **entire** `planarSlide` at every fragment.
  - `planarSlide = V_ts.xy / max(V_ts.z, 0.05) * heightScale` with the
    importer-installed `heightScale = 0.04` (`legacy_properties.rs:281-283`): at
    grazing incidence this reaches ≈0.8 UV units of slide, view-dependent per frame.
  - `sampleUV` is the single UV feeding every subsequent fetch — base, normal, detail,
    glow, gloss, dark, the eight terrain splat layers (`triangle.frag:231-241` and
    downstream), so the whole material slides, not just the height read.
  - The `#3530` route is not niche: its own comment records *"1,433 properties across
    741 distinct vanilla meshes carry it"* (`legacy_properties.rs:256-258`).
- **Impact**: On every Oblivion `APPLY_HILIGHT2` mesh whose normal map lacks a real
  alpha channel, the entire texture set swims with view angle at maximum parallax
  amplitude — the opposite of the intended "no-op when there is no height data".
  The mixed-block BC1 case is worse than either extreme: 3-colour blocks decode
  `A = 0` (instant break, no displacement) while 4-colour blocks decode `A = 1`
  (full displacement), so the surface tears along block boundaries. Both POM marchers
  inherit it identically, so reflections agree with the raster pass — on the wrong
  image.
- **Suggested Fix**: Gate the bit on the same signal `NORMAL_ALPHA_SPEC_BIT` uses.
  The cheapest correct placement is `static_meshes.rs:306-311`, where
  `normal_has_alpha` is already in scope:
  `if parallax_map_index != 0 && normal_has_alpha && mat.is_some_and(|m| m.parallax_height_in_alpha)`.
  Note the canonical-state purist reading argues for resolving it at the NIFAL
  boundary instead — but the DDS format is not known there, which is precisely why
  `normal_has_alpha` is a render-side `MaterialTextureHandles` field and not a
  `Material` field. Add a pin next to
  `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader`.
- **Cross-dimension corroboration**: Independently found a second time as *D2-01* by the SSBO/ray-query dimension, which rated it MEDIUM on reachability grounds and stated the caveat explicitly. Severity arbitrated **up** to HIGH here per the project rubric's *"severity is about IMPACT, not likelihood"* rule: the mechanism is certainly wrong and the failure is maximal-amplitude rather than graceful. The affected population is **uncensused** — Oblivion `_n.dds` are commonly DXT3/DXT5 (which do carry alpha); the BC1/BC5/single-channel subset within the 1,433 `APPLY_HILIGHT2` properties is the exposed set and no Oblivion texture archive was mounted in this session to measure it. Census first, then fix.

---


### MEDIUM

#### REN-2026-08-30-D3-01: the `DBG_*` u32 flag mask is fully exhausted — all 32 bits are allocated and no test guards uniqueness or headroom


- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants_data.rs` (`DBG_BITS`, `DBG_RESERVED_20`, `DBG_RESERVED_200`), `crates/renderer/src/shader_constants.rs` (`dbg_bits_catalog_covers_every_dbg_constant`)
- **Status**: New
- **Description**: The `DBG_*` debug-visualisation bitfield carried in
  `GpuCamera.render_debug.x` has consumed every bit of its `u32`. There is no free
  bit left to allocate, and the one census guard that exists cannot detect the
  failure mode that exhaustion creates — a new `DBG_*` constant that *aliases* an
  already-assigned bit.
- **Evidence**: Machine-counted over the live source, not quoted:
  ```
  single-bit DBG_* constants: 32
  union mask:                 0xffffffff
  free bits remaining:        0
  reserved placeholders:      ['DBG_RESERVED_20', 'DBG_RESERVED_200']
  ```
  `DBG_BYPASS_POM = 0x1` … `DBG_VIZ_SELECTED_LIGHT = 0x80000000` (bit 31) covers
  bits 0-31 with no gaps. The `DBG_BITS` catalog holds **35** entries — the 32
  single bits plus 3 compound unions (`DBG_VIZ_MATERIAL_LOBES`, `DBG_VIZ_RT_LOD`,
  `DBG_VIZ_SHADOW_VISIBILITY`). Only two slots are recyclable: `DBG_RESERVED_20`
  (bit 5) and `DBG_RESERVED_200` (bit 9).

  The sole census guard, `dbg_bits_catalog_covers_every_dbg_constant`
  (`shader_constants.rs:86`), compares `DBG_BITS.len()` against a **text count** of
  `pub const DBG_…: u32 =` lines in the data file. It asserts nothing about the
  *values*. A 33rd bit — which on a full `u32` can only be written as a duplicate of
  an existing value — gets a catalog entry, passes this test, passes
  `generated_header_contains_all_defines`, passes `triangle_frag_dbg_bits_not_redeclared`,
  and ships as two debug views silently firing each other.

  The codebase already has the exact guard this needs, applied to the *other*
  flag field: `instance_flag_bits_unique_and_outside_packed_windows`
  (`crates/renderer/src/vulkan/scene_buffer/constants.rs:454`) asserts
  `a.count_ones() == 1` per flag and `a & b == 0` pairwise. `INSTANCE_FLAG_*` — which
  has bits 4, 5 and 9-15 still free — is defended; `DBG_*`, which has none, is not.
- **Impact**: Adding any new debug view is now impossible without either silently
  aliasing an existing bit (undetected by the whole test suite) or knowing to
  recycle one of the two `DBG_RESERVED_*` slots — a fact recorded nowhere. Debug-path
  only, so no shipping-frame corruption, but the next person to add a debug view is
  set up to produce a confusing, test-green miscompare during exactly the kind of
  investigation debug views exist to serve.
- **Suggested Fix**: (1) Add `dbg_bits_are_single_bit_and_pairwise_disjoint`,
  modelled on `instance_flag_bits_unique_and_outside_packed_windows`, walking
  `DBG_BITS` and skipping the three known compound unions by name; assert the union
  of the single bits equals `u32::MAX` *and* emit the count of free bits so the
  exhaustion is visible in test output. (2) Document the two `DBG_RESERVED_*` slots
  as the allocation pool in the `DBG_BITS` doc comment. (3) For real expansion,
  `GpuCamera.render_debug` is a `uvec4` whose `.w` lane is unused — shaders read only
  `.x` (mode), `.y` (`rtLodScale`, itself a float smuggled through a uint lane via
  `uintBitsToFloat`) and `.z` (`rtLodTelemetryEnabled`) in `triangle.frag:141/789/793`.
  A second flag word costs zero bytes.

---

---

#### REN-2026-08-30-D3-02: every GLSL-mirror lockstep guard drives off a hardcoded `SOURCES` list, so a newly-added mirror site is silently unguarded


- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_light_glsl_copies_stay_in_lockstep`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`, `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`), `crates/renderer/src/vulkan/reflect.rs` (`camera_ubo_size_matches_gpu_camera_in_every_shader`)
- **Status**: New
- **Description**: Hand-mirrored GLSL struct declarations are the documented #1
  silent-desync source in this codebase, and the guards built for them
  (#1916, #2748) work well — but each one hardcodes the *set of files it checks* as a
  `const SOURCES: &[(&str, &str)]` of `include_str!` literals. Nothing scans
  `crates/renderer/shaders/` for declarations. A new shader that declares
  `struct GpuInstance` (or `GpuLight`, or a `CameraUBO` block) is therefore born
  completely outside the lockstep contract, and every existing test stays green.
- **Evidence**: `gpu_instance_glsl_copies_stay_in_lockstep`
  (`shader_contract_tests.rs:1751`) hardcodes 5 paths;
  `gpu_light_glsl_copies_stay_in_lockstep` (`:1682`) hardcodes 4;
  `camera_ubo_size_matches_gpu_camera_in_every_shader` (`reflect.rs:606`) hardcodes 6
  `.spv` and carries the tell in its own comment:

  > `// Every shader that declares the `CameraUBO` block. Add new readers here so they are pinned too.`

  The guard's correctness is delegated to a code-review convention, which is precisely
  what the guard was introduced to stop relying on.

  **The lists are currently complete** — verified independently, so this is latent,
  not a live desync:
  * `struct GpuInstance` → 5 real declarations (the 6th `grep -rl` hit,
    `skin_vertices.comp`, is a comment).
  * `struct GpuLight` → 4 declarations, matching the 4 hardcoded.
  * `CameraUBO` → exactly 6 committed `.spv` contain the block
    (`strings -a *.spv | grep '^CameraUBO$'`), matching the 6 hardcoded.

  The near-miss is already on record: `skin_vertices.comp` reads
  `morph_delta_address` / `morph_weight_address` / `vertex_count` — three fields that
  mirror `GpuInstance` semantics — through a hand-written `layout(push_constant)`
  block, and its own source comment (`skin_vertices.comp:82-84`) states the parity is
  *"not covered by an automated parity test, since this shader has no
  `struct GpuInstance` for the existing GpuInstance-lockstep tests to anchor on."*
  (That particular block does currently match `SkinPushConstants` field-for-field and
  is size-pinned at 32 B, so it is correct today — but it is correct unguarded.)
- **Impact**: The strongest structural guarantee in the renderer's GPU-contract
  test suite has a discovery hole. The failure is silent by construction: a 6th
  `GpuInstance` mirror with a dropped or reordered field produces garbage
  transforms / texture indices / morph addresses for whatever pass reads it, with a
  fully green `cargo test`. Given `GpuInstance` grew twice in recent history
  (#2219 128 B, #3231 160 B) and each growth touched every mirror, the probability of
  a new consumer appearing is non-trivial.
- **Suggested Fix**: Replace the hardcoded lists with discovery. Tests can read the
  filesystem (`CARGO_MANIFEST_DIR` is set): walk `crates/renderer/shaders/**/*.{vert,frag,comp,glsl}`,
  collect every file containing `struct GpuInstance` (excluding comment-only hits —
  the existing `extract_struct_body` helper already distinguishes them), and assert
  the discovered set **equals** the expected set before running the field compare.
  That converts "someone forgot to add the file" from silent to a named test
  failure. Apply the same to `GpuLight`. For `camera_ubo_size_…`, iterate every
  committed `.spv` and pin any that reflects a `CameraUBO` block, rather than
  enumerating six by hand.

---

---

#### REN-2026-08-30-D5-01: three Scene-Buffers rows in `memory-budget.md` contradict test-pinned constants, `MAX_LIGHTS` by 2×

- **Severity**: Medium
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md:30,35,37` vs
  `crates/renderer/src/shader_constants_data.rs:41-49` (`MAX_LIGHTS`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9-18` (`GpuTerrainTile`),
  `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:67-79` (`GpuCamera`)
- **Status**: Open — the **doc** is wrong on all three; every code value is
  deliberate and compile- or test-pinned.
- **Description**: The ledger's Scene Buffers table is the page other sections
  and the VRAM roll-up derive from. Three of its eight rows no longer match code:
  1. `| Light SSBO | MAX_LIGHTS = 512 | 512 | 64 B | 32 KB | 64 KB |`. Commit
     `8e7582ed` (2026-08-16) redefined `MAX_LIGHTS` as
     `RESERVOIR_LIGHT_MASK as usize` = `(1 << 10) - 1` = **1023**, with
     `const _: () = { assert!(MAX_LIGHTS == 1023); }` beside it. Real footprint
     is 1023 × 64 B ≈ 64 KB/frame, **128 KB** across 2 FIF. The doc's `512` looks
     like it was transcribed from the neighbouring `MAX_LIGHTS_PER_CLUSTER = 512`.
  2. `| Terrain tile SSBO | … | 32 B | — | 32 KB |`. `GpuTerrainTile` is three
     `[u32; 8]` members = **96 B**, pinned by `gpu_terrain_tile_is_96_bytes` with
     the shader's `ArrayStride 96`, and `buffers.rs:480` sizes the buffer from
     `size_of::<GpuTerrainTile>()`. Real total ≈ **96 KB**, 3× the documented row.
  3. `| Camera UBO | — | 1 | 352 B | 352 B | 704 B |`. `GpuCamera` has been
     **368 B** since `#3323` added `exterior_sky_tint`, pinned by
     `gpu_camera_is_368_bytes`.
- **Evidence**:
  - `shader_constants_data.rs:41-47`: `RESERVOIR_LIGHT_BITS: u32 = 10;` …
    `pub const MAX_LIGHTS: usize = RESERVOIR_LIGHT_MASK as usize;` …
    `assert!(MAX_LIGHTS == 1023);`
  - `scene_buffer/constants.rs:15`: `pub(super) const MAX_LIGHTS: usize = crate::shader_constants::MAX_LIGHTS;`
  - `git log -S RESERVOIR_LIGHT_BITS` → `8e7582ed`, 2026-08-16; the doc row was
    last touched by `78540d8e`, 2026-06-02.
  - `gpu_instance_layout_tests.rs:300-307` / `:67-79` for the two struct sizes.
- **Impact**: Anyone budgeting against this page under-counts light-SSBO and
  terrain-tile residency by 2× and 3×. More consequentially, `MAX_LIGHTS` is the
  documented *overflow ceiling* — the number a reader uses to reason about the
  light-clamp path — and the page states half the real value under a constant name
  that resolves to something else in the same file.
- **Suggested Fix**: Update the three rows (1023 / 64 KB / 128 KB; 96 B / 96 KB;
  368 B / 736 B) and the "Total resident scene buffers" line. Prefer wording that
  names `MAX_LIGHTS = RESERVOIR_LIGHT_MASK` so the derivation, not a literal,
  is what the page records.
- **Dedup note**: NOT #3447 — that issue names `shader-pipeline.md` for the
  `GpuCamera` 352 B claim and `memory-budget.md` only for the *Instance SSBO*
  25 % understatement. The three rows above are separate sites and separate
  numbers; the `GpuCamera` row here is `memory-budget.md:37`, not
  `shader-pipeline.md`.

---

---

#### REN-2026-08-30-D5-02: the mesh-side `StagingPool` — a second 128 MB retained `CpuToGpu` pool — has no ledger row, and #3298's 64 MiB chunking made its retention permanent

- **Severity**: Medium
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs:403` (`geometry_staging_pool`),
  `crates/renderer/src/vulkan/buffer.rs:53` (`DEFAULT_STAGING_BUDGET_BYTES`),
  `crates/renderer/src/mesh.rs:55` (`GEOMETRY_REBUILD_CHUNK_BYTES`);
  `docs/engine/memory-budget.md:422` + the VRAM roll-up at `:495-515`
- **Status**: Open
- **Description**: `memory-budget.md` mentions a staging pool exactly once — a
  `Staging pool cap | 128 MB` row inside the **Texture Registry** section — and the
  VRAM Rough Budget table has no staging line at all. There are two live pools:
  `TextureRegistry::staging_pool` (`texture_registry.rs:526`) and
  `MeshRegistry::geometry_staging_pool` (`mesh.rs:919, 1195, 1509`). Both are
  built with `StagingPool::new`, i.e. `DEFAULT_STAGING_BUDGET_BYTES` = 128 MB of
  **retained** `MemoryLocation::CpuToGpu` capacity each, and there is no
  production `trim_to(0)` caller anywhere in the workspace — the only shrink is
  `release`'s auto-trim back to budget.
  #3298 changed the mesh pool's steady state rather than its cap. The pre-#3298
  atomic path staged the *whole* vertex buffer in one `acquire`, so a large scene
  released a single 600 MiB entry that immediately blew the budget and was
  evicted largest-first, leaving the pool near empty. The chunked path stages
  `GEOMETRY_REBUILD_CHUNK_BYTES` = 64 MiB at a time, so after any chunked rebuild
  the pool holds one ~64 MiB vertex-chunk entry plus one 64 MiB index-chunk entry
  — 134,217,672 B against a 134,217,728 B budget, i.e. exactly at the ceiling and
  therefore never trimmed. That is up to ~128 MB of resident host-visible memory
  (VRAM on a ReBAR-enabled 4070 Ti) that the ledger does not carry for a subsystem
  the ledger does describe.
- **Evidence**:
  - `buffer.rs:53`: `pub const DEFAULT_STAGING_BUDGET_BYTES: vk::DeviceSize = 128 * 1024 * 1024;`
  - `buffer.rs:159-165`: `acquire` allocates `MemoryLocation::CpuToGpu`.
  - `buffer.rs:205-215`: `release` auto-trims only when
    `total_capacity() > budget_bytes`.
  - `grep -rn "trim_to" crates/renderer/src` → only `buffer.rs` internals
    (`:214`, `:254`, `:259` in `destroy`); no production caller.
  - `mesh.rs:1512-1513`: `GEOMETRY_REBUILD_CHUNK_BYTES / size_of::<Vertex>()` and
    `/ size_of::<u32>()` — the two chunk sizes released back into the pool at
    `buffer.rs:1577` (`staging.release_to(staging_pool, capacity)`).
- **Impact**: A per-session ~128 MB residency that no budget row accounts for, on
  top of the two-generation geometry peak. It is not a leak (bounded, freed at
  `destroy_all`), but the "Estimated total ~1.74 GB / ~3.4 GB at native 4K"
  roll-up is computed without it, and the page is the stated authority for the
  `< 4 GB` target.
- **Suggested Fix**: Add a Staging Pools section (or a row in the VRAM table)
  naming both pools, `DEFAULT_STAGING_BUDGET_BYTES`, and the
  `2 × GEOMETRY_REBUILD_CHUNK_BYTES` floor the chunked rebuild now parks in the
  mesh pool. If the 128 MB retention is not wanted on the geometry side,
  `StagingPool::with_budget` already exists — but that is a policy call, not a
  doc fix, and should be measured first.
- **Dedup note**: NOT #3463 — that issue is about the vertex/index *pool* row not
  carrying #3298's two-generation device-local peak. This is the host-visible
  staging side, a different allocation class and a different (missing) row.

---

---

#### REN-2026-08-30-D6-01: the Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as BOTH parallax height and the normal-alpha-as-spec mask — the render-side predicate never consults `Material::parallax_height_in_alpha`


- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`normal_alpha_spec_binding_applies`, `normal_alpha_spec_applies`), `byroredux/src/render/static_meshes.rs` (`build_static_mesh_draws`, lines ~306-312 and ~474-484), `crates/nif/src/import/material/legacy_properties.rs` (the `APPLY_HILIGHT2` arm)
- **Status**: OPEN — new (the `parallax_height_in_alpha` field landed in `19813460`, after the 2026-08-27 sweep)
- **Description**: #3530 resolved a per-game channel-meaning decision at the NIFAL boundary: `Material::parallax_height_in_alpha` records that this material's height values live in the bound texture's **alpha**, because Oblivion ships no `_p.dds` and `legacy_properties.rs` therefore binds the *normal* map into `MaterialTextureSet::height`. The render path transports that as `PARALLAX_ALPHA_HEIGHT_BIT` on `parallax_map_index`.

  Fifty lines further down in the same loop, `normal_alpha_spec_binding_applies` makes an *independent* claim about the same channel of the same texture — that the normal map's alpha is a per-pixel **specular-intensity mask** — and re-points the gloss slot at the normal map with `NORMAL_ALPHA_SPEC_BIT`. It reads `material_kind`, `normal_has_alpha`, `normal_map_index` and `gloss_map_index`; it does **not** read `parallax_height_in_alpha`. The two are not mutually excluded anywhere.

  For an `APPLY_HILIGHT2` mesh the preconditions of the second predicate are satisfied by construction: `normal_has_alpha` must be true (that alpha *is* the height payload), `normal_map_index != 0` (the parallax slot was bound from it), and `material_kind < 100` for ordinary Oblivion architecture. Only a bound `NiTexturingProperty.gloss_texture` (`gloss_map_index != 0`) suppresses it.
- **Evidence**:
  - `legacy_properties.rs`: `if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() { … info.parallax_map = Some(normal); info.parallax_height_in_alpha = true; }`
  - `crates/nif/src/import/material/mod.rs:1249` — `height: self.parallax_map`, so `textures.height` and `textures.normal` resolve to the *same* path and therefore the same bindless handle.
  - `static_meshes.rs`: `if parallax_map_index != 0 && mat.is_some_and(|m| m.parallax_height_in_alpha) { parallax_map_index |= PARALLAX_ALPHA_HEIGHT_BIT; }`
  - `static_meshes.rs`: `if normal_alpha_spec_binding_applies(mat, normal_has_alpha, material_kind, metalness, normal_map_index, gloss_map_index) { gloss_map_index = normal_map_index | NORMAL_ALPHA_SPEC_BIT; }` — no `parallax_height_in_alpha` term.
  - `normal_alpha_spec_applies` body is exactly `material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0`.
  - Both consumers then read the same texel: `material_sampling.glsl::sampleParallaxHeight` returns `texel.a`, and `triangle.frag:1247-1255` does `normalAlphaSpecMask = glossTexel.a; specStrength *= normalAlphaSpecMask;`.
  - `normal_has_alpha` originates from `dds::format_has_alpha` on the bound normal (`scene/nif_loader.rs:1101`), so it is true precisely for the population that carries height data.
- **Impact**: On the `APPLY_HILIGHT2` population (the commit message cites 1,433 properties across 741 vanilla Oblivion meshes) the specular strength is multiplied by the **height field**: crevices go matte and raised brickwork goes glossy, with the modulation tracking displacement rather than any authored spec mask. Symmetrically, the engine now asserts two mutually exclusive meanings for one channel in one draw with nothing arbitrating — which is the exact class of render-time channel-meaning re-derivation NIFAL exists to eliminate, reintroduced one predicate away from the field that was added to prevent it. Confined to Oblivion; every other producer leaves `parallax_height_in_alpha` false and is unaffected.
- **Suggested Fix**: Make the two exclusive at the canonical boundary rather than in the draw loop. Thread `parallax_height_in_alpha` into `normal_alpha_spec_applies` (or add it to `normal_alpha_spec_binding_applies`'s inputs, which already takes `Option<&Material>`) and return `false` when it is set, so a material whose normal alpha was already claimed as height cannot also claim it as a spec mask. Pin it with a test alongside the existing `normal_alpha_spec_binding_applies` cases in `material_translate.rs:1743-1770`. Before landing, census `NiTexturingProperty.gloss_texture` fill on the `APPLY_HILIGHT2` meshes to confirm the suppressing `gloss_map_index != 0` arm is as rare as it appears (the fix is correct either way; the census only sizes the affected population).

---
- **Cross-dimension corroboration**: Found independently three times — also filed as *D2-02* (SSBO/indexing) and *D19-02* (tangent-space). All three traced the same two predicates and reached the same conclusion; the write-up below is the NIFAL-dimension one, which carries the corpus figure.

---

#### REN-2026-08-30-D7-01: no guard asserts `hash_gpu_material_fields` covers every `GpuMaterial` field — the three existing pins are mutually blind to a field omitted from both hash walks


- **Severity**: MEDIUM
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs` (`hash_gpu_material_fields`), `crates/renderer/src/vulkan/context/mod.rs` (`DrawCommand::material_hash`, `material_hash_matches_gpu_material_field_hash`)
- **Status**: OPEN (missing regression guard; today's coverage verified complete)
- **Description**: `MaterialTable::intern_by_hash` keys its `FxHashMap<u64, u32>` dedup index solely on the u64 returned by `hash_gpu_material_fields` / `DrawCommand::material_hash`. A `GpuMaterial` field that is populated by `to_gpu_material` but omitted from **both** hash walks makes two visually-different materials collapse onto one table slot — the first-seen record wins and every later draw renders with the wrong value. Nothing in the test suite can fail on that. The three pins that look like they cover it do not:
  - `gpu_material_size_is_432_bytes` (`material.rs:1494`) pins `size_of`, which a correctly-added-but-unhashed field still satisfies (the author bumps 432 → 436).
  - `gpu_material_glsl_field_order_matches_rust_struct` (`scene_buffer/shader_contract_tests.rs:1383`) compares the Rust struct against `include/bindings.glsl` — both sides get updated in a normal field addition.
  - `material_hash_matches_gpu_material_field_hash` (`context/mod.rs:2638`) compares the two hash walks **against each other**; it passes when a field is missing from both.

  The only live net is the `#[cfg(debug_assertions)]` byte-equality `debug_assert!` inside `intern_by_hash` (`material.rs:1344`), which is runtime-only, debug-only, and fires only if content that differs in the unhashed field is actually loaded that session. Release builds mis-render silently. The struct has grown 272 → 260 → 296 → 300 → 348 → 364 → 396 → 432 B across ~8 separate additions (size history on `GpuMaterial`, `material.rs:40`), so this is a recurring edit path, not a hypothetical one.
- **Evidence**: Mechanical diff of the struct's declared field names against the `mat.<field>` identifiers in the `hash_gpu_material_fields` body: 108 fields declared (108 × 4 B = 432 B, no pad fields), 108 hashed, symmetric difference empty — coverage is complete **today**. `DrawCommand::material_hash` reaches the same 108 via 97 literal `write_u32` calls plus the `for texture_index in &self.supplemental_texture_indices[..12]` loop. `grep -rn "hash_gpu_material_fields"` across `crates/renderer/src` returns no test that enumerates the struct's fields; the only field-specific hash tests are the two single-field pins `material_alpha_participates_in_the_dedup_hash` (`material.rs:1448`) and `greyscale_lut_index_difference_is_distinct` (`material.rs:2063`). `cargo test -p byroredux-renderer --lib material` → 52 passed, 0 failed.
- **Impact**: A future `GpuMaterial` field addition that misses both hash walks silently merges distinct materials in release builds — the failure presents as "some objects render with a neighbour's material", with no log line, no assert, and no failing test. Real interior cells intern 50–200 unique materials and a Skyrim radius-3 grid 4000+, so the collapsed pair is highly likely to be visible.
- **Suggested Fix**: Add a source-scanning test next to `gpu_material_size_is_432_bytes`. The machinery already exists and is already applied to this exact file: `shader_contract_tests.rs:1384` does `include_str!("../material.rs")` and `parse_rust_struct_fields(rust_src, "pub struct GpuMaterial")`; `gpu_instance_layout_tests.rs:180` uses the same helper as a ban-list guard. Parse the struct's field names, extract the `mat.<ident>` identifiers from the `hash_gpu_material_fields` body out of the same `include_str!` source, and assert set equality in both directions (a field in the struct but not the hash = silent dedup collapse; a stale identifier in the hash but not the struct = the walk drifted). Both assertion messages should name the field.

---

---

#### REN-2026-08-30-D9-01: a failed first-sight `bind_inverses` upload is swallowed with no requeue — the slot's palette source stays UNDEFINED for the entity's whole residency


- **Severity**: MEDIUM
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2504-2513` (`draw_frame`, the `upload_pending_bind_inverses` call), interacting with `byroredux/src/app_frame.rs:569-573` (`render_one_frame`, the `#1791` requeue) and `crates/renderer/src/vulkan/scene_buffer/buffers.rs:602` (`bind_inverses_persistent`)
- **Status**: OPEN — new
- **Description**: `SkinSlotPool::drain_pending` removes first-sight `(slot, entity)` entries from the pool *irrevocably* before `draw_frame` is called. #1791 / D6-01 built exactly one recovery path for that: if `draw_frame` bails before the skin section, `ctx.skin_dispatch_ran` stays `false` and `render_one_frame` calls `requeue_pending`. That flag does not cover the case where `draw_frame` *reaches* the upload and the upload itself fails. `upload_pending_bind_inverses` returns `Result`, and the call site collapses `Err` to `0` with a `log::warn!`. `pending_capped == 0` then skips `record_pending_bind_inverse_copies` entirely, and `record_skinned_blas_refit` (which sets `skin_dispatch_ran = true`, `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:63`) still runs afterwards — so the requeue branch never fires and the drained entries are dropped on the floor.
- **Evidence**:
  - `draw.rs:2504-2513`:
    ```rust
    let pending_capped = if !bind_inverse_pending_uploads.is_empty() {
        self.scene_buffers
            .upload_pending_bind_inverses(&self.device, bind_inverse_pending_uploads)
            .unwrap_or_else(|e| {
                log::warn!("Failed to upload pending bind_inverses: {e}");
                0
            })
    } else { 0 };
    ```
    `upload_pending_bind_inverses` (`scene_buffer/upload.rs:329-360`) has two fallible steps: `staging.mapped_slice_mut()?` and `staging.flush_if_needed(device)?`.
  - `skinned_blas_refit.rs:63` — `self.skin_dispatch_ran = true;` is set unconditionally at the top of `record_skinned_blas_refit`, which `draw.rs` calls at line 2597, i.e. after the upload block.
  - `app_frame.rs:569-573` — `if !ctx.skin_dispatch_ran { … requeue_pending(…) }` is the only requeue site in the tree (`grep -rn requeue_pending`).
  - `crates/core/src/ecs/resources/skin_slot_pool.rs:163-166` — `allocate` returns early for an entity already in `entity_to_slot`, so a resident entity never re-enters `pending_uploads` on its own.
  - `buffers.rs:602` — `bind_inverses_persistent` is `GpuBuffer::create_device_local_uninit(...)`: never cleared, so an unwritten slot region holds undefined bytes, not zeros.
  - `skin_palette.comp:77` — `palette[slot] = boneWorld[slot] * bindInverses[slot];` — the undefined matrix is multiplied into the palette every frame.
- **Impact**: One dropped first-sight upload permanently corrupts that entity's bone palette for its remaining lifetime in the cell — the palette feeds both `skin_vertices.comp` (so the skinned BLAS is refit against garbage world positions, dragging the TLAS AABB with it) and `triangle.vert`'s inline raster skinning. Symptom is an exploded/vanished actor plus an inflated RT cost, with only a single WARN as evidence. Trigger (host-visible map / flush failure) is rare, which is precisely why the silent-and-permanent shape matters: there is no self-healing frame. This is the same defect class #1791 was filed for, reached through the sibling branch that fix did not cover.
- **Suggested Fix**: Latch the failure on the context (e.g. `self.bind_inverse_upload_failed = true` in the `unwrap_or_else`, reset alongside `skin_dispatch_ran` at `draw.rs:1567`) and widen `app_frame.rs:569` to `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed`. The entries then reappear on the next `drain_pending` and the persistent SSBO region is written a frame later. Pin it with a source-position test in the style of `skin_dispatch_ran_rollback_scope_tests` (`app_frame.rs:692`) / `skin_dispatch_ran_ordering_tests` (`draw.rs:4577`). Optionally also zero-init `bind_inverses_persistent` so a missed write degrades to a collapsed-to-origin mesh rather than undefined memory — that is a defence-in-depth change, not the fix.

---

---

#### REN-2026-08-30-D10-01: the depth-capture readback hardcodes `D32_SFLOAT` while `find_depth_format` can select `D16_UNORM`, and `VulkanContext::depth_format` is never consulted


- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_record_copy` L135-136, `depth_capture_finish_readback` L49/L94-98); `crates/renderer/src/vulkan/context/helpers.rs` (`find_depth_format` L26); `crates/renderer/src/vulkan/context/mod.rs` (`depth_format` field, L1875)
- **Status**: New
- **Description**: The capture path assumes 4 bytes per depth sample in both
  halves — `buffer_size = width * height * 4 /* D32_SFLOAT */` when sizing the
  staging buffer, and `slice.chunks_exact(4).map(f32::from_le_bytes)` when
  decoding it. `find_depth_format` is a *fallback chain*:
  `let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];` — it
  returns whichever the physical device reports first with
  `DEPTH_STENCIL_ATTACHMENT` optimal-tiling support. Vulkan mandates
  `D16_UNORM` support for depth attachments but does **not** mandate
  `D32_SFLOAT` (only that one of `D32_SFLOAT` / `X8_D24_UNORM_PACK32` be
  supported), so the D16 arm is genuinely reachable. The selected format is
  already stored on the context as `self.depth_format` and `depth_capture.rs`
  never reads it.
- **Evidence**:
  - `helpers.rs:26` — `let candidates = [vk::Format::D32_SFLOAT, vk::Format::D16_UNORM];`
  - `depth_capture.rs:135-136` — `extent.width as vk::DeviceSize * extent.height as vk::DeviceSize * 4 /* D32_SFLOAT */`
  - `depth_capture.rs:49` — `let expected = width as usize * height as usize * 4;`
  - `depth_capture.rs:94-98` — `// D32_SFLOAT: one f32 per sample …` then `.chunks_exact(4).map(|b| f32::from_le_bytes(...))`
  - `mod.rs:1875` — `depth_format: vk::Format,` (present, unused by this module)
  - No `aspect`/stencil hazard: both candidates are depth-only, so
    `ImageAspectFlags::DEPTH` and the absence of stencil-interleaving handling
    are correct as written — the format *width* is the only wrong assumption.
- **Impact**: On a device that falls back to `D16_UNORM`, the staging buffer is
  allocated at 2× the needed size (harmless) but the readback reinterprets
  pairs of adjacent unorm16 samples as one f32, at half the sample count and
  at the wrong pixel positions. `analyze_depth_field` would then report
  `distinct_codes` / band occupancy that is pure noise, with only a partial
  tell (some garbage bit patterns decode outside `[0,1]` and land in
  `stats.invalid`). Because the whole point of this code is to supply the
  before/after evidence for the #3308 reversed-Z architectural decision, a
  silently-wrong capture is worse than no capture. Zero impact on the dev
  RTX 4070 Ti, which selects `D32_SFLOAT`.
- **Suggested Fix**: Read `self.depth_format` in both halves. Either (a) gate
  the capture with an early `log::warn!` + `return` when the format is not
  `D32_SFLOAT`, so the tool refuses rather than lies, or (b) carry the format
  through `depth_capture_pending_readback` alongside the extent and decode
  `D16_UNORM` as `u16 as f32 / 65535.0`. Option (a) is the smaller change and
  matches the module's existing "diagnostic, single consumer" posture; either
  way the `/* D32_SFLOAT */` comments become an assertion instead of an
  assumption.

---

---

#### REN-2026-08-30-D10-02: the #3308 comparison gate can only be run *before* the conversion — `analyze_depth_field` is hardcoded to the conventional mapping in both its background test and its decode


- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/core/src/ecs/components/camera.rs` (`analyze_depth_field` L317, `linear_distance_from_depth` L277, cleared test at L351); `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)
- **Status**: New
- **Description**: `DEFAULT_RENDER_DISTANCE`'s doc block states the gate's
  contract as *"Run it before the conversion, run it after, and the far
  decades' `distinct_codes` are the before/after evidence — the thing that was
  otherwise unobservable and that made shipping reversed-Z speculative."*
  `depth_capture.rs`'s module doc repeats it (*"after a reversed-Z conversion —
  report the before/after difference"*), as does `commands/depth.rs`. The code
  cannot deliver the "after" half. Three separate sites are hardwired to the
  conventional near→0 / far→1 mapping, and there is no mapping selector on
  `analyze_depth_field`:
  1. the background classifier `if z >= 1.0 { stats.cleared += 1; continue; }`
     (L351) — under reversed-Z the clear value is `0.0`, so *nothing* would be
     classified as background and the frame's entire sky would decode into the
     bands, swamping exactly the far decade the gate reads;
  2. the decode `linear_distance_from_depth` (L277), whose
     `denom = 1.0 - z * (f - n) / f` inverts only the conventional
     `z_ndc(d) = f/(f-n)·(1 − n/d)` — there is no
     `linear_distance_from_depth_reversed` sibling to the
     `depth_resolution_at_reversed` that *was* added;
  3. `DepthBand::analytic_resolution` is always populated from
     `self.depth_resolution_at(mid)` and
     `analytic_resolution_reversed` always from the reversed sibling — after a
     conversion the two columns are swapped relative to reality, so the
     `depth.stats` table's "BU/step (reversed-Z would be)" header is then wrong
     in both columns.
- **Evidence**: read the full body of `analyze_depth_field`
  (`camera.rs:317-390`) — it takes only `&self` and `&[f32]`; `Camera` carries
  no reversed/conventional flag, and grepping `_reversed` in that file yields
  only `depth_resolution_at_reversed` and `DepthBand::analytic_resolution_reversed`
  (both analytic-only). `depth.rs`'s `execute` calls
  `camera.analyze_depth_field(&capture.samples)` with no mapping argument.
- **Impact**: The gate is half a gate. Its stated reason for existing is to
  make the reversed-Z conversion non-speculative by giving it a measurable
  before/after; whoever does that work will find the "after" run reports
  `cleared = 0`, a wildly inflated last-decade sample count, and nonsense
  `BU/step` columns, and will have to fix the analysis in the same change that
  they are trying to validate — precisely the position #3308 is trying to avoid
  being in. This is a design gap in brand-new code, not a live rendering bug.
- **Suggested Fix**: Add a mapping discriminant (a `DepthMapping::{Conventional,
  Reversed}` enum parameter on `analyze_depth_field`, or a `reversed: bool`
  field on `Camera` set by whatever sets the projection) and route all three
  sites through it: cleared test becomes `z >= 1.0` / `z <= 0.0`, decode picks
  between `linear_distance_from_depth` and a new reversed inverse
  `d = n / (z·(1 − n/f) + n/f)`, and the two `analytic_*` columns are labelled
  "current mapping" / "other mapping" rather than fixed. Adding the reversed
  inverse also lets `depth_decode_round_trips_the_projection` cover the
  reversed encode, which today it cannot.

---

---

#### REN-2026-08-30-D13-01: TAA resolves only the pre-composite direct HDR — sky, denoised indirect, volumetrics, caustics and bloom bypass the resolve entirely (FSR, the default, does not)

- **Severity**: MEDIUM
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_post_passes`, lines 264–271: `record_taa_pass` → `record_composite_pass` → `record_bloom_pass` → `record_upscale_pass`); `crates/renderer/shaders/composite.frag` (`has_surface` / `is_sky`, lines 536–537); `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::write_descriptor_sets`, `curr_hdr` ← `hdr_views[f]`)
- **Status**: OPEN — architectural, root cause of the symptom `#2760` patched around from inside the constraint
- **Description**: The TAA resolve is wired to `composite.hdr_image_views[f]` — the **raw** main-render-pass HDR attachment, i.e. direct lighting only — and writes `history[f]`, which `composite` then samples as binding 0 (`hdrTex`, `composite.frag:497`). Everything composite *adds* after that point is never seen by TAA: the analytically-synthesised sky (`compute_sky`), the SVGF-denoised indirect, volumetrics, water caustics, and (since `#2796`) bloom. The FSR path is the mirror image: `record_upscale_pass` takes `scene_color = composite.scene_image(frame)` (`post_passes.rs:967–971`) — the fully composited, post-bloom scene — so FSR 3.1 temporally resolves *all* of it. Since `UpscalerMode::default()` is `Fsr3(Quality)` (`upscaling.rs`), the lower-coverage path is the one selected by `--upscaler taa`.
  The concrete visible consequence is the geometry/sky silhouette. `composite.frag:536` classifies each pixel with a hard binary `depth < 1.0` against the **jittered** depth buffer, downstream of the resolve. Sub-pixel Halton jitter flips which side of a silhouette a pixel centre lands on every frame, so that pixel alternates between "TAA-resolved geometry colour" and "freshly-computed, never-temporally-filtered sky", and no filter in the chain ever averages the two. `#2760` correctly diagnosed the mechanism in `taa.comp`'s own comment ("sky is synthesised later in composite.frag and never exists in this HDR attachment") and softened the *history-acceptance* half of it (`disocclusionFromSky` → `gamma = 0.0` box filter) — but it can only improve the frames on which the pixel is geometry. The frames on which jitter makes the pixel sky bypass `hdrTex` for the sky branch entirely.
- **Evidence**:
  - `post_passes.rs:264` `self.record_taa_pass(cmd, frame);` then `:266` `self.record_composite_pass(cmd, frame);` — TAA strictly precedes composite.
  - `taa.rs::write_descriptor_sets`: `curr_hdr = ... .image_view(hdr_views[f])`, and `resize.rs:968–971` / `init.rs:1287–1338` show `hdr_views` is `composite.hdr_image_views` (the render-pass colour attachment), while composite's binding 0 is rebound to `taa.output_view(i)` at `init.rs:1338` / `resize.rs:1001` / `resize.rs:1372`.
  - `post_passes.rs:967–971`: `let scene_color = self.composite... .scene_image(frame);` handed to `FrameUpscaler::record` — post-composite, and `record_bloom_pass` (`:895–903`) has already written into that same `scene_images[frame]`.
  - `composite.frag:535–537`: `float depth = texelFetch(depthTex, ivec2(gl_FragCoord.xy), 0).r; bool has_surface = depth < 1.0; bool is_sky = !has_surface && ...` — no temporal term, no coverage term.
  - `taa.comp:150–165` (the `#2760` comment block) states the constraint explicitly.
- **Impact**: On `--upscaler taa`, exterior geometry/sky silhouettes crawl every frame with a parked camera — the exact artefact TAA exists to remove — and indirect/volumetric/caustic edges receive no temporal antialiasing at all. Because the default upscaler hides it, the TAA fallback path (FSR context creation failure at startup promotes to `UpscalerMode::Taa`, per `upscaling.rs`'s `Taa` doc / `#2480`) silently delivers materially worse image stability than the mode it replaced, in a way no `cargo test` observes.
- **Suggested Fix**: Move the TAA resolve to the same tap FSR uses — dispatch it on `composite.scene_images[frame]` after `record_composite_pass` (and after or before bloom, matching FSR's tap) instead of on the raw HDR attachment, so one code path feeds both temporal reconstructors the same image. That makes `#2760`'s `disocclusionFromSky` special case unnecessary rather than merely mitigated, since the sky would then be present in the resolved image on both jitter phases. **Needs RenderDoc verification** for the resulting layout/barrier sequence (`scene_images` is `COLOR_ATTACHMENT | SAMPLED | TRANSFER_SRC | STORAGE` and already changes layout twice in the tail of the frame) — do not ship the barrier reshuffle on test evidence alone. If the move is judged too large, the narrower fix is to feed composite a temporally-stable sky/geometry coverage instead of the binary `depth < 1.0` (e.g. resolve coverage in TAA and pass it through), but that is a second temporal history to validate.

---

---

#### REN-2026-08-30-D16-01: `docs/engine/renderer.md` still documents M55 as Phase 1 with the output gated OFF

- **Severity**: Medium
- **Dimension**: Volumetrics
- **Location**: `docs/engine/renderer.md` (§"Volumetric lighting (M55)", ~line 665; plus the pipeline bullet at ~line 67)
- **Status**: OPEN — new
- **Description**: The engine's primary renderer reference describes the
  volumetrics pass as scaffolding: *"**Phase 1 (current)** allocates the
  per-FIF 3D images … the inject/integrate dispatch plumbing is wired but the
  output is gated off (`VOLUMETRIC_OUTPUT_CONSUMED = false`) until Phase 2
  adds density+lighting injection (TLAS shadow raymarch + Henyey-Greenstein
  phase) and ray-march integration."* Every clause is false at HEAD. The same
  claim is repeated in the pipeline bullet list near the top of the file
  (*"allocation + layout + dispatch plumbing live, scattering output not yet
  consumed (`VOLUMETRIC_OUTPUT_CONSUMED = false`)"*). The section also states
  `VOLUME_FAR = 200`.
- **Evidence**:
  - `crates/renderer/src/vulkan/volumetrics.rs:546` — `pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;`
  - `crates/renderer/shaders/composite.frag:720` — `combined = combined * vol.a + vol.rgb;` (the output *is* consumed)
  - `crates/renderer/shaders/volumetrics_inject.comp:1633`–`1636` — the TLAS shadow ray query the doc says is "Phase 2" work; `:1212` — the HG phase clamp
  - `crates/renderer/shaders/include/shader_constants.glsl:133` — `#define VOLUME_FAR 8960.0`, not 200 (`shader_constants_data.rs:345`–`354` records that the 200 was a units bug: 200 world units = 2.86 m)
  - Session 62 + 69 shipped injection, temporal reprojection, clustered local volumes and the transported combustion solver (`ROADMAP.md:809`)
- **Impact**: A reader (or an auditor) consulting the authoritative renderer
  doc concludes the volumetrics dispatches are dead GPU work and is one step
  from "optimising away" ~0.1–0.25 ms/frame of load-bearing work — precisely
  the mistake `post_passes.rs:440`–`446` warns against in code. It also
  understates the pass's VRAM and ray-query cost by describing an empty
  scaffold.
- **Suggested Fix**: Rewrite both sites against current code: consumed = true,
  the six-volume per-FIF set, the `froxel_extent` derivation with the live
  divisor, the config-driven far plane (`VolumetricsParams::volume_params.x`,
  default 128 m = 8 960 units), and a pointer to
  `docs/engine/procedural-volumetric-fog.md` as the deep spec. Consider
  extending the existing `froxel_grid_cost_matches_the_memory_budget_doc`
  pattern with a one-line `include_str!` assertion that `renderer.md` does not
  contain the string `VOLUMETRIC_OUTPUT_CONSUMED = false`.

---

---

#### REN-2026-08-30-D17-01: `MAT_FLAG_TRANSLUCENCY`'s #1147 Phase 2b subsurface term is unreachable — the per-light contribution gate `continue`s on exactly the `−N·L > 0` geometry that is the term's only non-zero domain


- **Severity**: MEDIUM
- **Dimension**: Disney BSDF (Phase 2b sibling gating)
- **Location**: `crates/renderer/shaders/triangle.frag` (contribution gate, lines 2865–2877; translucency block, lines 2913–2954), `crates/renderer/shaders/include/lighting.glsl` (`bethesdaDiffuseLightFactor` line 80, `bethesdaRimFactor` line 98, `bethesdaBackFactor` line 106)
- **Status**: NEW. Not in the 159-issue OPEN set (keyword sweep of `issues.json` for `translucen|sss|subsurface|bsdf` returns only #3452/#3448/#3071, none of which is this) and not in `docs/audits/AUDIT_RENDERER_2026-08-27.md` (`grep -n "translucen\|backDotL\|TRANSLUCENCY"` → no hits). Predates the 2026-08-25 Bethesda work — `git log -L 2864,2876:triangle.frag` shows the gate was `float contribution = NdotL * atten;` before `ceb69d24` widened it, so the defect is longstanding, not a regression of that commit.
- **Description**: Inside the cluster light loop, the per-light early-out is

  ```glsl
  float rawNdotL = dot(N, L);
  float NdotL = max(rawNdotL, 0.0);
  vec3 diffuseGate = bethesdaDiffuseLightFactor(mat, lightingMask, rawNdotL);
  float legacyGate = max(bethesdaRimFactor(mat, NdotV, NdotL),
                         bethesdaBackFactor(mat, rawNdotL));
  float contribution = max(max(diffuseGate.r, max(diffuseGate.g, diffuseGate.b)),
                           legacyGate) * atten;
  if (contribution < 0.001) { continue; }
  ```

  55 lines further down, still in the same iteration, the Phase 2b block runs
  `float backDotL = max(-dot(N, L), 0.0);` and accumulates
  `sssTint * translucencyTransmissiveScale * thicknessShape * turbMod * unshadowedRadiance`.
  `backDotL` is non-zero **iff** `rawNdotL < 0`. For a material that carries
  `MAT_FLAG_TRANSLUCENCY` and nothing else from the Bethesda lighting-response
  family, all three gate terms are identically zero on that half-space:
  `bethesdaDiffuseLightFactor` returns `vec3(max(rawNdotL, 0.0))` when
  `MAT_FLAG_SOFT_LIGHTING` is clear (lighting.glsl:84-86);
  `bethesdaRimFactor` returns `0.0` when `MAT_FLAG_RIM_LIGHTING` is clear
  (line 99) and is otherwise multiplied by `frontNdotL == 0`;
  `bethesdaBackFactor` returns `0.0` when `MAT_FLAG_BACK_LIGHTING` is clear
  (line 107). So the loop `continue`s. On the complementary half-space
  (`rawNdotL >= 0`) the gate passes but `backDotL == 0`, so the term is zero
  there too. The translucency contribution is therefore identically zero at
  every fragment, for every light.
- **Evidence**:
  - The three flags are genuinely disjoint on real content, so the "SOFT_LIGHTING rescues it" escape does not exist. `MAT_FLAG_TRANSLUCENCY` comes from `ImportedMaterial::has_translucency`, set only from `bgsm.translucency` (`byroredux/src/asset_provider/material.rs:40-41`), which `crates/bgsm/src/bgsm.rs:207-213` reads only when `version >= 8`. `MAT_FLAG_SOFT_LIGHTING` comes from `ImportedMaterial::soft_lighting`, set either from `bgsm.subsurface_lighting` (`forward_bgsm_rim_subsurface`, `byroredux/src/asset_provider/material.rs:92-97`), which `bgsm.rs:214-219` reads only in the `else` arm (`version < 8`), or from `skyrim_slsf2::SOFT_LIGHTING` — and `crates/nif/src/import/material/dedicated_shader.rs:170-181` extracts those three SLSF2 bits **only** for `TextureSlotLayout::Skyrim`, a family that ships no BGSM. Same argument for `MAT_FLAG_BACK_LIGHTING`.
  - Bit packing confirmed at `byroredux/src/cell_loader.rs:255-276` (`pack_imported_material_flags`) and `crates/renderer/src/shader_constants_data.rs:407-422`.
  - `grep -n "MAT_FLAG_TRANSLUCENCY" crates/renderer/shaders/**` → the only shading consumer is triangle.frag:2913 (2933/2946 are the `THICK_OBJECT`/`MIX_ALBEDO` sub-branches inside it; 1547 is the `viewMaterialLobe` debug colour). There is no second, ungated evaluation site.
  - The feed is fully wired and non-trivial: `translucency_subsurface_{r,g,b}`, `translucency_transmissive_scale`, `translucency_turbulence` are parsed (`bgsm.rs:209-213`), merged (`asset_provider/material.rs:1501-1514`), uploaded (`crates/renderer/src/vulkan/material.rs`), and offset-pinned. All of it terminates in dead shader code.
- **Impact**: The entire #1147 Phase 2b subsurface feature produces zero output on 100% of loaded content. FO4 foliage, paper, thin cloth, skin and frost-rimed glass — every vanilla BGSM v≥8 material that authors `bTranslucency` — never shows the back-lit wraparound the flag exists to produce, and the failure is silent: the flag is set, the fields are non-zero in the GPU material, `mat.dump` shows a correctly translated material, and `viewMaterialLobe` paints the fragment magenta ("translucency"). Nothing in `cargo test` can see it, because the defect is a control-flow ordering between two blocks that are each independently correct.
- **Suggested Fix**: Fold the translucency driver into the gate rather than moving the block. Add a fourth term alongside `legacyGate`, e.g.
  `float sssGate = ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u) ? max(-rawNdotL, 0.0) * mat.translucencyTransmissiveScale : 0.0;`
  and include it in the `max(...)` that forms `contribution`. That keeps the early-out doing its job (it exists to skip lights that cannot contribute) while making it agree with the set of lobes evaluated below it. Do **not** simply lower the `0.001` threshold — the gate would still be zero, because the driver is absent from it, not merely small. Pin the result with a shader-source contract test in `shader_contract_tests.rs` in the style of `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path`: assert that the `contribution` expression mentions a translucency term, so a future edit cannot silently re-orphan the block.

---

---

#### REN-2026-08-30-D17-02: the soft-shadow emitter disk is re-derived in the shader from the CULL radius (`position_radius.w`) instead of reading the canonical source radius the CPU already uploads in `params.y` — the two formulas agree only in the unclamped middle of the range


- **Severity**: MEDIUM
- **Dimension**: Soft Shadows
- **Location**: `crates/renderer/shaders/triangle.frag` (ReSTIR arm, line 3326; legacy-WRS arm, line 3479 — identical literal, duplicated). Canonical source: `crates/core/src/lighting.rs` (`Emitter::from_legacy_world_units`, lines 256-265; `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`, line 18), `byroredux/src/render/lights.rs` (`gpu_light_from_emitter`, lines 89-113)
- **Status**: NEW. `issues.json` keyword sweep for `penumbra|soft shadow|source radius|shadow disk` returns nothing; `grep -n "source_radius\|lightDiskRadius" docs/audits/AUDIT_RENDERER_2026-08-27.md` → no hits.
- **Description**: Both shadow-sampling arms compute the point/spot penumbra disk as
  `float lightDiskRadius = max(radius * 0.025, 1.5);`
  where `radius` is `lights[i].position_radius.w` — which `gpu_light_from_emitter` (lights.rs:94) uploads as `emitter.range.to_bethesda_units() * LIGHT_RANGE_EXTENSION`, i.e. the **cull** radius, deliberately `2.0×` the authored range (`LEGACY_LIGHT_CULL_RANGE_MULTIPLIER = 2.0`, `crates/core/src/lighting.rs:18`; `pointSpotAtten` recovers the authored radius from it as `kneeFrac * R`, lighting.glsl:66-68).

  Meanwhile the same `GpuLight` already carries a canonical emitter size:
  `params[1] = emitter.source_radius.to_bethesda_units()` (lights.rs:110), derived once at the translation boundary as
  `(range_world_units * 0.05).clamp(1.0, 32.0)` (`crates/core/src/lighting.rs:256-260`). That value is not ignored elsewhere — `pointSpotAtten` reads it as `sourceRadius` for the inverse-square arm (lighting.glsl:47), and `traceShadowTransmittanceDetailed` receives it as `emitterRadius` for the near-emitter shell test (lighting.glsl:266, `shadow_transport.glsl:31`), i.e. it is in scope at the very call the shadow sampler is making.

  In the linear middle the two agree by coincidence (`radius * 0.025 = range * 2.0 * 0.025 = range * 0.05`). They diverge at both clamps and under any change to the culling constant.
- **Evidence**:
  - **No ceiling in the shader.** CPU clamps the source radius at 32 units; the shader grows linearly forever. A 1024-unit FNV exterior lamp: shader disk `1024 * 2.0 * 0.025 = 51.2` vs canonical `32` (1.6×). A 4096-unit worldspace light: `204.8` vs `32` (6.4×).
  - **Floor mismatch.** Shader floor `1.5`; CPU floor `1.0`.
  - **Procedural emitters diverge badly.** `crates/renderer/src/vulkan/volumetrics.rs:709-736` builds combustion lights with a *physically derived* source radius — `(3V/4π)^(1/3)` clamped to `[0.02, 8.0]` m — written into `params[1]`, while `position_radius.w` is `range_metres * 70 * COMBUSTION_LIGHT_RANGE_EXTENSION`. A 3 m-range flame with the minimum 0.02 m luminous radius: canonical `params.y = 1.4` units; shader disk `max(3 * 70 * 2 * 0.025, 1.5) = 10.5` units — 7.5× too soft, and the shader's own `1.5`-unit floor alone already exceeds the canonical value. `BETHESDA_UNITS_PER_METER = 70.0` (`crates/core/src/lighting.rs:16`).
  - **Culling tunable silently owns shadow softness.** `LIGHT_RANGE_EXTENSION` is `pub const LIGHT_RANGE_EXTENSION: f32 = byroredux_core::lighting::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;` (lights.rs:55) — a pure cull-window constant. Changing it to 1.5 would shrink every penumbra by 25% with no lighting intent expressed anywhere.
  - The two shader sites are byte-identical copies, so any future retune has to be applied twice.
- **Impact**: Penumbra width is wrong wherever either clamp binds — over-soft on large-range authored lamps (interior chandeliers, exterior street lights) and grossly over-soft on the volumetric combustion lights, which are precisely the emitters for which someone did the work to compute a real physical radius. Because the disk only jitters the ray direction, the error shows up as an over-blurred contact shadow, which the ReSTIR EMA + TAA then happily converge to — it looks like a stable, deliberate soft shadow rather than a bug. It also puts a second, drifting definition of "how big is this lamp" in the tree, contradicting the `feedback_format_translation` doctrine the surrounding code cites.
- **Suggested Fix**: Replace both literals with the canonical value already in the struct:
  `float lightDiskRadius = max(lights[i].params.y, 1.0);`
  (the `1.0` floor mirrors the CPU-side `clamp(1.0, 32.0)` so a zero `params.y` from a hand-built emitter still yields a visible penumbra). This is a one-line change at each of triangle.frag:3326 and :3479, needs no new upload lane, and makes the shadow sampler consistent with `pointSpotAtten`'s inverse-square arm and with `traceShadowTransmittanceDetailed`'s shell test, which already read the same field. Add a shader-source assertion that neither arm contains `radius * 0.025`, so the cull-radius derivation cannot come back. Note this changes penumbra widths on real content, so it wants the `--bench-hold` + `byro-dbg` visual A/B the repo already uses for shadow tuning — not a blind merge.

---

---


### LOW

#### REN-2026-08-30-D1-01: `TlasIntegritySnapshot` was built to close the #1228 telemetry gap but has no consumer anywhere in the workspace — the three `missing_blas` cause counters still surface only through the rate-limited `log::warn!`


- **Severity**: LOW
- **Dimension**: AS Correctness (observability)
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs` (`TlasIntegritySnapshot`, line 62; field `tlas_integrity`, line 170), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`integrity_snapshot`, line 1062; assignment in `build_tlas_instances`, line 667)
- **Status**: NEW (the underlying observability gap is #1228, which is not in the 159-issue OPEN set; `issues.json` contains no AS/TLAS telemetry issue — only `#3510` and `#2774` match the AS keyword sweep, and neither is this)
- **Description**: `build_tlas_instances` splits TLAS-membership loss into three cause counters (`missing_skinned_blas`, `missing_rigid_blas`, `missing_ssbo_instance`) and, since the `TlasIntegritySnapshot` addition, persists them plus `eligible` / `emitted` / `frame` on the manager. The struct's own doc-comment states the design intent explicitly: *"Unlike the historical rate-limited warning, this snapshot persists zeroes and is therefore suitable for a positive correctness assertion: emitted must equal eligible and every missing-cause counter must remain zero."* No such assertion exists. `integrity_snapshot()` is a `pub` accessor with **zero call sites**, and `tlas_integrity` is referenced only at its own definition, initialisation, and single write. So the operator-facing state of the #1228 gap is unchanged: the only way to observe a TLAS-membership regression is still the once-per-second `log::warn!` inside `build_tlas_instances`, gated on `frame_index == 0`.
- **Evidence**:
  - `grep -rn "tlas_integrity\|TlasIntegritySnapshot" crates/ byroredux/ tools/` returns exactly 5 hits, all inside `crates/renderer/src/vulkan/acceleration/` (definition at `mod.rs:62`, field decl `mod.rs:170`, `Default` init `mod.rs:312`, write `tlas.rs:667`, accessor `tlas.rs:1062`). No console command, no `DebugStats` field, no `debug-protocol` component, no test.
  - The rate-limited warn is still the only surface: `tlas.rs:675` `if missing_blas_total > 0 && frame_index == 0 { … static LAST_LOG … }`.
  - `cargo test -p byroredux-renderer --lib acceleration` → 95 passed; none of them names `integrity`.
- **Impact**: A steady-state RT-membership regression (an LRU eviction that never recovers, a skinned first-sight build stuck failing) is visible only if someone is reading the log at the moment the once-per-second warn fires, and is invisible to `cargo test` and to `byro-dbg`. The snapshot that would make it a positive assertion is computed every frame and thrown away. Secondarily this is an unused `pub` API — dead surface in the renderer's public interface.
- **Suggested Fix**: Wire one consumer. Cheapest high-value option: assert it. Add a `debug_assert_eq!(snapshot.emitted, snapshot.eligible)` (plus the three-zero check) at the end of `build_tlas`, guarded so warmup frames — where `missing_skinned_blas` is legitimately non-zero on the first-sight frame — do not trip it. The operator-facing option is to surface it through an existing debug command (`world_info.rs` / `assets.rs` already carry the `tex.missing`-style pattern) so `byro-dbg` can read `eligible/emitted/skinned/rigid/ssbo` live. Either way, do not leave the accessor with no reader — the doc-comment promises an assertion that does not exist.

---

---

#### REN-2026-08-30-D1-02: the Dimension 1 checklist in `audit-renderer/SKILL.md` carries two stale claims — a deleted entry-point symbol and a "no recovery path exists" gap that was closed on 2026-08-16 — and the staleness already produced a false "re-verified as unchanged" line in the last full sweep


- **Severity**: LOW
- **Dimension**: AS Correctness (audit-tooling doc rot)
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (line 74 entry-point list; line 85 Dimension-1 LRU/shrink checklist). Ground truth: `crates/renderer/src/vulkan/context/resources.rs` (`restore_missing_static_blas_for_draws`, line 267), `byroredux/src/app_frame.rs` (line 235)
- **Status**: NEW
- **Description**: Two independent inaccuracies in the same checklist paragraph the auditor is instructed to treat as authoritative:
  1. **Line 74** lists `crates/renderer/src/vulkan/context/resources.rs` (`build_blas_for_mesh`) as a Dimension-1 entry point. That symbol does not exist. It was deleted under #2914 together with the single-shot `build_blas`; `docs/engine/memory-budget.md` documents the deletion in its own words ("the single-shot `build_blas` / `build_blas_for_mesh` pair had **no caller anywhere in the workspace** … Both functions were deleted under #2914"). The only surviving mentions in the tree are two prose references (`crates/facegen/src/eval.rs:119`, `resources.rs:203`) that both describe it in the past tense. **Here the SKILL is wrong and the code+`memory-budget.md` are right.**
  2. **Line 85** instructs the auditor to recast, not re-report, "#1793: a permanently-missing rigid BLAS has no recovery path (**no per-frame build primitive exists**)". The parenthetical is false. `VulkanContext::restore_missing_static_blas_for_draws` (`resources.rs:267`) is exactly that per-frame build primitive: it collects every rigid, TLAS-eligible draw handle, LRU-stamps the whole set via `mark_static_blas_used`, retains only `!accel.has_blas(handle)`, resolves each survivor's retained source (dedicated RT buffers at offset 0, or a byte-offset subrange of the global geometry buffers for global-only LOD meshes), and re-batches them through `build_blas_batched` — and it is called every frame from `byroredux/src/app_frame.rs:235`, before `draw_frame`. `build_tlas_instances`' own `missing_rigid_blas` arm has been rewritten to match: *"The app-frame prepass normally restores an evicted rigid BLAS from retained mesh buffers before entering `draw_frame`. Reaching this arm therefore means that recovery failed or the source mesh was ineligible."*
- **Evidence**:
  - `grep -rn "build_blas_for_mesh" crates/ byroredux/` → 2 hits, both prose, zero definitions or calls.
  - `git log -S "restore_missing_static_blas_for_draws" -- crates/renderer/src/vulkan/context/resources.rs` → `8e7582ed`, dated **2026-08-16** — eleven days *before* the last full sweep at `969d81c8` (2026-08-27).
  - `docs/audits/AUDIT_RENDERER_2026-08-27.md`, "Known-open, deliberately NOT re-reported": *"Per `SKILL.md` Dimension 1, the two documented-not-fixed AS gaps from `#1793` (no recovery path for a permanently-missing rigid BLAS; …) were re-verified as unchanged and are not re-reported."* That line is a verification claim the code did not support at the time it was written; the stale checklist is what produced it.
  - #1793 is **not** in the 159-issue OPEN set, consistent with the gap having been closed.
  - The *second* #1793 gap in the same sentence (a synchronous multi-cell `--grid` burst false-evicting via the shared `frame_counter`) **is** still accurate — `blas_static.rs:228-238` still carries the "Deferred pending a `--grid` + low-VRAM-budget repro" note and still bumps `self.frame_counter` per `build_blas_batched` call. `mark_static_blas_used` partially mitigates it for the upcoming rigid draw set but does not remove the counter-semantics hazard. Only the first half of the sentence needs correcting.
- **Impact**: The checklist's "Recast, don't re-report" instruction converts a stale premise into a *positive false statement in the audit record* — the strongest form of the ~1-in-6 stale-finding problem, because it manufactures a "verified intact" line rather than merely a dropped finding. Any future Dimension-1 run will reproduce the same false verification until the text is corrected.
- **Suggested Fix**: In `SKILL.md`: (a) replace `build_blas_for_mesh` in the line-74 entry-point list with `restore_missing_static_blas_for_draws` (the live pre-TLAS recovery primitive) and `build_blas_batched`; (b) in line 85, drop the "permanently-missing rigid BLAS has no recovery path" clause and replace it with a regression guard — *"verify the per-frame `restore_missing_static_blas_for_draws` prepass (`resources.rs`, called from `app_frame.rs`) still runs before `draw_frame` and still calls `mark_static_blas_used` **before** `handles.retain(|h| !accel.has_blas(h))`"* — which is already pinned by the source-position test at `resources.rs:473-497`; (c) keep the `--grid` / shared-`frame_counter` half of #1793 as-is, it is still accurate.

---

## Verified clean this run (not filed)

**BLAS build geometry.** All three triangle-geometry construction sites —
`blas_static.rs:303` (batched static size-query + record),
`blas_skinned.rs:110` (batched skinned first-sight build), and
`blas_skinned.rs:552` (`refit_skinned_blas` UPDATE) — set
`vertex_format(R32G32B32_SFLOAT)`, `index_type(UINT32)`,
`max_vertex(count.saturating_sub(1))`, and `flags(GeometryFlagsKHR::OPAQUE)`.
Strides are correct and per-class: static uses
`size_of::<Vertex>()` (104 B), both skinned sites use
`shader_constants::SKIN_OUTPUT_STRIDE_BYTES` (12 B, position-only per #2170)
and carry the "these must move together" warning. `primitive_count` is
`index_count / 3` at every site.

**Build-flag constants.** `STATIC_BLAS_FLAGS` (`FAST_TRACE|ALLOW_COMPACTION`),
`SKINNED_BLAS_FLAGS` (`FAST_BUILD|ALLOW_UPDATE`, deliberate per
R6a-prospector-regress), `UPDATABLE_AS_FLAGS` (`FAST_TRACE|ALLOW_UPDATE`) —
all three unchanged in `acceleration/constants.rs` and matching
`docs/engine/memory-budget.md`'s table verbatim. The static size-query
(`blas_static.rs:414`) and record path both read the same `STATIC_BLAS_FLAGS`
constant (VUID-03801); same for the skinned size-query
(`blas_skinned.rs:135`) and `refit_skinned_blas` (`blas_skinned.rs:588`)
against `SKINNED_BLAS_FLAGS` (VUID-03667).

**VUID-03667 refit guards (#1145 / #907).** `BlasEntry.built_flags` and
`built_vertex_count` / `built_index_count` are still populated at BUILD time
(static: `blas_static.rs:922-926`; skinned: the batched path) and validated on
every UPDATE by `validate_refit_flags` / `validate_refit_counts` in
`predicates.rs:163` / `:129`, ahead of the mutable borrow, with a
`drop_skinned_blas` + logged-error fallback rather than a silent VUID
violation. Intact.

**`instance_custom_index` ↔ SSBO contract (CRITICAL path).**
`instance_custom_index_and_mask: Packed24_8::new(ssbo_idx, shadow_mask)` at
`tlas.rs:654` takes `ssbo_idx` from the shared `instance_map` (never the raw
enumerate index). `MAX_INSTANCES = 0x40000` with the const-assert
`MAX_INSTANCES < (1 << 24)` still live at `scene_buffer/constants.rs:147`,
mirrored by the truncation-site `debug_assert!(ssbo_idx < (1u32 << 24))` at
`tlas.rs:618`. The `#2913` count pin
(`gpu_instances.len() == instance_map.iter().flatten().count()`,
`draw.rs:3228`) is still placed at the one point where the two must agree
exactly — after the draw loop, before the UI-quad append.

**TLAS BUILD/UPDATE decision.** `decide_use_update` keys only on
`needs_full_rebuild`, the `blas_map_generation` short-circuit, and the
`last_blas_addresses` zip. The VUID-03708 count guard
(`use_update && instance_count != built_primitive_count → BUILD`) covers both
grow and shrink. `built_primitive_count` is only assigned in the BUILD arm.
The #2674 commit-point discipline holds: `last_blas_addresses`,
`needs_full_rebuild = false`, and `last_blas_map_gen = map_gen` are all
written *after* `cmd_build_acceleration_structures` records, so a failed
`write_mapped` cannot leave the manager asserting a build that never landed.
Padded/unused instance-buffer slots do not break UPDATE: the range's
`primitive_count` is `built_primitive_count`, and the
`debug_assert_eq!(built_primitive_count, instance_count)` at `tlas.rs:336`
pins the buffer-content ↔ range-count equality.

**Transform.** `column_major_to_vk_transform` (`predicates.rs:28`) emits
`m[0],m[4],m[8],m[12] / m[1],m[5],m[9],m[13] / m[2],m[6],m[10],m[14]` — the
correct 3×4 row-major transpose of a glam column-major `[f32;16]`.
`tlas_instance_transform` correctly returns identity for skinned draws
(their BLAS is already absolute-world) and the model matrix for rigid draws;
pinned by `skinned_tlas_instance_uses_identity_transform`.
`TRIANGLE_FACING_CULL_DISABLE` is gated on `draw_cmd.two_sided` — the
checklist's "on all instances" wording is the pre-#416 behaviour and the
gating is the intended current design (RT traversal matching the raster
`PipelineKey`), not drift.

**Empty TLAS from frame 0.** `ensure_tlas_state` runs unconditionally on
`instance_count == 0`; the host→transfer copy and both buffer barriers are
skipped behind `if copy_size > 0` (#317, VUID-VkBufferCopy-size-01988), while
the `primitiveCount = 0` BUILD still runs so the descriptor binding is always
valid. A freshly created slot gets `needs_full_rebuild: true` +
`last_blas_map_gen: u64::MAX` + `built_primitive_count: 0`, forcing BUILD on
the first frame.

**Device-address usage flags.** Every buffer whose address is taken carries
`SHADER_DEVICE_ADDRESS`: skin-slot output (`skin_compute.rs:504-506`, plus
`ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`), BLAS result buffers,
TLAS scratch and device-local instance buffer, and the global geometry SSBOs
(`mesh.rs:1394-1396`, gated on `rt_enabled` — and `accel_manager` is itself
`None` unless `device_caps.ray_query_supported`, so the global-subrange BLAS
path in `restore_missing_static_blas_for_draws` cannot reach a buffer created
without the flag).

**#3469 cached skinned vertex address (delta change, `types.rs` +
`blas_skinned.rs`).** `SkinnedBlasGeometry.vertex_buffer: vk::Buffer` became
`vertex_address: vk::DeviceAddress`, sourced from `SkinSlot::output_address()`
(resolved once in `create_slot`, `skin_compute.rs:543`). Verified sound:
`SkinSlot::output_buffer` is never resized in place — a mesh-vertex-count
remap destroys and recreates the whole slot via
`skin_slot_capacity_stale` (`skinned_blas_refit.rs:246-268`), which also
drops the paired BLAS. The two `#2402` ordering hazards are both intact —
the `skin_slot_backs_mesh` filter still precedes the address read in
`draw.rs:3021-3028`, pinned by the source-scan test
`cached_skin_address_read_stays_behind_the_backing_filter`
(`skin_compute.rs:1671`), and the companion
`draw_frame_resolves_no_buffer_device_addresses` pins that no
`get_buffer_device_address` returned to `draw.rs`. The decision to leave the
index and scratch addresses as live queries is documented and correct (the
scratch buffer has three realloc sites).

**Deferred BLAS destruction (regression guard, `a476b256`).** No immediate
`destroy_acceleration_structure` at any eviction/drop site. `drop_blas`
(`blas_static.rs:50`) and `evict_unused_blas` (`:1094`) both
`pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN)`; `drop_skinned_blas`
(`blas_skinned.rs:733`) mirrors it. `AccelerationManager::destroy` drains
synchronously (`mod.rs:333-354`). The `drop_blas` overwrite guard before
`blas_entries[handle] = Some(...)` (`blas_static.rs:910`, #2481) is still
present and still pinned by the source-scan test in
`tests/blas_static_tests.rs:45`. The remaining immediate destroys in
`blas_static.rs` (479 / 641 / 799 / 858 / 868 / 883) are all
`submit_one_time` error-rollback arms or the post-fence Phase-7 teardown of
compaction *originals* that were never registered in `blas_entries` — safe by
construction, not regressions.

**LRU / shrink wiring.** `shrink_blas_scratch_to_fit` is called from
cell-unload (`byroredux/src/cell_loader/unload.rs:378`) and swapchain
recreate (`context/resize.rs:59`), matching `memory-budget.md:349-352`.
`shrink_tlas_to_fit` then `shrink_tlas_scratch_to_fit` run at the END of
`draw_frame` (`draw.rs:4033` / `:4047`, post-`current_frame` increment),
matching `memory-budget.md:354-361`. The slack calibration is correctly
split: `shrink_tlas_scratch_to_fit` → `tlas_scratch_should_shrink`
(`TLAS_SCRATCH_SLACK_BYTES`, 256 KB), `shrink_tlas_to_fit` →
`tlas_instance_should_shrink` (`TLAS_REBUILD_SLACK_BYTES`, 1 MB) — no BLAS
16 MB slack leaked onto a TLAS path (#1226). `evict_unused_blas` per-frame
call with `pending_bytes = 0` is at `draw.rs:2737`. Post-TLAS `rt_flag`
patch (#1227) intact at `draw.rs:2723`, with the `tlas_build_failed` arm
clearing it at `:2647`.

**Global-subrange BLAS vs. geometry compaction (investigated, no defect).**
`restore_missing_static_blas_for_draws`' `(None, None)` arm builds a BLAS
from a byte-offset subrange of the global geometry buffers. This is correct:
`accumulate_global_geometry` stores **local** (0-based) indices — the raster
path rebases via `cmd_draw_indexed`'s `vertexOffset`
(`mesh.rs:606-609`) — so `vertex_byte_offset = global_vertex_offset ×
stride` with unmodified indices is the right pairing, and
`sanitize_scene_indices` (#1532) already guarantees every index is in-range
of the mesh's own vertex block. A later `rebuild_geometry_ssbo` destroying
and compacting those buffers does **not** invalidate the BLAS: a built BLAS
holds its own BVH and triangle copy and never re-reads the source buffer
(static BLAS never refit; only skinned BLAS take the UPDATE path). No
finding.

**Docs.** `docs/engine/memory-budget.md` §"Acceleration Structures" (lines
337-410) and `docs/engine/shader-pipeline.md` (lines 40, 86-87, 428, 532,
549-550) match current code on every value and call site I checked —
including the three build-flag rows, the three slack constants, both reserve
floors, `SKINNED_BLAS_REFIT_THRESHOLD = 600`,
`BATCH_EVICTION_CHECK_INTERVAL = 64`, and the #2914 deletion note. The only
AS-relevant doc drift found is in the audit skill, filed as D1-02 above.

**Tests.** `cargo test -p byroredux-renderer --lib acceleration` → **95
passed, 0 failed**.

## Needs RenderDoc / device verification

Nothing. No barrier, render-pass, or pipeline change is proposed by this
dimension, and no observation in it required a live device to settle.

---

#### REN-2026-08-30-D2-03: `shader-pipeline.md`'s Set-1 descriptor table is two rows out of date — binding 11 is documented as 8 × `u32` / 32 B but is 17 × `u32` / 68 B, and binding 19 (`SelectedRayProbeBuffer`) is absent entirely


- **Severity**: LOW
- **Dimension**: SSBO/Indexing (authoritative-doc divergence)
- **Location**: `docs/engine/shader-pipeline.md:439`
  (binding 11 row) and `:444` (table ends at binding 18). Ground truth:
  `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs:12-29`
  (`GpuRayBudget`, `WORDS = 17`),
  `crates/renderer/shaders/include/bindings.glsl:406-424`
  (`RayBudgetBuffer`) and `:479-489` (`SelectedRayProbeBuffer`),
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs:852`
  (`write_storage_buffer(set, 19, …)`).
- **Status**: NEW rows. **Adjacent to `#3447`** (`REN-2026-08-27-D3-01`, open:
  "shader-pipeline.md still documents GpuInstance at 128 B and GpuCamera at
  352 B") — same document, same defect class, but different rows; `#3447`'s title
  and body scope it to the `GpuInstance`/`GpuCamera` size literals and
  `memory-budget.md`. Fold these two rows into `#3447` rather than filing
  separately if that issue is being worked. The `GpuCamera` 352 B → 368 B drift I
  re-confirmed (doc `:193`, `:427` vs `gpu_camera_is_368_bytes`) is **already
  `#3447`** and is not re-filed here.
- **Description**: the doc's binding-11 row reads *"`GpuRayBudget` — 8 × `u32`
  (32 B): `rayBudgetCount`, `glassRayLimit`, `directShadowSamples`,
  `maxPathSegments`, `maxShadedHits`, `volumetricLightCap`, `qualityTier`,
  reserved. Only word 0 is the CPU-zeroed atomic counter; sizing a
  range/flush/barrier from `u32` is 28 B short."* The struct now carries nine
  further RT-LOD telemetry words (`lod_fragments`, `lod_bin_0..3`,
  `reflection_traced`, `reflection_lod_culled`, `gi_traced`, `gi_lod_culled`),
  matching `bindings.glsl:415-423` one-for-one — 17 words, 68 B. Nor is word 0
  still the only shader-written word: `triangle.frag:795-800`, `:2627-2628` and
  `:3576-3577` `atomicAdd` into the telemetry tail, and
  `collect_rt_lod_telemetry` (`scene_buffer/descriptors.rs:237-251`) reads it
  back. Separately, Set 1 Binding 19 has existed since the selected-ray-probe
  work but never reached the table, so a reader adding a binding would pick 19
  as the next free slot.
- **Evidence**: `GpuRayBudget::WORDS = 17` and the 17-element `words()` array
  (`ray_budget.rs:33-54`); `bindings.glsl` declares 17 `uint` members;
  `buffers.rs:852` writes binding 19 and `bindings.glsl:479` declares it.
  Verified the *code* is internally consistent and safe: every size, range and
  flush already derives from `std::mem::size_of::<GpuRayBudget>()`
  (`descriptors.rs:221`, `buffers.rs:824`), so the doc's own "28 B short" warning
  is describing a hazard the code does not have — only the doc is stale. **No
  Vulkan change is proposed by this finding.**
- **Impact**: documentation only. The hazard is prospective: the stale row is
  precisely the one that warns a future author about sizing a barrier from the
  wrong type, and it now understates the struct by 36 B rather than the 28 B it
  names; an absent binding-19 row invites a descriptor-slot collision.
- **Suggested Fix**: update the binding-11 row to 17 × `u32` (68 B), list the
  telemetry words, and correct "only word 0 is CPU-zeroed / shader-written"; add
  a binding-19 row for `SelectedRayProbeBuffer` (`GpuSelectedRayProbe`, 144 B,
  pinned by `selected_ray_probe_is_144_bytes_std430_compatible`,
  `gpu_instance_layout_tests.rs:80`). `#3447`'s own suggested remedy — an
  automated size-literal check instead of a sixth manual sweep — would have
  caught all four rows at once and remains the better fix.

---

## Verified clean this run (regression guards — not re-filed)

| Check | Result |
|---|---|
| **`#3530` bit-31 masking, every reader** | `triangle.frag:228` + `:1569`, `material_sampling.glsl:49`, `ray_hit.glsl:296` all `& ~PARALLAX_ALPHA_HEIGHT_BIT`. No unmasked `textures[nonuniformEXT(mat.parallaxMapIndex)]` survives anywhere; a grep across all `.glsl`/`.frag`/`.vert`/`.comp` finds no fourth reader. `water.frag` inherits the masked path through `ray_hit.glsl`. Pinned by `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader`. |
| **Both POM marchers honour the channel** | raster `sampleParallaxHeight` (3 call sites: initial, loop, secant) and secondary-ray `heightInAlpha ? …a : …r` (3 sites). Neither marcher was left on `.r`. |
| **`NORMAL_ALPHA_SPEC_BIT` sibling masking** | `triangle.frag:1247`; the unmasked `mat.glossMapIndex != 0u` test at `:1240` is safe because the CPU only sets the bit when `normal_map_index != 0`. Only reader in the tree. |
| **`instance_custom_index` → SSBO** | `tlas.rs:654` `Packed24_8::new(ssbo_idx, shadow_mask)`; 2^24 ceiling guarded at `tlas.rs:602-620`; `MAX_INSTANCES` 262 144 ≪ 16 777 216. Every ray-query hit site uses `rayQueryGetIntersectionInstanceCustomIndexEXT`; `gl_InstanceID` appears nowhere in the RT paths. |
| **Vertex/index SSBO offsets (Set 1 b8/b9)** | `VERTEX_STRIDE_FLOATS 26`, `COLOR 3`, `NORMAL 7`, `UV 10`, `TANGENT 22` match `offset_of!(Vertex, …)` 0/12/28/40/88 ÷ 4 exactly (`vertex.rs:337-345`). Skinned branch's deliberate `i0` (no `+vOff`) into the per-entity `SKIN_OUTPUT_STRIDE_FLOATS` buffer is correct and documented (`ray_hit.glsl:107-131`). |
| **Shadow rays** | `traceShadowBinary` (`shadow_common.glsl:23-45`) = `OpaqueEXT \| TerminateOnFirstHitEXT`, explicit `tMin`, `tMax <= tMin` early-out, `!= …NoneEXT` → binary. The `shadow_transport.glsl` transmittance walkers deliberately use closest-hit (traversal-order any-hit is invalid for a shaded/accumulating query) — correct, not a missing flag. |
| **Ray self-intersection / tMin** | every `tMin` is `0.0` *because* origins come from `offsetRayOrigin` / `offsetRayOriginForDirection` (`ray_origin.glsl`), a correct Wachter & Binder ULP step evaluated in render-origin-relative space. Alpha-skip / passthru loops re-origin the same way and charge `length(nextOrigin - rayOrigin)` against the remaining reach in all four loops. No world-space epsilon survives. |
| **Frisvad basis (`#820`)** | `buildOrthoBasis` (`math_common.glsl:123-130`) is the standard sign-branched Frisvad/Duff form, unit-length, no `cross(N,up)` degenerate. Feeds `cosineWeightedHemisphere`. |
| **1-bounce GI** | viewer-oriented *geometric* `N_geom` (`triangle.frag:3636-3642`), cosine-weighted hemisphere, biased origin, `6000.0` cutoff, miss → `pathEnvironmentRadiance(pathDir)`, `pathSegmentLimit` clamped to the `MAX_PATH_SEGMENTS` const. |
| **Glass / IOR** | `glassIORAllowed = isGlass && !isThinGlass && reflectionGlassRayEnabled && !isWindow` intact verbatim (`triangle.frag:1791-1792`); `MAT_FLAG_THIN_GLASS` gate at `:1483`; window-portal demote on coincident interior geometry (`#789`) at `:1717`; interior refraction/reflection miss falls back to `sceneFlags.yzw` cell ambient, not the sky blend (`:2187-2189`, `raytrace.glsl:45-47`). |
| **`GLASS_RAY_BUDGET` / `#1438` contract** | doc comment intact at `bindings.glsl:400-405` and `triangle.frag:1780-1801`; the `atomicAdd` is telemetry-only and its return value is never used as an admission gate; **no CPU reader of word 0** — `RtLodTelemetry` (`ray_budget.rs:98-113`) consumes only the nine LOD words, never `ray_count`. |
| **`DBG_VIZ_GLASS_PASSTHRU`** | diagnostic state set unconditionally at `triangle.frag:1988-1992`; both viz-write branches present (`:2126`, `:2386`). |
| **RT gating** | `rtEnabled = sceneFlags.x > 0.5` (`triangle.frag:744`) fans out to the three per-feature gates; `water.frag` gates at `:358`, `:538`, `:1024`, `:1141`. TLAS is Set 1 Binding 2 in `bindings.glsl:309`, matching the doc. |
| **ReSTIR-DI spatial cone (`d523b9b3`)** | `SPATIAL_NORMAL_COS = 0.906` at `triangle.frag:3192`, tested **before** combine at `:3240`, against `geomN = normalize(fragNormalEffective)` (`:3044`) — the geometric normal, not shading `N`. Neighbour normal `octDecode(unpackSnorm2x16(rn.pad0))`; write side `packSnorm2x16(octEncode(normalize(fragNormalEffective)))` at `:3424`. `Reservoir` still 8 × 4 B = 32 B (`bindings.glsl:461-467`). `DBG_DISABLE_SPATIAL` gate at `:3185`. |
| **ReSTIR surface identity** | `uint surfaceId = inst.surfaceId & RESERVOIR_SURFACE_MASK` at `triangle.frag:3051`; mask `4194303u` = 0x3FFFFF (22 bits) matches the 10-light/22-surface split documented at `bindings.glsl:461`. |
| **BC1 punch-through alpha (`ae285062`)** | `INSTANCE_FLAG_DIFFUSE_ALPHA 256u` (bit 8); honoured by both the raster path (`triangle.frag:267`) and the secondary-ray path (`ray_hit.glsl:389`). |
| **IGN / hash seeding** | `resFrameSeed = cameraPos.w` (frame counter) and `frameCount` feed every stochastic site — deterministic per-pixel-per-frame, TAA-convergent. |
| **`#3459` glass pivots (delta)** | `DEFAULT_GLASS_BLUR_SCALE` / `DEFAULT_GLASS_REFRACTION_SCALE` now single-sourced from `byroredux_core::…::material` (`shader_constants_data.rs:478-482`) through `build.rs` into the generated header; the two `triangle.frag` divisors reference the macros. Pinned at `shader_contract_tests.rs:2157`, `:2177`. Neutrality argument holds. |
| **`REN-2026-08-27-D17-02` is FIXED** | the orphaned `shadowableLightRadiance` doc block was moved back above its function in the delta (`lighting.glsl`). Was LOW in the last sweep; do not re-file. |
| **SPIR-V lockstep** | all 22 GLSL sources recompiled with plain `glslangValidator -V` (no `--target-env`, per the recompile memory note) → **all 22 `.spv` byte-identical** to the checked-in copies, including the two the delta touched. |
| **Test suite** | `cargo test -p byroredux-renderer --lib` → **777 passed, 0 failed**. |

## Not examined by this dimension

`caustic_splat.comp` and `volumetrics_inject.comp` ray queries (Dimension 2's
entry points are `triangle.frag` + its includes and `water.frag`); the SVGF/TAA
compute chain; anything CPU-side outside the GpuMaterial/GpuInstance/RayBudget
index plumbing needed to close the checklist. No barrier, render-pass or
pipeline change is proposed anywhere in this report, and nothing here requires
RenderDoc to settle — all three findings are decidable from source.

---

#### REN-2026-08-30-D3-03: `PresentationPushConstants` smuggles two `u32` bitfields through `f32` lanes, against the codebase's own `uvec4` idiom


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/presentation.rs` (`PresentationPushConstants::render_debug_flags`, `::render_debug_mode`), `crates/renderer/shaders/presentation.frag` (`PresentationParams`)
- **Status**: New (introduced in the `969d81c8..HEAD` delta, with the new presentation pass)
- **Description**: The new presentation pass declares its two debug integers as
  `f32` on both sides of the boundary and round-trips them through the float
  representation: the host writes `f32::from_bits(u32)` and the shader reads them
  back with `floatBitsToUint`. The bit patterns involved are overwhelmingly
  **denormal** floats. This works today, but it makes a GPU struct's correctness
  depend on float-representation preservation for data that has no reason to be
  typed as float, and it inverts the idiom the rest of the renderer uses.
- **Evidence**: `presentation.rs:22-34` declares `render_debug_flags: f32` /
  `render_debug_mode: f32`; `presentation.rs:542-543` populates them:
  ```rust
  render_debug_flags: f32::from_bits(input.render_debug_flags),
  render_debug_mode:  f32::from_bits(input.render_debug_mode),
  ```
  `presentation.frag:27-28` mirrors them as `float renderDebugFlags; float renderDebugMode;`
  and `:135-136` recovers them:
  ```glsl
  uint dbgFlags  = floatBitsToUint(params.renderDebugFlags);
  uint debugMode = floatBitsToUint(params.renderDebugMode);
  ```
  Smallest normal `f32` is bit pattern `0x00800000`, so **every** `DBG_*` mask whose
  highest set bit is ≤ 22 is a denormal — that is `DBG_BYPASS_POM` (`0x1`),
  `DBG_VIZ_NORMALS` (`0x4`), … through `DBG_VIZ_FSR_TEMPORAL` (`0x400000`), i.e. most
  of the commonly-used views. Likewise `render_debug_mode` is a small enum
  (`RENDER_DEBUG_FINAL = 0` … `RENDER_DEBUG_MODE_MAX`), so **every non-zero debug mode
  is a denormal**. Masks that set bits 23-30 together reach the `Inf`/`NaN` exponent
  band.

  The rest of the engine does the opposite and does it correctly:
  `GpuCamera.render_debug` is `[u32; 4]` / `uvec4 renderDebug`, and where a *float*
  needs to ride in it, `triangle.frag:791` bitcasts **out of** the uint lane
  (`uintBitsToFloat(renderDebug.y)`) — a uint lane never subjects the payload to
  float interpretation. `presentation.rs` is the only site that runs the cast in the
  fragile direction.

  Honest scope: no driver-observed failure is confirmed. Vulkan denorm flush-to-zero
  (`VK_KHR_shader_float_controls`) is specified for floating-point *operations*, and
  neither the push-constant load nor `OpBitcast` is one, so current behaviour is
  expected to be correct. The finding is that the struct relies on that reasoning
  for no benefit.
- **Impact**: Latent robustness risk on any implementation that canonicalises
  denormal or NaN payloads across a float-typed push-constant load, which would zero
  or corrupt the debug mask. Debug-path only. The larger cost is consistency: a
  reader of `PresentationPushConstants` sees two float fields whose values are
  meaningless as floats, and the mismatch with the `uvec4 renderDebug` idiom two
  files away invites the wrong fix.
- **Suggested Fix**: Type both fields `u32` in `PresentationPushConstants` and `uint`
  in `presentation.frag`'s `PresentationParams`, assigning `input.render_debug_flags`
  / `input.render_debug_mode` directly and dropping both `floatBitsToUint` calls.
  Both fields sit in the same 16-byte block as `exposure` and the explicit
  `padding: f32`, so the struct stays exactly 128 B and
  `presentation_push_constants_match_shader_alignment` (which asserts size 128 and
  the `exposure`/`lens`/`fade_color` offsets) continues to pass unchanged.

---

## Summary

**3 findings: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 1 LOW.**

No `#[repr(C)]` GPU struct is drifting from its shader struct at HEAD. All three
headline structs (`GpuInstance` 160 B, `GpuCamera` 368 B, `GpuMaterial` 432 B) are
byte-correct, offset-pinned, mirror-consistent, and backed by non-stale `.spv`.
The two MEDIUM findings are both **guard-coverage gaps rather than live defects** —
one flag namespace that has run out of room with nothing watching, and a discovery
hole in the mirror-lockstep tests. The LOW is a type-choice regression introduced
with the new presentation pass.

---

#### REN-2026-08-30-D4-01: the authoritative submission-order doc is missing the #3308 depth-capture copy and never places the Scaleform overlay draw


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§ "Per-Frame Submission Order", the fenced block at lines 69–141)
- **Status**: NEW — doc is wrong, code is right.
- **Description**: The audit skill designates this block the authoritative
  per-frame ordering reference, and it enumerates every pass including the
  ones whose only content is a barrier (step 8 "G-buffer →
  `SHADER_READ_ONLY_OPTIMAL`", step 9 "caustic accum atomic-add →
  `SHADER_READ`"). Two things in the current frame graph are absent from it:
  1. **`depth_capture_record_copy` (#3308).** Recorded immediately after step
     7 (`copy_depth_to_history`) in
     `crates/renderer/src/vulkan/context/draw.rs` and it is *not* a no-op
     pass: it performs two `cmd_pipeline_barrier` calls that move
     `self.depth_image` `DEPTH_STENCIL_READ_ONLY_OPTIMAL → TRANSFER_SRC_OPTIMAL
     → DEPTH_STENCIL_READ_ONLY_OPTIMAL` around a `cmd_copy_image_to_buffer`
     (`crates/renderer/src/vulkan/context/depth_capture.rs:205` and `:223`),
     between the depth-history copy and every later depth consumer (SSAO,
     SVGF, composite, FSR). A reader reasoning about depth-image layout from
     this doc will not know that pass exists.
  2. **The Scaleform overlay draw.** The block never places it — not in step 6
     (the main render pass, where it lived until #3426) and not in step 20
     (the presentation pass, where `PresentationPipeline::record_overlay` now
     records it). The shader table at lines 29–30 lists `ui.vert` / `ui.frag`
     with no home.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/draw.rs` calls
    `self.copy_depth_to_history(cmd);` then `self.depth_capture_record_copy(cmd);`
    back to back; the doc's step 7 covers only the first.
  - `grep -n "depth_capture\|#3308" docs/engine/shader-pipeline.md` → no hits.
  - `grep -n "ui\.vert\|ui\.frag\|overlay" docs/engine/shader-pipeline.md` →
    only lines 29–30 (the shader table), nothing in the order block.
  - `crates/renderer/src/vulkan/presentation.rs`
    (`PresentationPipeline::dispatch` → `record_overlay`) is the current
    recording site, pinned by the in-file test
    `ui_overlay_composites_after_the_tone_map_draw`.
- **Impact**: The doc a future barrier investigation is told to trust omits a
  pass that transitions the depth image and misplaces the only draw that was
  relocated across the tone-map boundary in this delta. Nothing misbehaves at
  runtime.
- **Needs RenderDoc**: no
- **Suggested Fix**: Insert a step between 7 and 8 for
  `depth_capture_record_copy` (naming its two depth transitions and that it
  restores `DEPTH_STENCIL_READ_ONLY_OPTIMAL`), and extend step 20 to say the
  Scaleform overlay quad is recorded in the same subpass after the tone-map
  triangle (#3426). No code change.

---
- **Cross-dimension corroboration**: Found independently three times — also as *D0-01* (orchestrator) and *D8-03* (denoiser/composite). Severity arbitrated **down** to LOW: it is documentation-only, and unlike open `#3447` (wrong byte counts in a GPU layout contract, same doc) a missing pass row misleads rather than miscomputes. The `audit-renderer` SKILL's hand-written *"a finding that places the UI quad at the tail of the geometry pass is written against the pre-`#3426` shape"* warning is a maintenance workaround for exactly this gap.

---

#### REN-2026-08-30-D4-03: `FrameSync::images_in_flight`'s invariant doc cites four `draw.rs` line numbers that are all stale, and its deadlock rationale was inverted by #952


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/sync.rs:93–113` (`FrameSync::images_in_flight` doc comment)
- **Status**: NEW — doc-in-code is wrong, code is right. Distinct from #3442 (which is about `#2771`'s source-scan pin over the `(f + 1) % MAX_FRAMES_IN_FLIGHT` expression, a different file and a different mechanism).
- **Description**: This comment is the only place in the tree that states the
  `images_in_flight` safety invariant and the "if `draw_frame` ever drops to a
  single-slot fence wait, this breaks silently" warning. Two problems:
  1. **All four line citations are dead.** It names
     `context/draw.rs:179-186` (the image-fence read),
     `context/draw.rs:144-156` (the both-slots wait), `draw.rs:180` (the
     aliasing guard) and `draw.rs:191` (the fence reset). Those lines now
     hold unrelated pure helpers — `draw.rs:145` is
     `uses_rigid_motion_history`, `draw.rs:159` is
     `skinned_vertex_address_for_draw`, `draw.rs:195` is
     `skin_slot_backs_mesh`. The real sites are `draw.rs:1624–1636`
     (`wait_for_fences` on `in_flight[frame]` + `in_flight[prev]`),
     `draw.rs:1745–1761` (the `image_fence != in_flight[frame]` guard and the
     `images_in_flight[img] = in_flight[frame]` store), and `draw.rs:3811`
     (`reset_fences`).
  2. **The stated reason for the aliasing guard no longer holds.** The doc
     says: "Reusing the slot's own fence would block on an UNSIGNALED handle
     (it's reset at `draw.rs:191`) and deadlock." `#952 / REN-D1-NEW-04` moved
     `reset_fences` out of that position to immediately before `queue_submit`
     — `draw.rs:1763` carries the comment recording the move, and the call
     itself is at `draw.rs:3811`. At the guard site the slot's own fence is
     therefore still SIGNALED (it was waited on at `draw.rs:1624`), so waiting
     on it would return immediately, not deadlock. The guard is still correct
     and worth keeping; its documented justification is simply no longer the
     true one.
- **Evidence**: `grep -n "draw.rs:179\|draw.rs:144\|draw.rs:180\|draw.rs:191"
  crates/renderer/src/vulkan/sync.rs` → lines 95, 97, 102, 105; compare
  against `sed -n '140,200p' crates/renderer/src/vulkan/context/draw.rs` and
  `grep -n "reset_fences" crates/renderer/src/vulkan/context/draw.rs`
  (`1763` comment, `3811` call).
- **Impact**: A maintainer following this comment to check the invariant
  before, say, narrowing the both-slots wait to one slot lands in the middle
  of `draw.rs`'s pure-function preamble and gets a rationale that contradicts
  the code. The invariant itself is intact and correctly upheld today.
- **Needs RenderDoc**: no
- **Suggested Fix**: Replace the four citations with symbol-anchored prose
  (`draw_frame`'s both-slots `wait_for_fences`; the `image_fence !=
  in_flight[frame]` guard; the pre-`queue_submit` `reset_fences`) rather than
  line numbers, and restate the guard's purpose post-#952. Consider a
  source-scan pin in the sibling style of the existing
  `dependency_chain_tests` in `egui_pass.rs`. No behavioural change.

---

---

#### REN-2026-08-30-D4-04: `screenshot_record_copy`'s `# Safety` contract still names composite as the swapchain writer — the same stale attribution #2786 fixed next door in `egui_pass.rs`


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:144–153` (`VulkanContext::screenshot_record_copy` doc + `# Safety` block)
- **Status**: NEW — doc-in-code is wrong, code is right.
- **Description**: The function's prose says "Called in `draw_frame()` **after
  composite dispatch** … The swapchain image is in `PRESENT_SRC_KHR` layout
  **after the composite pass**", and the `# Safety` clause says the image
  "must currently be in `PRESENT_SRC_KHR` layout (**this frame's composite
  pass output**)". Since the FSR tail landed, composite writes a
  render-resolution HDR intermediate and never touches the swapchain; the
  swapchain writer is `PresentationPipeline` (`presentation.rs`, attachment
  `UNDEFINED → PRESENT_SRC_KHR`), or `EguiPass` (`LOAD` op,
  `PRESENT_SRC_KHR → PRESENT_SRC_KHR`) when the debug overlay is active — and
  `screenshot_record_copy` is called after *both*, at the tail of
  `draw_frame`'s `unsafe` block. The *layout* half of the contract is still
  correct; only the attribution is wrong. `#2786` fixed precisely this stale
  "composite writes the swapchain" claim in `egui_pass.rs` and did not sweep
  this sibling.
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` — attachment
    `.initial_layout(vk::ImageLayout::UNDEFINED)
    .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)`.
  - `crates/renderer/src/vulkan/egui_pass.rs:353–358` — `LOAD` / `STORE`,
    `PRESENT_SRC_KHR` on both ends; and its `in_dep` comment, corrected under
    #2786, spelling out "that pass is `PresentationPipeline`
    (`presentation.rs`), not composite".
  - `crates/renderer/src/vulkan/context/draw.rs` records, in order,
    `record_post_passes(...)` (which ends in `record_presentation_pass`), then
    the egui `pass.dispatch(...)`, then `self.screenshot_record_copy(cmd,
    swapchain_image);`.
- **Impact**: The `# Safety` precondition of an `unsafe fn` names the wrong
  producer. A future reader auditing whether the `PRESENT_SRC_KHR` precondition
  still holds will go and check composite, which is now irrelevant to it.
- **Needs RenderDoc**: no
- **Suggested Fix**: Reword both the prose line and the `# Safety` clause to
  name the presentation pass (and egui when active) as the producer, mirroring
  the #2786 wording in `egui_pass.rs`. No code change.

---
- **Cross-dimension corroboration**: Found independently twice — also as *D20-02*, which lists three further sites in the egui overlay path carrying the same stale "composite writes the swapchain" attribution.

---

#### REN-2026-08-30-D4-05: `presentation.rs`'s "#2465 — MEASURED, deliberately unchanged" justification predates #3426, which added three new access types to that pass


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/presentation.rs:292–330` (the `#2465 (REN-D4-2026-08-07-01)` comment block sitting between the `incoming` and `outgoing` `vk::SubpassDependency` declarations in `PresentationPipeline::create`)
- **Status**: NEW — stale in-code justification. **Observation only; no edit proposed.**
- **Description**: The presentation render pass declares
  `incoming` with
  `dst_stage_mask = FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT` and
  `dst_access_mask = SHADER_READ | COLOR_ATTACHMENT_WRITE`. Immediately below
  it, a long comment closes a prior audit finding with: "Verified 2026-08-14,
  release build, `BYRO_VALIDATION=1` … 300 frames on a live FNV exterior …
  **zero SYNC-HAZARD reports** … So this stays as-is. … Revisit only with an
  actual sync-val hazard or a driver-observed artifact; **a repeat of the
  static reading alone is not new evidence**."

  On 2026-08-29 (`b28acb0c`, #3426) the pass's *contents* changed. Its single
  subpass previously held exactly one non-blending fullscreen triangle. It now
  additionally holds `record_overlay`, which introduces three access types the
  measured pass did not have:
  - `VERTEX_INPUT` — `cmd_bind_vertex_buffers` / `cmd_bind_index_buffer` on the
    UI quad;
  - `VERTEX_SHADER` `SHADER_READ` — `ui.vert` reads
    `instances[gl_InstanceIndex]` from the instance SSBO (set 1, binding 4);
  - `COLOR_ATTACHMENT_READ` — the overlay pipeline blends against the
    attachment.

  None of the three appears in the `incoming` dependency's dst scope. Reading
  the source, each looks benign: the UI quad's vertex/index buffers are
  uploaded once at registration on a separate fence-waited submit; the
  instance SSBO's host write is published by the global
  `HOST_WRITE → VERTEX_SHADER | FRAGMENT_SHADER | COMPUTE_SHADER |
  DRAW_INDIRECT` `memory_barrier` recorded before the geometry pass
  (`draw.rs:3612`), which is a plain pre-render-pass barrier and therefore
  applies without needing to be restated as a subpass dependency; and the
  blend reads only what the tone-map triangle wrote in the same subpass, which
  rasterization order covers. So this is **not** reported as a defect.

  What *is* a defect is the standing justification: the comment's own escape
  clause ("revisit only with an actual sync-val hazard") is now unsatisfiable,
  because the pass that was measured on 2026-08-14 is not the pass that ships
  today, and nobody can produce a sync-val hazard for the new contents without
  re-running the measurement.
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` — `incoming` masks as quoted;
    `#2465` comment at `:292`, "Verified 2026-08-14" at `:311`.
  - `PresentationPipeline::record_overlay` — `cmd_bind_vertex_buffers`,
    `cmd_bind_index_buffer`, `cmd_draw_indexed`, and the two-set rebind
    (`overlay.texture_set`, `overlay.scene_set`) against
    `self.overlay_pipeline_layout`. (The checklist's "both descriptor sets are
    rebound because the tone-map draw binds a layout-incompatible set 0" is
    **confirmed present** — see the verified-clean list below.)
  - `crates/renderer/shaders/ui.vert` — `GpuInstance inst =
    instances[gl_InstanceIndex];` in `main()`, i.e. a **vertex-stage** SSBO read.
  - `crates/renderer/src/vulkan/pipeline.rs::create_ui_pipeline` — blending
    enabled, one colour-blend attachment.
  - `git log -1 --format=%ad --date=short b28acb0c` → `2026-08-29`, four days
    after the recorded measurement.
- **Impact**: A future auditor reading this file is told the dependency scopes
  were empirically validated and should not be re-examined. That statement is
  now scoped to a superseded version of the pass. No runtime misbehaviour is
  claimed or implied.
- **Needs RenderDoc**: **yes** — settling whether the three new access types
  want naming in `incoming` requires a `BYRO_VALIDATION=1` run
  (`SYNCHRONIZATION_VALIDATION`) with the Scaleform overlay actually on screen
  (e.g. `--menu` on FO4/Skyrim per `docs/smoke-tests/m48-menu-load.sh`, plus
  `--bench-hold`). No such device exists in this session.
- **Suggested Fix**: **No barrier edit.** Annotate the `#2465` block to record
  that the measurement predates #3426 and covers only the tone-map triangle,
  and that a re-run with the overlay live is the outstanding evidence. If and
  only if that re-run reports a hazard should the masks be touched.

---

---

#### REN-2026-08-30-D4-06: `renderer.md` names a "HOST→AS_BUILD" barrier as what gates the ray-query consumers — no such barrier exists


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/renderer.md:290–293` (per-frame order, step 10)
- **Status**: NEW — doc is wrong, code is right.
- **Description**: Step 10 reads: "Rebuild/refit the TLAS over visible BLASes
  … **HOST→AS_BUILD memory barrier before the ray-query consumers.**" The
  barrier that actually gates the ray-query consumers is
  `ACCELERATION_STRUCTURE_BUILD_KHR` / `ACCELERATION_STRUCTURE_WRITE_KHR` →
  `FRAGMENT_SHADER | COMPUTE_SHADER` / `ACCELERATION_STRUCTURE_READ_KHR`
  (`crates/renderer/src/vulkan/context/draw.rs:2688`), and it is the frame's
  *only* AS_WRITE→AS_READ barrier — it publishes the skinned BLAS refits as
  well as the TLAS build (#2931). There is no `HOST → ACCELERATION_STRUCTURE_
  BUILD_KHR` barrier anywhere in the renderer: the only HOST-source barrier on
  the AS path is `HOST_WRITE → TRANSFER_READ` on the TLAS instance staging
  buffer (`acceleration/tlas.rs:206–212`), which orders the host write against
  the staging→device-local copy, not against any ray-query consumer.
  (`grep -rn "PipelineStageFlags::HOST" crates/renderer/src/vulkan/` returns
  eleven sites; none pairs HOST with an AS-build destination.)
  The same step list also predates the depth-history copy, the #3308 depth
  capture, and the overlay's move into step 23 — the `renderer.md` counterpart
  of D4-01.
- **Evidence**: `crates/renderer/src/vulkan/context/draw.rs:2688–2697`
  (the `memory_barrier(...)` call with `ACCELERATION_STRUCTURE_BUILD_KHR` /
  `ACCELERATION_STRUCTURE_WRITE_KHR` source and
  `FRAGMENT_SHADER | COMPUTE_SHADER` / `ACCELERATION_STRUCTURE_READ_KHR`
  destination, plus its #2931 both-arms comment);
  `crates/renderer/src/vulkan/acceleration/tlas.rs:200–212` (`host_to_transfer`,
  `HOST → TRANSFER`).
- **Impact**: `/audit-severity` sets "Missing AS barrier (build → shader read)"
  at HIGH minimum, so this is the one edge in the frame graph a doc must
  describe correctly. Describing it with the wrong source stage and access
  would let a reader conclude the real barrier is redundant. Runtime behaviour
  is correct.
- **Needs RenderDoc**: no
- **Suggested Fix**: Correct step 10 to name the actual barrier
  (`AS_BUILD/AS_WRITE → FRAGMENT_SHADER|COMPUTE_SHADER/AS_READ`, emitted on
  both the build-success and build-failure arms), and mention the
  `HOST_WRITE → TRANSFER_READ` instance-staging barrier separately if it is
  worth listing at all. No code change.

---

## Verified clean (no finding)

Checked against current source; all previously-fixed hazards in this dimension
are still fixed, and their regression guards are still in place.

- **`render_finished` is per swapchain image, indexed by `image_index`**
  (regression guard for `548c1b69` / VUID-vkQueueSubmit-pSignalSemaphores-00067).
  `crates/renderer/src/vulkan/sync.rs` — `render_finished: Vec<vk::Semaphore>`
  sized `swapchain_image_count`, rebuilt by `recreate_for_swapchain`;
  `draw.rs:3792` signals `self.frame_sync.render_finished[img]` (not `[frame]`),
  with the full Khronos-issue-2007 rationale retained in the `FrameSync` type
  doc. `image_available` and `in_flight` stay per frame-in-flight. Intact.
- **Fence/semaphore lifecycle.** Both-slots `wait_for_fences` at frame start
  (`draw.rs:1624`); `images_in_flight[img]` cross-slot wait with the
  `image_fence != in_flight[frame]` aliasing guard (`draw.rs:1745`);
  `reset_fences` immediately before `queue_submit` (#952, `draw.rs:3811`);
  six error paths call `recreate_image_available_for_frame` so a consumed-less
  acquire signal can never leak (#910), and submit failure additionally calls
  `recreate_in_flight_for_frame`. The `framebuffers.is_empty()` sentinel
  returns **before** `acquire_next_image` (#1211). The ordering is pinned by
  the source-scan test in `draw.rs:4538–4600`.
- **AS-build INPUT access flag** (regression guard, `507945d8` / #1436). Both
  sites still use `SHADER_READ` at `ACCELERATION_STRUCTURE_BUILD_KHR` for
  build *inputs*, not `ACCELERATION_STRUCTURE_READ_KHR`:
  `acceleration/tlas.rs:230–250` (instance-buffer `TRANSFER_WRITE → SHADER_READ`)
  and `context/skinned_blas_refit.rs:480–487`
  (`COMPUTE_SHADER/SHADER_WRITE → AS_BUILD|FRAGMENT_SHADER / SHADER_READ`).
  `ACCELERATION_STRUCTURE_READ_KHR` is retained only where an AS *structure*
  is read (`blas_static.rs:607`, `blas_skinned.rs:698`,
  `skinned_blas_refit.rs:675`, `draw.rs:2688`). Intact.
- **AS build → fragment/compute read.** `draw.rs:2688` emits
  `ACCELERATION_STRUCTURE_BUILD_KHR / AS_WRITE →
  FRAGMENT_SHADER|COMPUTE_SHADER / AS_READ` on **both** the TLAS-success and
  TLAS-failure arms (#2931), which is what also publishes the earlier
  `record_skinned_blas_refit` writes; the failure arm matters because
  volumetrics gates on `tlas_handle` rather than `rt_flag`.
- **Skin compute → BLAS → fragment chain.** Palette dispatch →
  `COMPUTE_SHADER/SHADER_WRITE → COMPUTE_SHADER|VERTEX_SHADER/SHADER_READ`
  buffer barrier (`draw.rs:2578–2596`); skinned-vertex compute →
  `AS_BUILD | FRAGMENT_SHADER` (the #2403 `skinnedVertexAddress`
  buffer-reference widening is still present) → refit →
  `AS_WRITE → AS_READ`. Chain complete without relying on cluster-cull's
  incidental global barrier.
- **G-buffer → compute consumers.** `helpers.rs::create_render_pass`'s
  `dependency_out` (`COLOR_ATTACHMENT_OUTPUT|EARLY_FRAGMENT_TESTS|
  LATE_FRAGMENT_TESTS → FRAGMENT_SHADER|COMPUTE_SHADER`, `SHADER_READ`) plus
  the attachments' `SHADER_READ_ONLY_OPTIMAL` final layouts; `#573`'s
  BOTTOM_OF_PIPE omission and `#947`'s EARLY_FRAGMENT_TESTS addition both
  still in place. The FSR mask attachments (6, 7) are read at
  `COMPUTE_SHADER`, inside that dst scope, and `record_fsr_barriers_before`
  additionally emits its own per-image barriers.
- **Caustic accumulator.** Cleared before the main pass
  (`wca.clear_pre_render_pass`), and `record_post_passes`'s leading
  water-caustic barrier publishes `water.frag`'s `imageAtomicAdd`
  (`FRAGMENT_SHADER` write) to the composite `SHADER_READ`.
- **egui incoming dependency vs the new presentation pass (#1433 / #2786).**
  Chain is stitched from both ends and source-pinned: `presentation.rs`'s
  `outgoing` declares `dst_stage = COLOR_ATTACHMENT_OUTPUT | TRANSFER` /
  `dst_access = COLOR_ATTACHMENT_READ|WRITE | TRANSFER_READ` (naming the egui
  overlay and the screenshot copy), and `egui_pass.rs`'s `in_dep` waits
  `COLOR_ATTACHMENT_OUTPUT → COLOR_ATTACHMENT_OUTPUT`. Both halves are held by
  `egui_pass.rs`'s `dependency_chain_tests`
  (`presentation_outgoing_dep_still_names_this_overlay_as_a_consumer`,
  `egui_incoming_dep_waits_at_the_stage_presentation_signals`). The overlay's
  relocation did not disturb this edge — the overlay draws *inside* the
  presentation pass, not between it and egui.
- **Presentation render-pass load/store ops vs FSR output layout.** Attachment
  is `DONT_CARE / STORE`, `UNDEFINED → PRESENT_SRC_KHR`; the fullscreen
  tone-map triangle covers the whole attachment before the overlay blends over
  it, so `DONT_CARE` is safe. The upscaler leaves `output_images[frame]` in
  `SHADER_READ_ONLY_OPTIMAL` on **both** paths —
  `record_fsr_barriers_after` (`GENERAL → SHADER_READ_ONLY_OPTIMAL`,
  `COMPUTE_SHADER → COMPUTE_SHADER|FRAGMENT_SHADER`) and `record_native_blit`'s
  trailing barrier (`TRANSFER_DST_OPTIMAL → SHADER_READ_ONLY_OPTIMAL`,
  `TRANSFER → FRAGMENT_SHADER`) — which is exactly what
  `PresentationPipeline::write_inputs` declares
  (`.image_layout(SHADER_READ_ONLY_OPTIMAL)`), and the pass's `incoming`
  `COMPUTE_SHADER|TRANSFER → FRAGMENT_SHADER` / `SHADER_WRITE|TRANSFER_WRITE →
  SHADER_READ` covers both producers. Coherent.
- **The #3426 descriptor rebind.** `record_overlay` does bind **both** sets
  (`&[overlay.texture_set, overlay.scene_set]` at set index 0, against
  `self.overlay_pipeline_layout`) before `cmd_draw_indexed`, and re-sets
  viewport + scissor, with the `UI_PIPELINE_DYNAMIC_STATES.len() == 2` const
  assertion carried over from the geometry pass (#663). The checklist item is
  satisfied.
- **Overlay pipeline / layout ownership across recreate + teardown.**
  `overlay_pipeline` is owned by `PresentationPipeline` and destroyed before
  its render pass; `overlay_pipeline_layout` is the shared
  `VulkanContext::pipeline_layout`, deliberately not destroyed there, and
  `resize.rs` never destroys or recreates that layout (it passes
  `self.pipeline_layout` back into `recreate_triangle_pipelines`), so the
  handle presentation stores stays valid across a resize.
- **Swapchain recreate.** `device_wait_idle` at entry (`resize.rs:37`); the
  presentation pipeline is destroyed **before** `frame_upscaler.recreate`
  replaces the output views its descriptors reference (`resize.rs:1007–1050`);
  old image views are destroyed after the new swapchain is created and before
  the retired swapchain (#654); `frame_sync.recreate_for_swapchain` resizes
  `images_in_flight` and rebuilds `render_finished` to the new image count,
  destroying `in_flight` fences in a separate pass from recreating them so a
  partial failure leaves no dangling handle; the egui pass is fully rebuilt on
  format change (pinned by
  `egui_pass_rebuilds_fully_on_swapchain_format_change`).
- **`#3308` depth capture's device→host visibility.** `depth_capture_
  finish_readback` runs after the both-slots fence wait (`draw.rs:1708`, right
  after `screenshot_finish_readback`) and performs an explicit
  `invalidate_mapped_memory_ranges` on non-`HOST_COHERENT` memory via
  `buffer::aligned_flush_range` — i.e. it correctly applies the #2740 /
  REN-D4-04 rule that a fence's memory dependency has device-only access
  scope. Its `to_src` barrier also correctly names
  `DEPTH_STENCIL_ATTACHMENT_WRITE` in the source access scope (the #2484
  lesson), and its trailing restore chains to `copy_depth_to_history`'s
  outgoing scope through the shared `EARLY_FRAGMENT_TESTS | FRAGMENT_SHADER`
  stages. New code, clean.

---

#### REN-2026-08-30-D5-03: `memory-budget.md`'s Texture Registry descriptor-pool row omits the ×2 for the second binding, and the code's own SAFETY comment repeats the omission

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md:421`;
  `crates/renderer/src/texture_registry.rs:434-447` and `:1735-1740`
- **Status**: Open — the **doc** (and one code comment) is wrong; the code is right.
- **Description**: The ledger records the bindless descriptor pool as
  `max_textures × MAX_FRAMES_IN_FLIGHT` combined image samplers. Both pool-creation
  sites size it as `max_textures * 2 * MAX_FRAMES_IN_FLIGHT`, deliberately — each
  per-frame set carries **two** `max_textures`-sized bindings, as the line-433
  comment says ("two bindings in each per-frame set"). The `SAFETY` comment three
  lines below the sizing (`:445-447`) then contradicts it: "sizes cover exactly
  `MAX_FRAMES_IN_FLIGHT` sets of `max_textures` samplers each" — the same dropped
  ×2 as the doc, sitting directly under the correct expression.
- **Evidence**:
  - `texture_registry.rs:436-438`: `descriptor_count: max_textures * 2 * MAX_FRAMES_IN_FLIGHT as u32,`
  - `texture_registry.rs:1735-1737`: identical expression on the pool-rebuild path.
  - `docs/engine/memory-budget.md:421`: `| Descriptor pool | max_textures × MAX_FRAMES_IN_FLIGHT combined image sampler descriptors |`
- **Impact**: Halves the documented descriptor-pool ceiling for the one subsystem
  whose known failure mode is exhausting that ceiling (the same section documents
  #2030's grow-only slot leak). A reader sizing `max_textures` against the doc
  under-provisions by 2×.
- **Suggested Fix**: Change the doc row to `max_textures × 2 × MAX_FRAMES_IN_FLIGHT`
  and say why (two bindings per set), and fix the `SAFETY` comment at
  `texture_registry.rs:445-447` to match the expression it is justifying.

---

---

#### REN-2026-08-30-D5-04: `rebuild_geometry_ssbo_inner`'s gate comment says a first build takes the chunked path; it takes the atomic one

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/mesh.rs:1370-1372` (comment) vs `:1386,1435` (code)
- **Status**: Open — introduced alongside the #3443 fix in `fa511bbf`.
- **Description**: The comment immediately above the #3443 gate reads: *"Only
  meaningful once there's an old generation to duplicate alongside — a first build
  has nothing to keep serving draws, so it always goes straight through the chunked
  path below."* The code does the opposite: the chunked block is entered only when
  `has_existing_buffers && duplicate_is_safe`, so a first build
  (`has_existing_buffers == false`) skips it entirely and falls through to
  `rebuild_geometry_ssbo_atomic_fallback`. That is the correct behaviour — there is
  nothing to keep serving, so a synchronous build is right — but the sentence
  describing the gate states the wrong branch, in the one comment a future auditor
  reads to confirm #2374's device-loss protection is intact.
- **Evidence**:
  - `mesh.rs:1385`: `let duplicate_is_safe = !geometry_rebuild_needs_idle(projected_bytes, has_existing_buffers);`
  - `mesh.rs:1386`: `if has_existing_buffers && duplicate_is_safe {` … `return self.advance_geometry_rebuild(...)`
  - `mesh.rs:1435`: unconditional fall-through to `rebuild_geometry_ssbo_atomic_fallback`.
  - `geometry_rebuild_needs_idle` (`mesh.rs:228-233`) returns `false` whenever
    `has_existing_buffers` is `false`, which is what makes `duplicate_is_safe`
    `true` for a first build and the sentence look plausible without being true.
- **Impact**: Doc-level only, but on the exact predicate #3443 was filed about. The
  same paragraph is what a reader would use to decide whether a large first-load
  (FO4 boundary, ~800–900 MiB) is chunked or atomic; it is atomic, and the comment
  says otherwise.
- **Suggested Fix**: Reword to "a first build has nothing to keep serving draws, so
  it skips the chunked path and builds synchronously in the fallback below" — or
  move the sentence next to `geometry_rebuild_needs_idle`, which is what the
  `has_existing_buffers` term it describes actually belongs to.

---

---

#### REN-2026-08-30-D5-06: `destroy_depth_capture_staging`'s SAFETY comment names a caller set that does not exist

- **Severity**: Low
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs:299-305`
- **Status**: Open — new in this delta.
- **Description**: The `unsafe { destroy_buffer }` inside
  `destroy_depth_capture_staging` is justified as: *"callers are the resize path in
  `ensure_depth_capture_staging` (between frames, before any copy is recorded
  against the new buffer) and shutdown teardown (after `device_wait_idle`)"*. Two
  of the three claims are wrong. `ensure_depth_capture_staging` is called only from
  `depth_capture_record_copy` (`depth_capture.rs:134`), which runs **during**
  command-buffer recording at `draw.rs:3684`, not between frames; and there is no
  resize call site at all — `recreate_swapchain` never touches depth-capture
  staging (`grep -n depth_capture crates/renderer/src/vulkan/context/resize.rs` is
  empty). The destroy *is* sound, but for a different reason: `draw_frame` waits
  **both** FIF fences at `draw.rs:1628-1640` before any recording, so no submitted
  copy can still target the buffer being freed.
- **Evidence**:
  - `depth_capture.rs:132-136`: `self.ensure_depth_capture_staging(buffer_size);`
    inside `unsafe fn depth_capture_record_copy`.
  - `depth_capture.rs:238-242`: `ensure_depth_capture_staging`'s only
    `destroy_depth_capture_staging()` call, on the grow branch.
  - `draw.rs:1628-1640`: `wait_for_fences(&[in_flight[frame], in_flight[prev]], true, u64::MAX)`.
- **Impact**: The recorded justification for an `unsafe` free points at a call site
  that does not exist and mis-states the timing of the one that does. The real
  invariant is the both-slot fence wait — which #3442 already flags as pinned by
  nothing that can see `draw.rs`'s `(f + 1) % MAX_FRAMES_IN_FLIGHT`. So the one
  correct reason is also the one currently unguarded, and this comment points away
  from it.
- **Suggested Fix**: Rewrite the SAFETY block to name the two real callers
  (`ensure_depth_capture_staging`'s grow branch, during recording, and
  `VulkanContext::drop`) and to cite `draw_frame`'s both-slot fence wait as the
  property that makes the recording-time free sound — the same invariant the
  screenshot sibling depends on.

---

---

#### REN-2026-08-30-D6-02: `ParticleEmitter::effect_shader_flags` is the one authored emitter override written *outside* `apply_emitter_overlays`, duplicated byte-for-byte at both spawn sites


- **Severity**: LOW
- **Dimension**: NIFAL Material (particle slice)
- **Location**: `byroredux/src/systems/particle.rs` (`apply_emitter_overlays`), `byroredux/src/scene/nif_loader.rs:627-628`, `byroredux/src/cell_loader/spawn.rs:1074-1075`
- **Status**: OPEN — new (`effect_shader_flags` landed in `70f1bb74`/#2610, after the 2026-08-27 sweep)
- **Description**: `apply_emitter_overlays` is documented as "the **single overlay boundary** that folds every authored emitter override … onto a name-heuristic preset", explicitly so "a newly-wired authored field can no longer silently diverge the two load paths (#1513)". #2610 wired a new authored field — the `BSEffectShaderProperty` payload now carried on both `ImportedParticleEmitter` and `ImportedParticleEmitterFlat` as `effect_shader` — and wrote it into the preset with a hand-copied line at each spawn site instead of routing it through that helper. #3344's sibling `max_particles`, landing in the same delta, *did* go through the helper (it is the 9th parameter), so the two new fields took opposite routes.
- **Evidence**:
  - `apply_emitter_overlays`'s parameter list ends at `max_particles: Option<u32>`; the function body never mentions `effect_shader_flags` (grep: the only `effect_shader_flags` hits in `systems/particle.rs` are zero).
  - `scene/nif_loader.rs:628`: `preset.effect_shader_flags = crate::cell_loader::pack_effect_shader_flags(emitter.effect_shader.as_ref());`
  - `cell_loader/spawn.rs:1075`: `preset.effect_shader_flags = crate::cell_loader::pack_effect_shader_flags(em.effect_shader.as_ref());` — byte-identical modulo the binding name.
  - Each site's comment points at the other ("Mirrored in `cell_loader::spawn::spawn_particle_emitters`" / "see the sibling site in `scene/nif_loader.rs`") — hand-synced duplication, the shape `attach_blend_and_facing_markers` (#2490) was extracted to eliminate for the mesh slice.
  - The two new tests in `render/particles.rs` (`forwards_authored_effect_shader_flags`, `unauthored_effect_shader_flags_stay_zero`) set the field directly on the component; neither exercises either spawn site, so nothing fails if one of the two lines is deleted.
- **Impact**: No behavioural divergence today (the two lines agree). The regression surface is the one #1513 closed for the other four overlays: a future change to how the effect payload is packed — a gate, a merge with `pack_imported_material_flags`, a `None` guard — applied at one site renders the same NIF differently depending on whether it was loaded loose or placed as a REFR, with no test that can see it. Secondary: unlike every other overlay the assignment is unconditional rather than `if let Some(…)`, so it also overwrites rather than overlays (harmless only because all seven presets initialise the field to `0`).
- **Suggested Fix**: Add `effect_shader: Option<&BsEffectShaderData>` (or the already-packed `u32`) as a parameter of `apply_emitter_overlays`, pack inside it, and delete both hand-copied lines. Extend `apply_emitter_overlays_applies_color_rate_size_and_force_fields` and `apply_emitter_overlays_none_inputs_keep_preset_defaults` to cover it, matching how `max_particles` was handled in the same commit range.

---

---

#### REN-2026-08-30-D6-03: the particle boundary drops `MaterialInfo.greyscale_lut_map`, so the two palette bits #2610 now forwards are structurally inert


- **Severity**: LOW
- **Dimension**: NIFAL Material (particle slice)
- **Location**: `crates/nif/src/import/walk/mod.rs` (`extract_particle_material`, `ParticleMaterial`), `byroredux/src/render/particles.rs` (`emit_particles`), `crates/core/src/ecs/components/particle.rs` (`ParticleEmitter`)
- **Status**: OPEN — new (introduced by the #2610 wiring in `70f1bb74`)
- **Description**: `extract_particle_material` builds a full `MaterialInfo` through `extract_material_info_from_refs` and now harvests four things from it — `texture_path`, `src_blend`, `dst_blend`, `effect_shader`. The `effect_shader` payload carries `effect_palette_color` / `effect_palette_alpha`, which `pack_effect_shader_flags` turns into `MAT_FLAG_EFFECT_PALETTE_COLOR` / `MAT_FLAG_EFFECT_PALETTE_ALPHA` on `ParticleEmitter::effect_shader_flags`. But the LUT *texture* those two bits index — available on the very same `MaterialInfo` as `greyscale_lut_map`, and resolved into `MaterialTextureSet::greyscale_lut` for the mesh path — is dropped at this function: `ParticleMaterial` has no field for it, `ImportedParticleEmitter{,Flat}` has no field for it, `ParticleEmitter` has no slot for it, and `emit_particles` hardcodes `greyscale_lut_index: 0`.
- **Evidence**:
  - `struct ParticleMaterial { texture_path, src_blend, dst_blend, effect_shader }` — no LUT role.
  - `crates/nif/src/import/material/mod.rs:468` `pub greyscale_lut_map: Option<FixedString>` and `:1270` `greyscale_lut: self.greyscale_lut_map.or_else(…)` — the role exists and the mesh path consumes it.
  - `render/particles.rs`: `greyscale_lut_index: 0` with the comment "particles never carry the greyscale palette LUT either; the bindless 0 slot signals 'no LUT'".
  - `triangle.frag:862-864` and `:1151-1152` both gate the palette remap on `mat.greyscaleLutIndex != 0u`, so the forwarded bits can never fire on a particle draw.
  - The adjacent `render/particles.rs` comment states this explicitly: "The palette bits stay inert while `greyscale_lut_index == 0`."
- **Impact**: No corruption — the gating is correct, and a bare palette bit on index 0 does not sample texture 0. The gap is a canonical-completeness one: a BGEM/`BSEffectShaderProperty` particle system that authored a greyscale→palette remap (the standard authoring for tinted smoke / energy FX) reaches the GPU with the *instruction* to remap and without the *palette*, so it renders as the un-remapped luminance sprite. Half of #2610's forwarded word is dead by construction, and the comment documenting that is easily read as "particles cannot author a LUT" rather than "we drop it one line above".
- **Suggested Fix**: Carry `greyscale_lut_map` on `ParticleMaterial` → `ImportedParticleEmitter{,Flat}` → a `greyscale_lut` path on `ParticleEmitter`, resolve it with the same `resolve_texture` call both spawn sites already make for the sprite, and forward the handle in `emit_particles` instead of the literal `0`. If the population turns out to be empty on installed corpora, census it and replace the "particles never carry" comment with the measured rate — the current wording asserts a format property that is not true.

---

---

#### REN-2026-08-30-D6-04: `Material::parallax_height_in_alpha` was added to the canonical struct without extending the canonical-completeness harness


- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs` (`canonical_completeness_harness::kitchen_sink_source`, `translate_material_copies_every_canonical_field`)
- **Status**: OPEN — new field, sibling of (not covered by) #3462
- **Description**: The harness's stated contract is that "deliberately reverting any single `source.X` → `material.X` line in `translate_material` fails exactly the corresponding assertion below". #3530 added a 60th `Material` field and its copy line (`parallax_height_in_alpha: source.parallax_height_in_alpha`) without adding it to `kitchen_sink_source()` (where it stays at the `ImportedMaterial::default()` value `false`) or asserting it. Deleting the copy line leaves the harness green.

  This is distinct from #3462: that issue enumerates four fields already uncovered at the 2026-08-27 sweep (`water_shader_flags` / `is_water_shader` at the NIFAL↔WATAL seam, plus two more). `parallax_height_in_alpha` did not exist then. The point is not the field count — it is that a new field shipped through the boundary without the harness being extended in the same commit, which is the failure mode #3462 was filed to stop recurring.
- **Evidence**:
  - Script-checked at HEAD: `Material` declares 60 `pub` fields; all 60 are written in the `translate_material` literal (`material_path` via destructuring shorthand). Ten are absent from the harness assertions; two of those ten (`shader_type_fields`, `effect_falloff`) are covered by multi-line assertions. The remaining eight are `water_shader_flags`, `is_water_shader`, `grayscale_to_palette_scale`, `ior`, `sheen`, `sheen_tint`, `anisotropic`, `parallax_height_in_alpha`. The first four are #3462's; `sheen`/`sheen_tint`/`anisotropic` are deliberate `0.0` literals with no source field (#2514); `parallax_height_in_alpha` is the new gap.
  - `kitchen_sink_source()` sets `texture_clamp_mode: 1`, `src_blend_mode: 2`, `dst_blend_mode: 3` "so the round-trip assertion below actually exercises the copy" but no `parallax_height_in_alpha: true`.
  - Round-tripping *elsewhere* is fine: `crates/nif/src/import/tests/material_texture.rs:282,303` pins the importer-side set/clear, and `byroredux/src/save_io/serde_default_guard_tests.rs:337` pins the `FORMAT_MAJOR` 10 save shape. Only the translate boundary itself is unpinned.
- **Impact**: Test-gap only; the copy is present and correct at HEAD. A future refactor of the `Material` literal that drops the line silently reverts every Oblivion `APPLY_HILIGHT2` mesh to `.r`-channel parallax against a normal map — i.e. sampling the packed normal's red channel as height — with the full workspace suite green.
- **Suggested Fix**: Add `parallax_height_in_alpha: true` to `kitchen_sink_source()` and `assert!(material.parallax_height_in_alpha)` to `translate_material_copies_every_canonical_field`, next to the `#2571` clamp/blend block. Fold the same edit into #3462's fix so the harness closes on all five at once, and consider adding a field-count pin (the same `include_str!`-scan trick `documented_texture_role_list_matches_the_struct` already uses in this file) so the next added field fails the harness rather than slipping past it.

---

## Verified clean (no finding filed)

- **Single boundary.** `translate_material` has exactly **three production callers** at HEAD — `byroredux/src/scene/nif_loader.rs:959`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:634`, `byroredux/src/cell_loader/placement_lod.rs:527`. The checklist's "exactly two" premise is **stale**: `placement_lod` is a legitimate third, added deliberately so Oblivion `_far.nif` sub-meshes route through the boundary instead of spawning without a `Material`, and the module doc was corrected for it under #3465. `cornell.rs:1994` is inside `#[cfg(test)]`. Every other `Material { … }` literal in the workspace is `#[cfg(test)]` (`render/static_meshes.rs:941/1000`, `commands/assets.rs:815`, `crates/save/src/validate.rs:628`, `helpers.rs:167`, `material_translate.rs:1743`) except the `--cornell` synthetic-scene constructors (`cornell.rs:1432-1560`), which have no NIF source to translate. `translate_texture_only_material` is the documented no-source-record sibling and its five call sites are pinned by the `include_str!` harness (now including `terrain_lod_btr.rs`, added under #3336).
- **All 60 `Material` fields are written at the boundary.** Script-verified against the struct declaration; no field is renderer-read-but-never-written, and none is written from a render-time inference.
- **No per-game branch between `Material` and the GPU.** Non-comment grep over `crates/renderer/src` + `crates/renderer/shaders` for every game name / `GameVariant` / `bsver` yields only `STARFIELD_WATER_CONCENTRATION_REFERENCE` — a named unit-normalisation constant re-exported from `byroredux_core::ecs::components::water` and emitted as a GLSL macro, not a branch — plus `volumetrics.rs:3098-3134`, which *asserts* that shader source contains no `"Fallout"` / `"Skyrim"` / `"GameKind"` tokens.
- **PBR resolve-once.** `metalness`/`roughness` are plain `f32`. `resolve_pbr` is NaN-sentinel → `classify_pbr_keyword` → `clamp(0,1)` / `clamp(0.04,1)`, with `resolve_pbr_is_idempotent` plus four sibling tests present. `render/static_meshes.rs:344-368` reads `m.roughness` / `m.metalness` directly and the pre-#1280 render-side glass heuristic is confirmed gone. (`resolve_pbr`'s hardcoded `specular_authored: false` backstop is already #2573.)
- **#3460 regression guard holds.** `Material::{soft_lighting, rim_lighting, back_lighting}` are gone; `pack_imported_material_flags` (`cell_loader.rs:268-276`) is the single derivation into `effect_shader_flags`, and the completeness harness now asserts the packed word (`SOFT_LIGHTING | RIM_LIGHTING | BACK_LIGHTING`) rather than the deleted bools.
- **#3459 regression guard holds.** `DEFAULT_GLASS_REFRACTION_SCALE` / `DEFAULT_GLASS_BLUR_SCALE` are named constants re-emitted as GLSL macros through `shader_constants.rs`, so the shader can no longer carry its own copies of the BGEM v21+ pivots.
- **`EmissiveSource`.** Resolved at the four gated set-sites (three NIF-side, plus the BGEM merge that #3371 gated); the renderer reads only the resolved `emissive_mult` and never the raw per-game property. The field has zero production readers and its doc says so explicitly ("this field is data-plumbing only (#1280 step 4)") — an honest doc, not doc rot.
- **`parallax_height_in_alpha` plumbing** (apart from D6-01/D6-04): set once in the importer's `APPLY_HILIGHT2` arm, copied at translate, defaults `false` for every non-Oblivion producer, transported as bit 31 of `parallaxMapIndex`, and masked (`& ~PARALLAX_ALPHA_HEIGHT_BIT`) in **both** marchers — `material_sampling.glsl:49` (raster) and `ray_hit.glsl:296` (secondary ray) — with `shader_contract_tests.rs:2012-2077` pinning both. The `0.04` / `4.0` scale pair is the pre-existing engine default, not an invented Oblivion constant.
- **Particle override precedence.** `apply_emitter_params` still overrides kinematics/lifetime/size and deliberately not `initial_color` (`apply_emitter_params_overrides_kinematics_and_size_not_color` present); the color curve remains the sole colour owner; `apply_emitter_overlays` still applies each overlay only when authored (`apply_emitter_overlays_none_inputs_keep_preset_defaults`); `render/particles.rs::emit_particles` reads `ParticleEmitter` post-overlay.
- **New particle floors are sourced, not guessed.** `MAX_PARTICLES_CEILING = 256` is justified as the largest value already in the preset table plus a measured FNV census (1,262 `NiPSysData` blocks: min 2 / median 125 / p75 1,604 / p90+ pinned at 10,000); the clamp is `min(authored, ceiling)` with a debug log on truncation and three tests. `extract_emitter_max_particles`'s "first block that carries a budget" walk correctly avoids the 27 other `NiPSys*` types that deserialise to the same marker struct. `sequence_emitter_rate`'s rank ordering (`idle` 0 / `*idle` 1 / named 2 / unnamed 3) with the `<=` skip guard resolves ties to block order as documented.

---

#### REN-2026-08-30-D7-02: two live comments on the dedup path attribute `GpuMaterial` variant slots to `GpuInstance::default`, the struct R1 Phase 6 moved them off


- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `byroredux/src/render/static_meshes.rs:534` and `:574` (`collect_static_mesh_draws`)
- **Status**: OPEN
- **Description**: The Skyrim+ `BSLightingShaderProperty` variant-payload block justifies its `material_kind`-gated zero fallbacks with "`GpuInstance::default` already zeroes the slots" (`:534`) and "the slot stays zeroed exactly as `GpuInstance::default` leaves it" (`:574`, added in the `#2602` hair-tint change in this sweep's delta). `GpuInstance` carries no such fields. Its full field list is `model, texture_index, bone_offset, vertex_offset, index_offset, vertex_count, flags, material_id, ior, avg_albedo_r/g/b, surface_id, skinned_vertex_address, _reserved, morph_delta_address, morph_weight_address, morph_target_count, _reserved2a/b/c` (`scene_buffer/gpu_types.rs:95`). `skin_tint_*`, `hair_tint_*`, `sparkle_*`, `eye_*` and `multi_layer_*` are all `GpuMaterial` fields (`material.rs:141-176`) with zero defaults in `GpuMaterial::default()` (`material.rs:433-435` for `hair_tint_*`); they were collapsed onto the material table by R1 Phase 6 and are explicitly named on the ban list in `gpu_instance_does_not_re_expand_with_per_material_fields` (`gpu_instance_layout_tests.rs:180`).
- **Evidence**: `grep -n "GpuInstance::default" byroredux/src/render/static_meshes.rs` → lines 534, 574. `grep -n "hair_tint" crates/renderer/src/vulkan/material.rs` → declared at 153-155, defaulted at 433-435, hashed at 1068-1070. No `hair_tint` / `skin_tint` / `sparkle` / `eye_` identifier exists in `pub struct GpuInstance`.
- **Impact**: Documentation only — the code is correct (the zeroes it writes are the `GpuMaterial::default()` values, so the neutral-output claim holds). But the comment is a per-instance/per-material attribution error sitting on the exact function that decides what goes into the dedup key, in the same file that already carries a `#3465` note about naming call sites by symbol. It invites a reader to look for the fallback on the wrong struct, or to conclude these are per-instance slots that could be re-widened.
- **Suggested Fix**: Change both references to `GpuMaterial::default` and, at `:534`, note that the material-table record — not the instance record — is what carries the variant payload after R1 Phase 6.

---

---

#### REN-2026-08-30-D7-03: the dedup hash's own doc understates its field count by 16, and the intern call site quotes a superseded `GpuMaterial` size


- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs:998` (`hash_gpu_material_fields` doc), `byroredux/src/render/static_meshes.rs:886` (`intern_by_hash` call site)
- **Status**: OPEN
- **Description**: Two stale numerics on the R1 dedup hot path:
  - `material.rs:998` — "Canonical material hash — FxHash (#1368) over the **92 live scalar fields** of `GpuMaterial` in declaration order." The struct declares 108 scalar fields and the function hashes all 108.
  - `static_meshes.rs:886` — "`intern_by_hash` skips the `to_gpu_material()` **364-byte** construction on the dedup-hit path". `GpuMaterial` has been 432 B since the 2026-08-25 BGEM-glass-optics + Bethesda-lighting growth; 364 B was the `#2221` intermediate, and the size history on `material.rs:40` records the two later steps (396 B, 432 B).
- **Evidence**: `awk` over the `pub struct GpuMaterial` body yields 108 `pub <ident>:` fields; 108 × 4 B = 432 B, matching `gpu_material_size_is_432_bytes` (`material.rs:1494`, passing). The same extraction over the `hash_gpu_material_fields` body yields 108 distinct `mat.<field>` identifiers. `grep -rn "364-byte\|92 live scalar" crates/renderer/src/vulkan/material.rs byroredux/src/render/static_meshes.rs` returns exactly these two sites.
- **Impact**: The "92 fields" line is the doc a reader consults before extending the walk — the point at which under-counting is most likely to become the D7-01 bug. `#1368`/`#2273` already established the convention of pointing at `gpu_material_size_is_432_bytes` instead of restating a drifting field count (see `intern_by_hash`'s collision-policy paragraph, `material.rs:1332`); this doc predates that convention and never got converted.
- **Suggested Fix**: Replace "the 92 live scalar fields" with a reference to `gpu_material_size_is_432_bytes` (or to the field-coverage guard proposed in D7-01, once it exists) rather than a fresh literal; update `static_meshes.rs:886` to 432 B, or drop the byte figure and say "the full `GpuMaterial` construction".

---

## Verified clean (no finding)

- **`intern` stability + collapse**: `MaterialTable::clear()` runs once at the top of `build_render_data` (`byroredux/src/render/mod.rs:666`), before both intern sites, and `app_frame.rs` calls `build_render_data` unconditionally ahead of `draw_frame` in the same block — no path uploads a stale table against fresh `draw_commands`. `materials` is private and `materials()` hands out `&[…]`, so no post-intern mutation can break the identity invariant.
- **Over-cap path**: `MAX_MATERIALS = 16384` (`scene_buffer/constants.rs:192`); `intern_by_hash` returns `0` (the `seed_neutral_default` slot) and warns once through `INTERN_OVERFLOW_WARNED`, whose message names `ctx.scratch` — the command that actually exists (`byroredux/src/commands/world_info.rs:157`). `mem.stats` / `mem` are correctly absent. Pinned by `intern_overflow_returns_material_zero` and `intern_overflow_persists_across_clear`.
- **SSBO sizing**: `upload_materials` (`scene_buffer/upload.rs:640`) `debug_assert!`s `len <= MAX_MATERIALS`, takes `count = len.min(MAX_MATERIALS)`, and hashes/uploads only `materials[..count]`.
- **Dedup-ratio telemetry (#780)**: `app_frame.rs:174-176` publishes `unique_user_count()` / `interned_count()` / `overflow_count()` into `ScratchTelemetry` every frame; `ctx.scratch` prints `N unique / M interned (R× dedup)` plus an `OVERFLOW` suffix. `unique_user_count()` correctly excludes the seeded slot 0.
- **Hash/Eq byte contract**: 108 scalar fields, no pad fields, 432 B exactly — the zeroed-pad invariant is vacuously satisfied because there is no pad. Field/offset/GLSL-order pins all pass.
- **`DrawCommand::material_hash` ↔ `hash_gpu_material_fields` lockstep**: verified field-for-field including the `supplemental_texture_indices[..12]` loop; slot constants `TINT..DECAL_3` are 0..11 in the same order the `GpuMaterial` declaration uses, and slots 12–15 (`GLASS_ROUGHNESS_SCRATCH`, `GLASS_DIRT_OVERLAY`, `LIGHTING_MASK`, `BACK_LIGHTING`) are each hashed explicitly. All 16 of 16 slots covered. `to_gpu_material` populates all 108 fields — none left at a constant.
- **Particle fade quantization (#1795)**: `quantize_fade` / `COLOR_FADE_STEPS = 32` survived the delta intact (`byroredux/src/render/particles.rs:39-48`); `color_t = quantize_fade(t)` drives the RGBA color LERP and `material_alpha`, while the size LERP still uses the continuous `t` (`particles.rs:130-146`). Spawn color is `em.start_color` for every particle in an emitter (`byroredux/src/systems/particle.rs:479`), so the snap genuinely collapses an emitter onto ≤32 materials. The delta's only new `GpuMaterial` input on this path — `effect_shader_flags: em.effect_shader_flags` (#2610) — is per-**emitter**, not per-particle, and is constant across a frame; `particle_roll` (`#`-hashed per particle) feeds only the model matrix. No new unquantized per-particle varying field.
- **`#3530` parallax alpha-height bit**: `parallax_map_index |= PARALLAX_ALPHA_HEIGHT_BIT` (`static_meshes.rs:310`, gated on a non-zero index) lands in a hashed `GpuMaterial` field, so the two variants dedup distinctly and correctly. `0x8000_0000` is masked in every GLSL reader and never used as a Rust-side array index — `grep` of `parallax_map_index` shows no host-side indexing.
- **Import-side regression guards**: BGSM smoothness is normalized once — `roughness_override = 1 - smoothness` (`byroredux/src/asset_provider/material.rs:1358`) and `resolve_pbr` (`crates/core/src/ecs/components/material.rs:1165`) runs the `glossiness` classifier only when metalness/roughness are still `NaN`, so no double-apply (#1241). Water surfaces are excluded from the triangle path entirely by the post-sort `is_water` flip (`byroredux/src/render/water.rs:139`), so their material entry can't collapse with glass/opaque in a way that reaches the GPU (#1243/#1244). `MODEL_SPACE_NORMALS` is packed into `material_flags` by `pack_imported_material_flags` (`byroredux/src/cell_loader.rs:261`) and `material_flags` is hashed (#972).
- **Per-instance/per-material split**: `GpuInstance` retains only `texture_index`, `ior` and `avg_albedo_*` as material-adjacent data, each with a written rationale (UI-quad path, caustic-pass descriptor set) and each consistent with the interned record; `gpu_instance_does_not_re_expand_with_per_material_fields` bans the 28 R1-Phase-6 fields from creeping back.
- **Unsampled-but-hashed fields**: `eye_*`, `multiLayerRefractionScale`, `lightingMapIndex`, `flowMapIndex`, `wrinkleMapIndex`, `shaderColor*`, `shaderFloat` are declared in `include/bindings.glsl` but never read. All are documented deferrals (#2712, #2221) and all except the `shaderColor`/`shaderFloat` pair are static per material, so they cost no dedup hit-rate. The animated pair is the already-open **#3246** and is not re-filed.

---

#### REN-2026-08-30-D8-04: two transcription defects in the code #3426 relocated — a mangled `const` assertion message and a warn-once that now misdiagnoses a second failure mode


- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/presentation.rs:645-651` (`_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`); `crates/renderer/src/vulkan/context/post_passes.rs:1095-1104` (the `overlay.is_none() && ui_instance_idx.is_some()` warn-once)
- **Status**: OPEN — introduced by commit `b28acb0c` (#3426)
- **Description**: Two independent nits, both artefacts of moving the block out of
  `geometry_pass.rs`:
  1. The assertion message lost its string line-continuation backslash in the
     move. The literal now reads
     `"UI overlay path covers VIEWPORT + SCISSOR only —                  extend it before growing UI_PIPELINE_DYNAMIC_STATES"`
     — 18 literal spaces mid-sentence. The `geometry_pass.rs` original had
     `only — \` + indentation, which the compiler folded away.
  2. The relocated warn-once fires on a strictly wider condition than the message
     describes. In `geometry_pass.rs` it was nested inside
     `if let Some(mesh) = self.mesh_registry.get(ui_quad)`, so "global-only" was
     the only reachable cause. The new site tests
     `overlay.is_none() && ui_instance_idx.is_some()`, which is also true when
     `self.mesh_registry.get(ui_quad)` returns `None` (handle not in the
     registry) — a different failure that would be reported as
     `"UI overlay quad has no per-mesh vertex/index buffer (global-only)"`. The
     surrounding comment already enumerates the three causes
     ("no UI texture this frame, no registered quad, or a quad with no per-mesh
     buffers"); only the log message was not widened. `ui_quad_handle == None` is
     genuinely unreachable here because `draw.rs:3242` gates `ui_instance_idx` on
     it, so the widening is by exactly one case.
- **Evidence**: `sed -n '645,651p' crates/renderer/src/vulkan/presentation.rs`;
  `git diff 969d81c8..HEAD -- crates/renderer/src/vulkan/context/geometry_pass.rs`
  shows the original `only — \` continuation; `draw.rs:3241-3252` shows the
  `ui_instance_idx` gate.
- **Impact**: Cosmetic in both cases — a compile-time panic string nobody has hit,
  and a once-per-process warning that would name the wrong of two adjacent causes.
  No runtime behaviour change.
- **Suggested Fix**: Restore the `\` continuation in the assertion message; make
  the warn text cover both causes (e.g. "UI overlay quad is unavailable (not in
  the mesh registry, or global-only with no per-mesh vertex/index buffer)"), or
  split the `mesh_registry.get` miss into its own arm.

---

---

#### REN-2026-08-30-D8-05: #3426's exact-colour-round-trip argument is premised on an sRGB swapchain that `choose_surface_format` does not guarantee and does not log when it misses


- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:163-173` (`choose_surface_format`); premise stated at `crates/renderer/src/vulkan/presentation.rs:98-102` and `crates/renderer/src/vulkan/pipeline.rs:943-945`
- **Status**: OPEN — observation, no live impact on any supported device
- **Description**: #3426 added an explicit correctness argument for the overlay's
  colour handling: *"Ruffle's capture is sRGB-encoded bytes uploaded as
  `R8G8B8A8_SRGB`, so the sampler linearises it; Vulkan blends in linear space
  against the sRGB swapchain attachment and re-encodes on write."* Every step of
  that chain verifies — Ruffle's `TextureTarget` is `Rgba8Unorm` and Flash colours
  are authored in gamma space (`render/wgpu/src/target.rs:201` and the comment at
  :66-70), `capture_frame` un-premultiplies to straight alpha
  (`render/wgpu/src/utils.rs:174`, matching the pipeline's
  `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`), and `Texture::from_rgba` uploads
  `R8G8B8A8_SRGB` (`vulkan/texture.rs:88`). The one link the argument asserts
  rather than establishes is the last: `choose_surface_format` prefers
  `B8G8R8A8_SRGB` + `SRGB_NONLINEAR` but falls back to `formats[0]` with no
  warning, and the presentation render pass takes whatever
  `swapchain_state.format.format` gives it. On a surface without that pair the
  hardware sRGB encode disappears and the overlay's read-linearise/write-encode
  round trip stops cancelling.
- **Evidence**: `swapchain.rs:163-173` — `.find(|f| f.format == B8G8R8A8_SRGB && f.color_space == SRGB_NONLINEAR).unwrap_or(formats[0])`, no `log::warn!` on the fallback arm; `presentation.rs:255-263` — the colour attachment is built from that format.
- **Impact**: None observed. `B8G8R8A8_SRGB` + `SRGB_NONLINEAR` is present on
  every desktop driver the project targets, and if it were ever missing the whole
  frame (not just the overlay) would be mis-encoded, so the overlay is not the
  first thing that would break. Filed because #3426 turned an implicit assumption
  into a documented invariant without adding anything that enforces or reports it.
- **Suggested Fix**: One-line `log::warn!` on the `unwrap_or(formats[0])` arm
  naming the chosen format and the colour-space consequence. A hard failure would
  be over-reach; a silent fallback under a documented invariant is the gap.

---

## Verified clean (no finding)

Checked against current code at `64f64480`; each of these was a checklist item or
a plausible regression from the #3426 restructure, and each holds.

**SVGF temporal** (`shaders/svgf_temporal.comp`)
- Ping-pong is read-prev / write-current: bindings 3/4/5 are the previous FIF
  slot's mesh-ID, indirect history and moments; 6/7 are this slot's writes.
  `MAX_FRAMES_IN_FLIGHT >= 2` is `const`-asserted (`svgf.rs:71-83`).
- Motion-vector convention matches the producer exactly. `triangle.frag:546`
  writes `outMotion = (currNDC - prevNDC) * 0.5`; with `uv = ndc*0.5 + 0.5` on
  both axes that is `currUV - prevUV`, and the shader reprojects
  `prevUV = uv - motion` (line 122). The pixel-space fallback gate converts via
  `length(motion * screen.xy)` before comparing to 1.5.
- Mesh-ID disocclusion rejection is correct and uses the stable representation.
  `include/mesh_id.glsl` never masks bit 31 before comparing (`stableMeshIdsMatch`
  requires both operands to lack the bit *and* be equal), so an alpha draw index
  can never alias an opaque surface ID. `triangle.frag:559-564` packs
  `inst.surfaceId & 0x7FFFFFFF` for opaque and `(fragInstanceIndex + 1) & 0x7FFFFFFF | 0x80000000`
  for alpha-blended, exactly as commit `883f57cd` specified. `surface_id` is
  `entity_id.wrapping_add(1)` (`draw.rs:3081`), so 0 stays reserved for
  sky/clear and no real opaque surface can collide with the background sentinel.
- The `#650`/SH-5 normal-cone rejection (0.9 cosine) is applied in both the 2×2
  bilinear loop and the sub-pixel nearest-tap fallback.
- **Firefly clamp is hoisted ahead of the `hasHistory` branch** (lines 258-279,
  before line 281) — the REG-07 / #1639 / #1481 regression guard holds, and the
  no-history disocclusion path at line 312 writes the clamped `currInd`.
- First frame uses current only: `params.z` (`first_frame_flag`) gates
  `reprojectOk` before any history fetch; `should_force_history_reset` drives it
  from `frames_since_creation`.
- Blend alpha is bounded: `alphaC = max(floorC, 1/(histAge+1))` with `histAge`
  capped at 255 on write; `#903` NaN/Inf taps are dropped before the weighted sum.
- Dispatch covers exactly the image: `width.div_ceil(WORKGROUP_X)` /
  `height.div_ceil(WORKGROUP_Y)` (`svgf.rs:1303-1305`) with a matching in-shader
  bounds early-out.

**SVGF à-trous** (`shaders/svgf_atrous.comp`, `svgf.rs`)
- `ATROUS_ITERATIONS` is `const`-asserted odd so the final iteration lands in
  ping-pong slot 0, which is what `indirect_view` hands composite;
  `atrous_dst_pp(k) = k % 2` / `atrous_src_pp(k) = (k-1) % 2` are consistent.
- Same `stableMeshIdsMatch` hard rejection; `octDecode` is byte-identical to the
  temporal pass's copy.

**Composite reassembly** (`shaders/composite.frag`)
- `grep -n "aces(" composite.frag` → no match; `grep -l "aces(" shaders/*` → only
  `presentation.frag`. The tone-map did **not** move back, and #3426 did not touch
  it.
- Reassembly is `combined = direct + indirect * albedo + caustic` (line 662), with
  the sky branch compositing behind rather than replacing (`#2466`).
- The caustic accumulator is a `usampler2DArray` read, divided by
  `CAUSTIC_FIXED_SCALE`, promoted to float before the water/glass add (`#1575`),
  firefly-capped, and added to `combined` as its own term — never folded into the
  SVGF-denoised indirect. The `#2508` double-count gate on the fallback-bound
  water view is intact.
- Bloom is still *not* added here (`bloomTex` binding 7 declared and unused per
  `#2796` / REN-D16-01); `bloom.rs::apply_to_scene` runs downstream on composite's
  own output, upstream of the tone map. Ordering unchanged by #3426.
- Composite writes the offscreen HDR image: `HDR_FORMAT = R16G16B16A16_SFLOAT`,
  `final_layout = SHADER_READ_ONLY_OPTIMAL` (`composite.rs:534-541`). It does not
  touch the swapchain. `presentation.rs:255-263` owns the swapchain attachment and
  its `UNDEFINED → PRESENT_SRC_KHR` transition.

**Alpha-blend aux-MRT alpha lanes** (`shaders/triangle.frag`)
- The `883f57cd` guard holds at the tail: `auxiliaryAlpha = isAlphaBlend ? finalAlpha : 1.0`
  written into both `outRawIndirect.a` and `outAlbedo.a` (lines 4079-4082).
- The RT-terminus glass exit uses `resolvedAlpha` on both (2363-2364) and the
  framebuffer-transmission/portal exit uses `portalAlpha` (1703-1704).
- The two remaining hardcoded `vec4(albedo, 1.0)` sites (2141, 2389) are both
  inside `DBG_VIZ_GLASS_PASSTHRU` diagnostic returns — debug oracles, correctly
  exempt. The effect/emissive and `MATERIAL_KIND_NO_LIGHTING` exits write
  `alpha = 0.0` deliberately ("Alpha zero preserves the opaque receiver's
  auxiliary G-buffer state in blended pipelines"), and no consumer reads
  `albedoTex.a` / `indirectTex.a` — composite samples `.rgb` on both.

**#3426 presentation-tail restructure**
- UI is composited **after** tone-mapping, intentionally and consistently. The
  overlay is authored in display space (Flash colours in a `Rgba8Unorm` Ruffle
  target), read back straight-alpha, uploaded `R8G8B8A8_SRGB`, and blended
  `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA` against the sRGB swapchain — the round trip
  cancels (see D8-05 for the one unenforced link).
- Blend state is correct for a non-HDR target: exactly one colour-blend
  attachment, matching the pass's one colour attachment; `ui.frag` declares only
  `location = 0`, so there is no MRT mismatch. `dst_alpha = ZERO` writing the
  overlay's own alpha into the swapchain is harmless — `composite_alpha` is
  `OPAQUE` (`swapchain.rs:108`).
- Both descriptor sets are rebound before the overlay draw (`record_overlay`
  binds `overlay.texture_set` + `overlay.scene_set` at
  `overlay_pipeline_layout`), which is required because the tone-map draw bound
  this pass's layout-incompatible set 0. Viewport/scissor are re-set for the same
  reason the old site did.
- Resolution: `FrameExtentSet::for_output` sets `output = swapchain_state.extent`
  (`upscaling.rs:190-233`, `init.rs:250`), and the Ruffle texture is created at
  `ctx.swapchain_extent()` — so the overlay now draws 1:1 against its texture
  instead of at `frame_extents.render` and being FSR-upscaled. #3426 removed a
  resolution mismatch rather than introducing one.
- Lifetimes: `overlay_pipeline_layout` is borrowed, not owned —
  `PresentationPipeline::destroy` deliberately does not destroy it; `recreate`
  reads it out *before* the `destroy` call; `Drop for VulkanContext` calls
  `presentation.destroy()` at `teardown.rs:186`, well before
  `destroy_pipeline_layout` at `:317`. `resize.rs` reuses `self.pipeline_layout`
  across the rebuild (`recreate_triangle_pipelines` takes it), so the field never
  dangles, and the presentation pipeline is recreated unconditionally
  (`resize.rs:1007-1050`).
- `record_presentation_pass` stays error-propagation-free after the SVGF latch
  (`#2146` / `#917`): the overlay is assembled with `zip`/`and_then`/`map`, no `?`.

**Not filed — needs RenderDoc, not a code change**
- The presentation render pass's incoming `SUBPASS_EXTERNAL` dependency names
  `FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT` in its dst scope, and #3426 added
  reads at `VERTEX_INPUT` (the UI quad's VB/IB) and `VERTEX_SHADER` (the instance
  SSBO) inside that pass. Traced both: the quad's buffers are uploaded once at
  startup on a fenced transfer submit, and the instance SSBO is host-written and
  flushed before `vkQueueSubmit`, whose implicit host-write domain operation
  covers every device stage in the submission. No hazard, and no barrier change is
  warranted on static reading alone — consistent with the `#2465` /
  REN-D4-2026-08-07-01 note already in `presentation.rs:292-326`, which measured
  this pass clean over 300 frames under `BYRO_VALIDATION=1`.

---

#### REN-2026-08-30-D9-02: `SkinComputePipeline::dispatch`'s SAFETY comment still describes the pre-#3231 12-byte push block and names a test that no longer exists


- **Severity**: LOW (doc-rot)
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:680-684` (`SkinComputePipeline::dispatch`)
- **Status**: OPEN — new
- **Description**: The SAFETY justification for the `std::slice::from_raw_parts` that builds the push-constant byte view asserts a struct shape that #3231 changed 3 fields ago, and cites a test name that is not in the tree. `SkinPushConstants` (`skin_compute.rs:48-74`) is now `u64, u64, u32, u32, u32, u32` = 32 B; the live pin is `push_constants_size_is_32_bytes` (`skin_compute.rs:1177`). The sibling palette dispatch's comment (`skin_compute.rs:1029-1031`) is accurate, which makes the drift a local one rather than a house-style issue.
- **Evidence**:
  - `skin_compute.rs:680-684`: `// SAFETY: `SkinPushConstants` is `repr(C)` with three u32 fields, / 12 bytes, no interior padding. … mismatched shape is caught by `push_constants_size_is_12_bytes` test).`
  - `grep -n "push_constants_size_is" crates/renderer/src/vulkan/skin_compute.rs` → only `push_constants_size_is_32_bytes` (1177) and `skin_palette_push_constants_size_is_4_bytes` (1524). No `_12_bytes` test exists.
  - `skin_compute.rs:1177-1187` asserts `PUSH_CONSTANTS_SIZE == 32` and `size_of::<SkinPushConstants>() == 32`.
- **Impact**: No runtime effect — the code takes `PUSH_CONSTANTS_SIZE`, not the literal, and the shader block (`skin_vertices.comp:92-110`) matches at 32 B. The cost is that the SAFETY argument on an `unsafe` block is unverifiable as written and points a reader at a non-existent guard, exactly the failure mode the "verify the premise" rule exists for. A future editor checking the invariant finds nothing and may conclude it is unpinned.
- **Suggested Fix**: Rewrite the comment to describe the current layout (two `u64` at offsets 0/8, four `u32` at 16/20/24/28, 32 B, no interior padding) and cite `push_constants_size_is_32_bytes`. One-line change; no test needed beyond the one already there.

---

## Verified clean

Checked at HEAD, premise confirmed against current code, nothing to file:

- **#3469 cached device address — the brief's highest-value check: SAFE.** `SkinSlot::output_buffer` has exactly one producer (`create_slot`, `skin_compute.rs:499`) and one consumer-of-ownership (`destroy_slot`, `skin_compute.rs:591`). `grep -rn "output_buffer *=\|output_address *="` across `crates/renderer/src` + `byroredux/src` finds no in-place reassignment anywhere — the buffer is never grown, shrunk or recreated under a live slot. The only "resize" path is the #1297/#1298 capacity reconciliation (`skinned_blas_refit.rs:228-267`), which `remove`s the slot, routes it through `destroy_slot`, drops the paired BLAS and lets `create_slot` build a *new* `SkinSlot` — so the cache is reconstructed with the buffer, not invalidated separately. `resize.rs:1154` touches only `last_used_frame`. The stale-address class this optimization risks is therefore structurally unreachable. Both source-scan guards (`draw_frame_resolves_no_buffer_device_addresses`, `cached_skin_address_read_stays_behind_the_backing_filter`) pass, and the `#2402` `skin_slot_backs_mesh` filter still precedes the read at `draw.rs:3010-3029`. The sibling `SkinnedBlasGeometry.vertex_address` (`acceleration/types.rs:38`, fed from `slot.output_address()` at `skinned_blas_refit.rs:653`) inherits the same lifetime; the index and scratch addresses are deliberately left as per-call queries with the reasoning recorded at `blas_skinned.rs:535-543`, which is the correct call given `blas_scratch_buffer` has three realloc sites.
- **`VERTEX_STRIDE_FLOATS = 26`** (`shader_constants_data.rs:64`), imported (not hardcoded) at `skin_compute.rs:27`, pinned against `size_of::<Vertex>()` by `vertex_stride_matches_rust_vertex_size` (`skin_compute.rs:1096-1103`, also value-pins `VERTEX_STRIDE_BYTES == 104`). `SKIN_OUTPUT_STRIDE_FLOATS = 3` / `_BYTES = 12` pinned at 1119-1141, and `skinned_blas_build_sites_use_the_narrowed_stride` (1155-1165) blocks a `size_of::<Vertex>()` from creeping back into the AS-build stride.
- **`skin_palette.comp` ordering + workgroup**: palette dispatch at `draw.rs:2536-2597` precedes `record_skinned_blas_refit` (which owns the `skin_vertices.comp` dispatch); both shaders take `local_size_x = SKIN_WORKGROUP_SIZE` from the generated header, pinned by `skin_palette_workgroup_size_matches_skin_vertices`; group count is `vertex_count.div_ceil(WORKGROUP_SIZE)` (`skin_compute.rs:697`).
- **SPIR-V drift**: `skin_palette.comp`'s source commit (`fd483a2f`, 2026-06-26) is newer than its `.spv`'s, but that commit only swapped the literal `64` for the `SKIN_WORKGROUP_SIZE` define and its own body records the recompile as byte-identical. Not a drift.
- **Push-constant contract**: `SkinPushConstants` = 32 B ≤ 128 B, field order matches `skin_vertices.comp:92-110` (`u64`s first for 8-byte alignment); `SkinPalettePushConstants` = 4 B matches `skin_palette.comp:65-71`.
- **Output-buffer usage flags** (`skin_compute.rs:503-506`): `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. `VERTEX_BUFFER` correctly absent per `b99ae91e` / #681 — not reported, per the brief.
- **Barrier chain**: `COMPUTE_SHADER/SHADER_WRITE → (ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER)/SHADER_READ` at `skinned_blas_refit.rs:483-491` (the `#2403` FRAGMENT widening is intact); palette `SHADER_WRITE → SHADER_READ` over `COMPUTE_SHADER | VERTEX_SHADER` at `draw.rs:2578-2594`.
- **Scratch-serialize barrier (#1790 regression guard)**: `record_scratch_serialize_barrier` (`blas_skinned.rs:687-705`) still uses `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR` as the dst mask. **No regression.**
- **Refit correctness**: `validate_refit_counts` (`predicates.rs:129-144`) rejects on `!=` for both vertex and index counts (VUID-…-03667); `validate_refit_flags` pins `SKINNED_BLAS_FLAGS` against `built_flags`; `SKINNED_BLAS_REFIT_THRESHOLD = 600` (`acceleration/constants.rs:68`) matches `docs/engine/memory-budget.md:408-409`.
- **LRU / in-flight pinning**: eviction threshold is `MAX_FRAMES_IN_FLIGHT + 1` (`skinned_blas_refit.rs:726`), `should_evict_skin_slot`'s `last_used_frame == 0` sentinel protects a slot created mid-frame, `resize.rs:1153-1162` rebases both slot maps' stamps when `frame_counter` is zeroed (#2925), and `drop_skinned_blas` routes through `pending_destroy_blas` with a FIF countdown.
- **Bone-palette overflow guard**: `SkinSlotPool::allocate` returns `None` at capacity with a `overflow_warned`-gated single WARN plus a silent `overflow_attempt_count` (`skin_slot_pool.rs:172-186`); `bone_palette_overflow_tests.rs` pins both the at-capacity ceiling and the over-capacity drop, and `upload.rs:401-407` carries the `(slot_id + 1) * MBPM <= MAX_TOTAL_BONES` copy-destination assert.
- **#3374 morph eviction lock scope**: the `MorphSlot` drain sits outside the `(skin_compute, accel_manager)` guard at `skinned_blas_refit.rs:774-820`, with `morph_eviction_drain_sits_outside_the_skin_compute_accel_guard` passing. **No regression.**
- **FxHash hot-path rule (#2923 / #2985 / #3061)**: `pose_dirty_crosses_the_crate_boundary_without_siphash` (`context/mod.rs:2874-2960`) covers `FrameInputs.pose_dirty`, `record_skinned_blas_refit`'s parameter, `skin_slots`, `morph_slots`, `failed_skin_slots`, `failed_skin_blas` and the two scratch sets; the new `byroredux/src/render/skin_offsets_hasher_tests.rs` adds `skin_offsets` across its 4 declaration sites and self-pins its own extraction. All green.
- **`morph_slot_backs_mesh` gating** is applied on both the raster publish (`draw.rs:3038-3053`) and the compute dispatch (`skinned_blas_refit.rs:133-153`) — no split decision.
- **`MorphSlot::stage_weights` length contract**: only caller (`byroredux/src/render/skinned.rs:289-291`) builds exactly `slot.target_count()` entries, so the `debug_assert_eq!` is not load-bearing in release.
- **Bind-inverse cap accounting**: `drain_pending(MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME)` leaves the remainder in `pending_uploads`, so `upload_pending_bind_inverses`'s own `min(cap)` can never silently truncate a batch. (The *failure* path is D9-01 above; the *cap* path is correct.)

Test runs: `cargo test -p byroredux-renderer --lib skin` → 54 passed, 0 failed. `cargo test -p byroredux --bin byroredux` → 1647 passed, 0 failed.

---

#### REN-2026-08-30-D10-03: `GpuCamera`'s own rustdoc header still says 352 bytes and contradicts the test it names two lines later


- **Severity**: LOW
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera`, L335-343)
- **Status**: New (distinct site from #3450 / #3447 — this is the struct's own doc, not a SKILL file or `shader-pipeline.md`)
- **Description**: The header line reads
  `/// GPU-side camera data (**352 bytes**, std140-compatible).` while the very
  next paragraph says `Layout pinned by \`gpu_camera_is_368_bytes\` test — three
  \`mat4\` … + eleven trailing \`vec4\` … → 368 B`. The size-history sentence
  also terminates at `336 → 352 B with the structured renderer-debug control`
  and never records the `352 → 368 B` step that `exterior_sky_tint` (#3323)
  added. `GpuCamera.render_origin` — this dimension's primary entry point — is
  documented inside that same block, so anyone arriving here to reason about
  the render-origin contract meets a self-contradicting header first.
- **Evidence**:
  - `gpu_types.rs:335` — `/// GPU-side camera data (**352 bytes**, std140-compatible).`
  - `gpu_types.rs:337` — `/// Layout pinned by \`gpu_camera_is_368_bytes\` test`
  - `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66` —
    `fn gpu_camera_is_368_bytes()` asserting `size_of::<GpuCamera>()`
  - The field list in the doc already enumerates eleven `vec4`s including
    `exterior_sky_tint`, so only the headline number and the history sentence
    are stale.
- **Impact**: Documentation only — the layout itself is pinned by a passing
  test and by `reflect.rs:608`'s SPIR-V size cross-check. But it is the third
  independent site now carrying the stale 352, and #3450 / #3447 were filed
  against the other two; leaving the authoritative one wrong is what keeps
  re-seeding the copies.
- **Suggested Fix**: One-line edit: `(**368 bytes**, std140-compatible)`, and
  extend the history sentence with `then 352 → 368 B with \`exterior_sky_tint\`
  (#3323)`. Consider closing it out together with #3450 / #3447.

---

## Verified clean (not re-filed)

**Two-convention split — no mixing found.**
- Raster is render-origin-RELATIVE end to end. Rigid: `rebase_model_matrix`
  (`context/draw.rs:4059-4071`) subtracts `render_origin` from `model[12..14]`
  for both the current and previous model (`draw.rs:2843`, `:2853`), pinned by
  `current_and_previous_rigid_models_share_current_render_origin`
  (`draw.rs:4155`). Skinned: `triangle.vert:238-239` still does
  `xform[3].xyz -= renderOrigin.xyz; xformPrev[3].xyz -= renderOrigin.xyz;`
  (#1486) so both branches project in the same relative space. DOF's
  `look_at_rh(jittered_eye - render_origin, focal_pt - render_origin, up)`
  (`draw.rs:636`) keeps the DOF eye in the same space as the pinhole one.
- RT stays ABSOLUTE. `acceleration/tlas.rs:575-577` documents and uses the
  absolute model matrix (and the already-absolute skinned bone palette);
  `include/ray_hit.glsl:135-137` lifts the relative `hi.model` back with
  `+ renderOrigin.xyz` on all three triangle vertices.
- `triangle.frag:137` reconstructs `fragWorldPos = fragWorldPosRel +
  renderOrigin.xyz` at the top of `main()`; `cameraPos.xyz` is absolute per
  `GpuCamera::position`'s doc and is rebased locally where a relative value is
  needed (`triangle.frag:979`, `camRel = cameraPos.xyz - renderOrigin.xyz`).

**No derivative consumer regressed onto the absolute varying** (the brief's
HIGH-floor check). Exhaustive `dFdx|dFdy|fwidth` sweep over `*.vert`, `*.frag`
and `include/*.glsl`: the only position-derivative sites are
`triangle.frag:177` (flat-shading normal), `:185`, `:788` (rtLOD footprint),
`:511`/`:527` (`perturbNormal`), `:236` (`parallaxDisplaceUV`) — every one
passes `fragWorldPosRel`. `include/material_sampling.glsl:62-65` and `:217-220`
take `worldPos` as a parameter and have exactly one call site each, both
relative. `include/pbr.glsl:321-322` differentiates `N`, not a position.
The commit-`19813460` Oblivion `APPLY_HILIGHT2` parallax delta to
`material_sampling.glsl` (+31) is purely a height-channel selector
(`PARALLAX_ALPHA_HEIGHT_BIT` mask + `sampleParallaxHeight`) and adds no
position-dependent code; the `ray_hit.glsl` (+33) half is the same change on
the RT side and correctly masks the bit off the bindless index.

**The new analytic functions are mathematically correct.** Derivation checked
against `glam::Mat4::perspective_rh` (which this project's `projection_matrix`
uses, with only a Y-flip): `z_clip = r·z_eye + r·n`, `w_clip = −z_eye`,
`r = f/(n−f)`; with `z_eye = −d` this gives
`z_ndc(d) = f/(f−n)·(1 − n/d)` — exactly the doc's formula, `0` at `d = n` and
`1` at `d = f`. `dz/dd = f·n/((f−n)·d²)`, so inverting one f32 step gives
`Δd = ulp·(f−n)·d²/(f·n)`, which is literally
`ulp * (f - n) * distance * distance / (f * n)` at `camera.rs:223`. Reversed:
`z_ndc(d) = (n/d − n/f)/(1 − n/f)` is `1` at `d = n`, `0` at `d = f`, with the
same slope magnitude — correct, and taking the ulp at the *encoded* value is
what makes the comparison honest rather than an assumed worst case, as the doc
claims. Spot-checked numerically: `n = 5, f = 400 000, d = 250 000` →
`ndc ≈ 0.9999925`, `ulp = 2⁻²⁴ ≈ 5.96e-8`, `Δd ≈ 745.0` (doc: 745); the same
at `n = 0.1` → `≈ 37 253` (doc: ~37 250); reversed at `n = 5` →
`ndc ≈ 7.500e-6`, `ulp = 2⁻⁴¹`, `Δd ≈ 0.00568` (doc: 0.0057); reversed at
`n = 0.1` → `≈ 0.0089`, i.e. a 1.56× move for a 50× near-plane change against
the conventional mapping's 50× — the doc's "under 2×" claim holds. The
745 / 0.0057 ≈ 130 000× headline is therefore arithmetically sound. All 16
`camera.rs` unit tests pass (`cargo test -p byroredux-core --lib camera`).
`linear_distance_from_depth` is the exact inverse of the same encode and
returns `far` at `z = 1.0`. `analyze_depth_field`'s decade-edge construction
(`10^(floor(log10 near)+1)`, stepping ×10 while `< far`, then `push(far)`) is
correct for both `near = 0.1` and `near = 5.0`; the worst degenerate case
(`near` exactly a power of ten with a rounding-down `log10`) yields a
zero-width empty band, not a misclassification, since `d >= w[0] && d < w[1]`
can never match it and `linear_distance_from_depth` guarantees `d >= near` so
the `unwrap_or(0)` fallback is unreachable.

**`depth_capture.rs` fence + teardown discipline.** The readback runs at
`draw.rs:1708`, immediately after `screenshot_finish_readback` and *after* the
`wait_for_fences` at `draw.rs:1628` — which waits on **both** slots
(`in_flight[frame]` and `in_flight[prev]`), and `MAX_FRAMES_IN_FLIGHT == 2`
(`sync.rs:6`, const-asserted), so that is device-idle-equivalent for every
prior frame and genuinely proves the previous frame's copy completed. The
non-coherent `invalidate_mapped_memory_ranges` (#2740) is present and uses the
shared `aligned_flush_range` helper, matching the screenshot path.
`destroy_depth_capture_staging` is called from `teardown.rs:244` next to
`destroy_screenshot_staging`, and from the grow path in
`ensure_depth_capture_staging` — which runs during recording, but only after
the top-of-frame both-slot fence wait, so no in-flight command buffer can still
reference the destroyed buffer. Both the allocation-failure and bind-failure
paths clean up the buffer/allocation rather than leaking. The recorded copy
uses `frame_extents.render`, which is exactly the extent the depth image is
created at (`init.rs:273`, `resize.rs:246`), so the copy region can never
exceed the image. The `#1634` "read back the extent captured at record time"
discipline is implemented via `depth_capture_pending_readback: Option<Extent2D>`
and is correct across a mid-flight resize. Layout contract holds:
`copy_depth_to_history` (`post_passes.rs:55-180`) is unconditional and restores
`DEPTH_STENCIL_READ_ONLY_OPTIMAL`, and `depth_capture_record_copy` runs on the
next line (`draw.rs:3684`) with matching barriers and restores the same layout;
the two barriers chain through overlapping stage masks. The missing owner
tag / capture generation is documented and correct for a single consumer.

**Unchanged invariants re-verified.** `RT_ABSOLUTE_PRECISION_CEILING` is still
`debug_assert!`-ed on the loaded-cell bounds via
`worldspace_extent_over_rt_ceiling` (`cell_loader/references/complete.rs:85`,
unit-tested in `references/import_tests.rs`) and by `render/fog_volumes.rs:159`;
no new absolute-space shader consumer was added by the delta.
`snap_render_origin` (`scene_buffer/constants.rs:389`) is still
`floor(pos / RENDER_ORIGIN_SNAP) * RENDER_ORIGIN_SNAP` with its pinning test.
DoF's degenerate-`focus_dist` guard (#1525) is live at `draw.rs:617`
(`dof.aperture <= 0.0 || dof.focus_dist <= DOF_MIN_FOCUS_DIST`) with
`zero_focus_dist_falls_back_to_pinhole_and_stays_finite`.
`Camera::for_content_scale`'s single production call site in
`byroredux/src/scene.rs` still passes `has_nif_content && harness_cam.is_none()`.

**Existing: #3308** — reversed-Z itself. The measured ~130 000× remaining
headroom is recorded in `DEFAULT_RENDER_DISTANCE`'s doc and in
`reversed_z_resolution_is_orders_better_at_the_lod_ring`; not re-filed.
D10-02 above is about the *gate* being one-directional, which is a distinct,
smaller, `cargo test`-fixable gap inside #3308's step 2.

---

#### REN-2026-08-30-D11-01: the UI overlay's `firstInstance` is submitted un-clamped while `upload_instances` drops exactly that instance on overflow

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the `ui_instance_idx` append at :3241-3253 and the RP-1 guard at :3298), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_instances`, :548-556), `crates/renderer/src/vulkan/presentation.rs` (`record_overlay`)
- **Status**: OPEN (pre-existing; not introduced by #3426)
- **Description**: The UI quad's `GpuInstance` is pushed onto `gpu_instances` as the **last**
  element, and `ui_instance_idx = gpu_instances.len() as u32` is captured before the push.
  `upload_instances` then clamps with `let count = instances.len().min(MAX_INSTANCES);` and
  warns that "excess draws silently dropped". Because the UI instance is last, it is the
  *first* thing the clamp discards — yet `ui_instance_idx` is still handed to
  `UiOverlayDraw.instance_index` and issued as `firstInstance` in
  `device.cmd_draw_indexed(cmd, overlay.index_count, 1, 0, 0, overlay.instance_index)`.
  `ui.vert` then reads `instances[gl_InstanceIndex]` past the end of an SSBO allocated at
  exactly `size_of::<GpuInstance>() * MAX_INSTANCES` (`scene_buffer/buffers.rs:468`).
- **Evidence**:
  - `draw.rs:3241` `let ui_instance_idx = if let (Some(ui_tex), Some(_)) = …  { let idx = gpu_instances.len() as u32; … gpu_instances.push(instance); Some(idx) }`
  - `upload.rs:548` `let count = instances.len().min(MAX_INSTANCES);`
  - `ui.vert` `GpuInstance inst = instances[gl_InstanceIndex]; fragTexIndex = inst.textureIndex;`
  - `ui.frag` `outColor = texture(textures[nonuniformEXT(fragTexIndex)], fragUV);`
  - `crates/renderer/src/vulkan/device.rs:652` — the enabled `vk::PhysicalDeviceFeatures`
    chain does **not** request `robust_buffer_access`, so the OOB SSBO load is undefined
    rather than a guaranteed zero; the garbage `textureIndex` then feeds a `nonuniformEXT`
    index into the unbounded bindless `textures[]` array.
  - The existing RP-1 comment at `draw.rs:3287-3297` reasons carefully about the clamp but
    only about *dropped draws* — it does not consider that one of the dropped entries still
    has its index submitted as `firstInstance`.
- **Impact**: Requires `gpu_instances.len() > MAX_INSTANCES` (262,144), a condition that
  already emits a one-shot `log::error!`, so this is a second-order consequence of an
  already-flagged overflow, not an independently reachable bug. But the consequence is
  worse than the documented one ("draws silently dropped"): an out-of-range descriptor-array
  index rather than a missing quad.
- **Needs RenderDoc**: no
- **Suggested Fix**: Clamp at capture — `let idx = gpu_instances.len(); if idx < MAX_INSTANCES { … Some(idx as u32) } else { None }` — so an overflowing frame skips the overlay instead of drawing it from an out-of-range instance slot. One line, and it makes the `None` arm that `record_presentation_pass` already handles do the work.

---

---

#### REN-2026-08-30-D11-02: both `create_render_pass` call sites still describe a 7-attachment G-buffer with a reservoir attachment removed under #1583

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/init.rs:305-306`, `crates/renderer/src/vulkan/context/resize.rs:299-300`
- **Status**: OPEN
- **Description**: The two places that call `create_render_pass` both label it
  "Main render pass: 7 color attachments (HDR + G-buffer + raw_indirect + albedo +
  **reservoir**) + depth." The pass has **8** color attachments (+ depth as attachment 8),
  and the ReSTIR reservoir output at location 6 was deleted under #1583 — slots 6 and 7 are
  now the FSR reactive and transparency-and-composition masks. `helpers.rs`'s own
  `create_render_pass` header block (:148-190) is correct and enumerates all nine.
- **Evidence**:
  - `init.rs:305` `// 10. Main render pass: 7 color attachments (HDR + G-buffer +` / `:306` `// raw_indirect + albedo + reservoir) + depth.`
  - `resize.rs:299` `// Main render pass: 7 color (HDR + G-buffer + raw_indirect` / `:300` `// + albedo + reservoir) + depth.`
  - `helpers.rs:222-237` `color_refs` is 8 entries; `attachments` is 9 with `depth_attachment` last.
  - `reflect.rs:1118` `triangle_frag_declares_eight_color_outputs` (passing) is the live pin.
- **Impact**: Documentation only. The hazard is specific though: attachment-count drift is
  exactly the class of bug this dimension exists to catch (a blend-state array that does not
  match `attachmentCount` is `VUID-VkGraphicsPipelineCreateInfo-renderPass-07609`), and the
  two comments a reader hits *first* both state the wrong number and name a slot that is now
  something else entirely.
- **Needs RenderDoc**: no
- **Suggested Fix**: Replace both with "8 color attachments (HDR + normal + motion + mesh_id + raw_indirect + albedo + 2 FSR masks) + depth", or just point at `helpers::create_render_pass`'s header table so there is one copy.

---

---

#### REN-2026-08-30-D11-03: `water.rs`'s module doc contradicts its own pipeline builder about which attachments water masks off

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/water.rs:20-23` (module doc) vs. `:826-874` (the blend table in the pipeline builder)
- **Status**: OPEN
- **Description**: The module doc says "attachments 1..6 (normal, motion, mesh_id,
  raw_indirect, albedo, **reservoir**) are masked off". The builder 800 lines below masks
  off 1..=5 and deliberately **writes** 6 and 7 with `fsr_mask_max` (`MAX` over `ONE`/`ONE`,
  `color_write_mask = R`), and its own comment says so — "Attachments 1..=5 are write-masked
  off … Attachments 6 and 7 (the FSR masks) are written". The doc also names the removed
  reservoir slot.
- **Evidence**:
  - `water.rs:21-22` `//!   attachments 1..6 (normal, motion, mesh_id, raw_indirect, albedo,` / `//!   reservoir) are masked off (`color_write_mask = 0`) so water never pollutes`
  - `water.rs:828-831` `// Attachments 1..=5 are write-masked off … Attachments 6 and 7 (the FSR masks) are written — see below.`
  - `water.rs:864` `// the reservoir attachment was removed under #1583.` (in the same function)
  - `water.rs:865-873` the 8-entry `attachments` array: `[hdr_blend, masked_off × 5, fsr_mask_max, fsr_mask_max]`
- **Impact**: A reader taking the module doc at face value would conclude water writes no FSR
  mask, which is the opposite of the transparency-ghosting contract the code implements.
- **Needs RenderDoc**: no
- **Suggested Fix**: Update the module doc to "attachments 1..=5 … masked off; 6 and 7 (FSR reactive + transparency) MAX-blended at full strength", matching the in-function comment.

---

---

#### REN-2026-08-30-D13-02: TAA's permanent-failure latch signals no temporal discontinuity, unlike FSR's `#2519` edge — the failing frame is rendered jittered and blitted unresolved

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, lines 753–778) vs. the FSR sibling at lines 1026–1036 (`take_new_dispatch_failure` → `signal_temporal_discontinuity`)
- **Status**: OPEN — dormant today (see Impact), asymmetry with a hazard the FSR side explicitly closes
- **Description**: `taa_jitter` is evaluated at the top of `draw_frame` (`draw.rs:2039–2048`) and gates on `self.taa_failed`, so once the latch is set every *subsequent* frame renders unjittered — that is `#1932`. But `taa_failed` is set inside `record_taa_pass`, in the post-pass tail, long after the geometry pass for that frame already rendered with the Halton offset. Composite is then rebound to raw HDR (`fall_back_to_raw_hdr`) and that jittered image is presented with nothing to resolve it, and, more importantly, the *next* frame's SVGF / volumetrics reprojection accumulates against G-buffer content that is half a pixel offset from everything after it. The FSR path treats exactly this as a hazard worth a one-shot signal: `FrameUpscaler::new_dispatch_failure` (`frame_upscaler.rs:113–125`) → `take_new_dispatch_failure()` → `self.signal_temporal_discontinuity(FSR_DISPATCH_FAILURE_RECOVERY_FRAMES)`. Its own doc names the TAA side as the same class of hazard, but `record_taa_pass` calls no equivalent.
- **Evidence**:
  - `post_passes.rs:763–771`: on `Err`, the handler does `self.taa_failed = true;` and `composite.fall_back_to_raw_hdr(&self.device);` — nothing else. `grep -rn "self.taa_failed = true" crates/renderer/src` returns this one site.
  - `post_passes.rs:1032–1036`: `if fsr_dispatch_failed_this_frame { self.signal_temporal_discontinuity(...); }` — the guard TAA lacks.
  - `frame_upscaler.rs:122–125`: "Same class of hazard `taa_jitter`'s `!taa_failed` gate closes on the TAA side (#1932)" — the `#1932` gate covers later frames, not the failing frame.
  - Adjacent, same shape: `draw.rs:3559–3564` logs `TAA upload_params failed` and continues, so the dispatch still runs against `param_buffers[frame]`'s contents from two frames ago (stale `screen` / `first_frame`). `GpuBuffer::write_mapped` (`buffer.rs:1160–1173`) can only fail on a missing/unmapped allocation, so this is theoretical rather than reachable.
- **Impact**: Currently **dormant**: `TaaPipeline::dispatch` (`taa.rs`) contains no fallible call between its `cmd_pipeline_barrier` prologue and its terminal `Ok(())`, so the `Err` arm in `record_taa_pass` is unreachable and `taa_failed` can never latch from it. The finding is a defence-in-depth gap that opens the moment anything fallible (a descriptor rewrite, a per-dispatch UBO write, a device-lost probe) is added to `dispatch`, at which point the failing frame silently poisons one frame of every downstream temporal history.
- **Suggested Fix**: In `record_taa_pass`'s `Err` arm, add `self.signal_temporal_discontinuity(N)` alongside the existing latch + `fall_back_to_raw_hdr`, mirroring `post_passes.rs:1032`. A named constant next to `FSR_DISPATCH_FAILURE_RECOVERY_FRAMES` keeps the two recovery windows discoverable together. Pin it with a source-scan test in the style of `frame_upscaler.rs:1302` (`POST_PASSES_RS.contains("take_new_dispatch_failure()")`).

---

---

#### REN-2026-08-30-D13-03: The Halton phase-count rationale is mathematically false and misidentifies the sample `% 8` missed — duplicated verbatim at two sites

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`taa_jitter` doc comment, lines 347–349; the identical inline copy in `draw_frame`, lines 2029–2033)
- **Status**: OPEN — doc-rot; the constant `16` itself is fine, the stated reason for it is not
- **Description**: Both copies read: *"Halton(2) natural period is 2, Halton(3) natural period is 3, LCM = 6. Using 16 (nearest power-of-2 ≥ 6) avoids the asymmetric Y-coverage gap that `% 8` caused (the 9th Halton(3) sample ≈ 0.889 was never reached with `% 8`)."* Both claims are wrong. (a) Halton sequences are aperiodic — the radical inverse `halton(index, base)` (`draw.rs:50–59`) is injective on `index`, so there is no "natural period 2/3" and no LCM to take. (b) `halton(9, 3) = 1/27 ≈ 0.037`, not `0.889`; `0.889 = 2/3 + 2/9 = halton(8, 3)`, and with `% 8` the index range is `(frame % 8) + 1 ∈ 1..=8`, so index 8 — and therefore `0.889` — *was* reached. The sample `% 8` actually omitted is index 9's `1/27`. As it happens `% 8` gave a perfectly stratified Y set (`{1,2,4,5,7,8}/9 ∪ {1,2}/3` = all eighths of the ninth-grid) while `% 16` adds four 27ths that are not aligned to that grid — so the comment's premise is not merely wrong, it is backwards about which is more uniform.
- **Evidence**:
  - `draw.rs:50–59` — textbook radical inverse; `halton(9,3)`: `9%3=0` (`+0`), `3%3=0` (`+0`), `1%3=1` (`+1/27`) → `0.037`. `halton(8,3)`: `8%3=2` (`+2/3`), `2%3=2` (`+2/9`) → `0.889`.
  - `draw.rs:375–376`: `let idx = (frame_counter % 16) + 1;` — the pre-`#1093` form was `% 8`, giving `idx ∈ 1..=8`, which includes 8.
  - The two comment blocks are byte-identical apart from `///` vs `//` prefixes, so any correction must be applied twice.
- **Impact**: No runtime effect — the sequence, the 1-indexing (`+ 1`, so the degenerate `halton(0, b) = 0` offset is correctly never produced), and the 16-entry wrap are all correct as written, and verified: indices 1..=16 in base 2 give all fifteen `k/16` plus `1/32`, and in base 3 give all thirds/ninths plus eight 27ths. The risk is purely that the false rationale invites a future "correction" toward the stated `LCM = 6`, or toward a different phase count on a premise that does not hold.
- **Suggested Fix**: Replace both copies with the real reason for 16 (a power-of-two wrap that keeps the base-2 dimension exactly stratified across the cycle, with a phase count comparable to the ~10-frame effective window implied by `alpha = 0.1` in `taa.rs::upload_params`), and drop the period/LCM sentence and the `0.889` claim. If the phase count is ever revisited on quality grounds, that is a measurement, not a comment edit — do not change `16` as part of this doc fix.

---

---

#### REN-2026-08-30-D13-04: `taa.comp` holds a fourth, differently-named copy of the octahedral decoder that the shared-copy maintenance comment does not enumerate

- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp` (`oct_decode`, line 36); `crates/renderer/shaders/svgf_atrous.comp` (lines 77–80, the enumeration); siblings `crates/renderer/shaders/svgf_temporal.comp:68`, `crates/renderer/shaders/caustic_splat.comp:176`; encoder at `crates/renderer/shaders/include/math_common.glsl:35`
- **Status**: OPEN — duplication + stale maintenance note
- **Description**: The octahedral **encoder** is centralised (`octEncode` in `include/math_common.glsl`, the single function every `outNormal` write in `triangle.frag` goes through). The **decoder** is not: it is copy-pasted into four shaders. Three of them spell it `octDecode` and carry a maintenance comment enumerating the other copies — `svgf_atrous.comp:79` says it "must stay bit-identical to the `octDecode` copies in `svgf_temporal.comp` and `caustic_splat.comp`". `taa.comp` spells its copy `oct_decode` (snake_case, unlike every sibling) and is absent from that enumeration, so neither a `grep octDecode` nor the comment leads a maintainer to it.
- **Evidence**:
  - `grep -rn "oct_decode\|octDecode" crates/renderer/shaders/` → `taa.comp:36,175,176` (`oct_decode`) plus `svgf_atrous.comp:80`, `svgf_temporal.comp:68`, `caustic_splat.comp:176` (`octDecode`). `include/math_common.glsl` defines `octEncode` only.
  - `svgf_atrous.comp:77–79` names exactly two sibling copies; taa.comp is the third sibling and is not named.
  - The four bodies are currently identical in behaviour (`n.z = 1 - |x| - |y|`, wrap-fold when `n.z < 0`, `normalize`), so this is drift *risk*, not present drift.
- **Impact**: `taa.comp`'s only use of the decoder is the surface-consistency disocclusion test (`dot(currNormal, prevNormal) < 0.85`, `taa.comp:175–177`) that the `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` guard depends on. A future correction to the shared decode (precision, a `normalize` removal, a snorm-range change) applied to the three enumerated copies would leave TAA decoding differently from the G-buffer producer and from SVGF — a silent, per-pixel divergence in a history-rejection predicate, invisible to every existing test since all of them are source-scan pins.
- **Suggested Fix**: Move the decoder next to `octEncode` in `include/math_common.glsl` as `octDecode`, have all four shaders `#include` it (`taa.comp` already uses `GL_GOOGLE_include_directive` for `shader_constants.glsl` and `mesh_id.glsl`), and delete the enumeration comments that only existed to compensate for the duplication. Minimum change if the include is deferred: rename `taa.comp`'s copy to `octDecode` and add it to the `svgf_atrous.comp` enumeration so the existing convention at least finds it.

---

## Verified clean (no finding)

- **Halton implementation.** `halton()` (`draw.rs:50–59`) is a correct radical inverse for arbitrary base. `taa_jitter` (`draw.rs:366–384`) uses `idx = (frame_counter % 16) + 1`, i.e. **1-indexed** — the degenerate zero-jitter `halton(0, b) = 0` is never produced. Coverage over the 16-entry cycle checked by hand in both bases (see D13-03 Impact); no repeated or clustered offset. Mapping `(h - 0.5) * 2.0 / extent` is the correct pixel→NDC conversion, and it uses `frame_extents.render` (not `output`), which is the extent the resolve runs at.
- **Un-jittered projection retained for motion vectors.** `triangle.vert:253–254, 318–333`: `fragCurrClipPos` / `fragPrevClipPos` are both un-jittered (`prevViewProj * prevWorldPos`), and `currClip.xy += jitter.xy * currClip.w` is applied only to `gl_Position`, after the varyings are written. `water.vert:248–252` does the same. Motion-vector reconstruction is jitter-free.
- **One jitter source, not two.** The `(jx, jy)` block (`draw.rs:2039–2066`) is a `match` on `renderer_config.upscaler`: the `Taa` arm calls `taa_jitter` (Halton 2,3 × 16), the `Fsr3` arm reads `FsrTemporalState::current()` (`upscaling.rs:326`), whose sample table is built once from the SDK's own `fsr3::jitter_phase_count(render.width, output.width)` + `fsr3::jitter_offset` (`upscaling.rs:305–317`). The two are mutually exclusive per frame and neither re-derives the other's sequence, so no divergence is possible. The phase counts *do* differ (fixed 16 for TAA; SDK-computed, scale-ratio-dependent for FSR) — correct, since FFX requires its own count and TAA is not an FFX consumer. Sign conventions were reconciled by `#2772`: both negate Y into Vulkan NDC (`fsr_pixel_jitter_to_ndc`, `upscaling.rs:372–378`), pinned by `taa_and_fsr_negate_jitter_y_the_same_way` (`draw.rs:424–453`).
- **Per-frame-in-flight history, no aliasing.** `history[f]` is written (binding 5, storage) while `history[prev]` is sampled (binding 4), with `prev = (f + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT` — the general previous-slot form since `#2771`, guarded by the compile-time `assert!(MAX_FRAMES_IN_FLIGHT >= 2)`. `prev_mid` / `prev_normal` use the same index, so all three previous-frame taps agree on the slot.
- **Reprojection filtering.** Motion is `texelFetch`ed and dilated over a 5-tap cross constrained to `stableMeshIdsMatch(candidateMeshId, currMid)` (`taa.comp:118–139`) — not naive point sampling. History is sampled with the 9-tap Catmull-Rom (`sample_history_catmull_rom`) through `linear_sampler`, `CLAMP_TO_EDGE`, with the offscreen case rejected before the taps can reach outside.
- **Neighborhood clamp and disocclusion.** Full 3×3 YCoCg moment clamp at `γ = 1.5` (`#1108`), mesh-ID disocclusion, alpha-blend opt-out via bit 31 (`meshIdHasStableHistory`, never `& 0x7FFFFFFF`), normal-based surface-consistency reject at `0.85`, and the `#903` NaN/Inf guard applied **before** the clamp so it does not depend on driver-specific `min`/`max` NaN semantics. G-buffer normals are world-space (`octEncode(N)` at `triangle.frag:539` with `N` built from world-space geometry/tangent inputs), so the `dot` test is camera-rotation invariant — it does not reject history on a fast turn.
- **`#1497` progressive-alpha floor is gone and cannot recur.** `alpha` is the flat `0.1` in `upload_params`; `taa.comp` contains no `pixelStatic` / `cameraStatic` / `static_frames`, and `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` asserts their absence. Not re-reported.
- **First frame / forced reset.** `should_force_history_reset(c) := c < MAX_FRAMES_IN_FLIGHT` (`svgf.rs:240–242`) drives `params.y`, and `taa.comp:110–113` returns `currRgb` before any `uPrevHistory` / `uPrevMeshId` tap — the undefined-contents window is never sampled. `frames_since_creation` advances only via `mark_frame_completed` after `queue_submit` succeeds (`#917`), and is zeroed by all three reset entry points (`new_inner`, `signal_history_reset`, `recreate_on_resize`). Pinned by `first_frame_params_y_is_one_when_frames_since_creation_is_zero` and `params_y_is_zero_once_history_is_warm`.
- **Layouts.** Every history slot is walked `UNDEFINED → GENERAL` exactly once by `initialize_layouts`, from all three construction sites (`init.rs:1313`, `resize.rs:1363`, and inside `recreate_on_resize` per `#1031`). The per-frame barriers are `GENERAL → GENERAL` (`image_barrier_general_write_to_read`, `descriptors.rs:245–253`) — no per-frame `UNDEFINED` discard. Post-barrier `dst_stage` covers both consumers (`FRAGMENT_SHADER | COMPUTE_SHADER`, `#653`).
- **`validate_set_layout` fires.** `taa.rs` calls it on all 9 bindings against `taa.comp.spv` with `.expect("taa descriptor layout drifted against taa.comp (see #427)")` — a construction-time hard failure, and the pool is sized `DescriptorPoolBuilder::from_layout_bindings(&bindings, …)` from the same array (`#1030`), so pool and layout cannot drift apart either.
- **Dispatch grid.** `width.div_ceil(WORKGROUP_X)` / `height.div_ceil(WORKGROUP_Y)` using the build-script-generated constants `taa.comp`'s `local_size` is generated from (`#2768`) — no literal tile size, no unwritten bottom-right region.
- **Disable path and preset switching.** TAA is constructed only when `renderer_config.upscaler == UpscalerMode::Taa` (`init.rs:1287`); under FSR `self.taa` is `None` and `record_taa_pass` is a no-op — the dispatch is skipped entirely, not dispatched-and-discarded. `set_upscaler_mode` (`resize.rs:1223–1310`) `device_wait_idle`s, destroys the TAA pipeline, rebinds composite binding 0 back to the raw HDR attachments *before* those views are replaced, rebuilds render targets, calls `build_taa_pipeline()` on the way into TAA (including on the rollback path), and finishes with `signal_temporal_discontinuity(8)`. `composite::recreate_on_resize` independently rewrites binding 0 to the fresh `hdr_image_views` (`composite.rs:1339–1342`) before `resize.rs:998–1002` re-points it at TAA output only `if let Some(ref t) = self.taa` — so no stale or dangling TAA history view can survive a switch in either direction.
- **`taa_failed` recovery.** Reset to `false` by `recreate_swapchain` (`resize.rs:1056`) and by `build_taa_pipeline` (`resize.rs:1375`), so a latched failure is not permanent across a resize/mode change. (The missing *discontinuity* signal on the latching frame is D13-02.)

---

#### REN-2026-08-30-D16-02: `renderer.md`'s bloom section still credits composite with the bloom add, and omits `bloom_apply.comp`

- **Severity**: Low
- **Dimension**: Bloom
- **Location**: `docs/engine/renderer.md` (§"Bloom (M58)", ~line 642; pipeline bullet ~line 64)
- **Status**: OPEN — new
- **Description**: The section names only `bloom_downsample.comp` and
  `bloom_upsample.comp` and states *"The final `up_mips[0]` is what composite
  adds to scene HDR before tone-mapping"*. Since #2796 composite does not add
  bloom at all — a third shader, `bloom_apply.comp`, reads composite's output
  back as a storage image and does the add in place, and the bloom chain now
  runs **after** composite rather than before it. `bloom_apply.comp` is not
  mentioned anywhere in `renderer.md`, including its file-tree listing
  (`~line 195`, "Bloom pyramid (M58) — separable down/up compute passes").
- **Evidence**:
  - `crates/renderer/shaders/bloom_apply.comp:52` — `imageStore(sceneImage, coord, vec4(scene.rgb + bloom * BLOOM_INTENSITY, scene.a));`
  - `crates/renderer/src/vulkan/bloom.rs:760` — `pub unsafe fn apply_to_scene(...)`
  - `crates/renderer/shaders/composite.frag:820`–`829` — "bloom now dispatches AFTER this pass … `bloomTex` (binding 7) is therefore unused by this shader now"
  - `crates/renderer/src/vulkan/context/post_passes.rs:271` — `self.record_bloom_pass(cmd, frame);` ordered after `record_composite_pass`
- **Impact**: The doc points a reader at the wrong pass for the add site and
  at the wrong pass ordering. Given #3247 is open on exactly the barriers
  around that relocation, an out-of-date map of where bloom reads and writes
  actively works against whoever picks up #3247.
- **Suggested Fix**: Add `bloom_apply.comp` to the section, the pipeline
  bullet, and the file-tree listing; restate the add site as
  "`bloom_apply.comp`, in place on `composite.scene_images[frame]`, upstream
  of the FSR/native upscale and of `presentation.frag`'s ACES" — the
  tone-map claim itself is still correct and should stay.

---

---

#### REN-2026-08-30-D16-03: `procedural-volumetric-fog.md` still ships `froxel_xy_divisor` default 4 and a four-volume footprint

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `docs/engine/procedural-volumetric-fog.md:47`, `:284`, `:294`, `:306`, `:327`
- **Status**: OPEN — new
- **Description**: The M55 design spec — the doc `ROADMAP.md:809` names as the
  authoritative M55 spec — states four things the live code contradicts:
  (a) *"Defaults are one froxel per 4×4 render pixels"* (`:47`);
  (b) `--froxel-xy-divisor <2..32>   default 4` (`:284`), repeated in the
  worked example (`:294`); (c) the measurement table marks the divisor-4 row
  `214×120×64` as "default" (`:306`); (d) *"At the default 214×120×64 grid,
  the **four** RGBA16F fields (raw, integrated, chemistry, velocity) plus the
  R32F emissive-history sidecar consume about 56 MiB per frame slot"*
  (`:327`) — the live set is **five** RGBA16F (it omits
  `combustion_optical_volumes`) plus the R32F, i.e. 44 B/froxel/slot, not the
  36 B that yields 56 MiB.
- **Evidence**:
  - `crates/renderer/src/vulkan/upscaling.rs:135` — `froxel_xy_divisor: 8`
  - `crates/renderer/src/vulkan/volumetrics.rs:600`–`606` — `FROXEL_VOLUMES_PER_SLOT: usize = 6`, `FROXEL_BYTES_PER_SLOT: u64 = 44`
  - `crates/renderer/src/vulkan/volumetrics.rs:1029`–`1039` — `combustion_optical` volume, `COMBUSTION_FIELD_FORMAT` (RGBA16F), the fifth RGBA16F the doc does not list
  - `byroredux/src/cli_args.rs:97` — the CLI default *is* `VolumetricsConfig::default().froxel_xy_divisor`, so the shipped default is 8 and the doc's "default 4" is wrong on both the flag and the grid
  - `docs/engine/memory-budget.md:235`, `:238`, `:250` — the *other* doc states divisor 8 / six volumes / 44 B and is test-pinned by `volumetrics.rs:3661`
- **Impact**: The two volumetrics docs now disagree with each other. The
  memory-budget one is right (and enforced); the design spec is wrong and
  unenforced, so it is the one a reader hits first from the ROADMAP link.
  Anyone sizing the grid or reproducing the measurement table from `:294`
  will silently run at 4× the shipped froxel count.
- **Suggested Fix**: Update `:47`, `:284`, `:294` to 8; re-label the table's
  "default" marker onto the divisor-8 row (keeping the divisor-4 measurements
  as historical rows, per the doc's own "keep rows even when a path is not
  implemented" rule); rewrite `:327` for five RGBA16F + one R32F = 44 B/froxel
  and re-derive the per-slot MiB. Extend
  `froxel_grid_cost_matches_the_memory_budget_doc` to `include_str!` this doc
  too, so both ledgers are pinned by the one test.

---

---

#### REN-2026-08-30-D16-04: in-code comments quote the pre-retune froxel grid and a 4× ray-query count

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:543` (doc comment on `VOLUMETRIC_OUTPUT_CONSUMED`), `crates/renderer/shaders/volumetrics_inject.comp:25`–`26`, `crates/renderer/shaders/volumetrics_integrate.comp:27`–`28`
- **Status**: OPEN — new
- **Description**: Three comments state a froxel grid derived from the old
  `froxel_xy_divisor = 4`:
  - `volumetrics.rs:543` — *"~36.9M ray queries/frame at the default
    320x180x64 grid for a 1280x720 render extent"*
  - `volumetrics_inject.comp:25`–`26` — *"At the default 320x180x64 grid that
    is a worst-case ~36.9M ray queries/frame from this pass alone"*
  - `volumetrics_integrate.comp:27`–`28` — *"we run it 129 600 times per frame
    at 1080p with the default /4 grid"*

  At the live divisor of 8, a 1280×720 render extent gives **160×90×64**
  (921 600 froxels), so the same worst case is **~9.2M** ray queries, and a
  1920×1080 native extent gives 240×135 = **32 400** integrate columns, not
  129 600.
- **Evidence**:
  - `crates/renderer/src/vulkan/upscaling.rs:135` — `froxel_xy_divisor: 8`
  - `crates/renderer/src/vulkan/volumetrics.rs:562`–`576` — `froxel_extent` = `render.div_ceil(divisor)` × `froxel_z_slices`
  - `1280.div_ceil(8) = 160`, `720.div_ceil(8) = 90`; `160 × 90 × 64 = 921 600`; × the comment's own worst-case 10 traversals/froxel = 9 216 000
  - Test `froxel_extent_uses_render_resolution_and_configured_divisor`
    (`volumetrics.rs:3416`) was deliberately written to derive rather than
    snapshot `[320, 180, 64]` for exactly this reason — the comments were not
    given the same treatment.
- **Impact**: These are the numbers a performance investigation reads first
  when deciding whether the inject pass is worth optimising; being 4× high
  misdirects that decision. `volumetrics.rs:543` in particular is the
  justification comment on the live `VOLUMETRIC_OUTPUT_CONSUMED` gate.
- **Suggested Fix**: Restate all three relative to the config
  (e.g. "at the default divisor of 8, a 1280×720 render extent gives
  160×90×64") or drop the absolute counts and keep the per-froxel worst case,
  which is divisor-independent. Same fix shape the test at `:3416` already
  adopted.

---

---

#### REN-2026-08-30-D16-05: the volumetric far plane has three unpinned copies of its default

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/upscaling.rs:137` (`grid_far_meters: 128`), `crates/renderer/src/vulkan/volumetrics.rs:268` (`DEFAULT_GRID_FAR_METERS: f32 = 128.0`), `crates/renderer/src/shader_constants_data.rs:354` (`VOLUME_FAR: f32 = 8_960.0`)
- **Status**: OPEN — new (regression guard)
- **Description**: The same default — 128 m — is written independently in
  three places. `VOLUME_FAR = 8_960.0` is the same value pre-multiplied by
  `BETHESDA_UNITS_PER_METER = 70.0` and its own comment calls it *"the
  canonical default for diagnostics and shader-contract tests"*, but nothing
  asserts `VOLUME_FAR == DEFAULT_GRID_FAR_METERS * 70.0 ==
  VolumetricsConfig::default().grid_far_meters as f32 * 70.0`. `grep` finds no
  test relating any pair.
- **Evidence**:
  - `crates/renderer/src/vulkan/volumetrics.rs:268`–`269` — `DEFAULT_GRID_FAR_METERS: f32 = 128.0;` / `DEFAULT_VOLUME_FAR = DEFAULT_GRID_FAR_METERS * WORLD_UNITS_PER_METER`
  - `crates/renderer/src/vulkan/upscaling.rs:137` — `grid_far_meters: 128`
  - `crates/core/src/lighting.rs:16` — `BETHESDA_UNITS_PER_METER: f32 = 70.0`
  - `crates/renderer/src/vulkan/context/draw.rs:3509` — `DEFAULT_VOLUME_FAR` is the live fallback when `self.volumetrics` is `None` (reachable: `context/init.rs:959` sets it to `None` on a froxel-layout init failure), so a drifted copy is a behavioural divergence from `--fog-grid-far-m`, not only cosmetic
  - The three values currently agree (128 / 128 / 8 960 = 128 × 70) — this is a guard, not a live bug
- **Impact**: This is the #3117 failure shape one axis over: #3117 was filed
  because a stated default (the ledger's froxel cost) silently diverged from
  the live one after a retune. `froxel_xy_divisor` and `froxel_z_slices` are
  now pinned to the config by `froxel_extent_uses_render_resolution_and_configured_divisor`
  and the memory-budget test; `grid_far_meters` is the one member of
  `VolumetricsConfig` with duplicate literals and no pin.
- **Suggested Fix**: Either derive — `DEFAULT_GRID_FAR_METERS` becomes
  `VolumetricsConfig::default().grid_far_meters as f32` and `VOLUME_FAR`
  becomes `DEFAULT_GRID_FAR_METERS * BETHESDA_UNITS_PER_METER` (blocked only
  if `shader_constants_data.rs` must stay dependency-free for `build.rs`, in
  which case) — or add a three-line test in `volumetrics.rs`'s `tests` module
  asserting all three agree, alongside the existing budget test.

---

---

#### REN-2026-08-30-D16-06: ROADMAP's M58 row does not record that bloom shipped, and the shaders' deferral has no tracking home

- **Severity**: Low
- **Dimension**: Bloom
- **Location**: `ROADMAP.md:812` (M58 row); referenced from `crates/renderer/shaders/bloom_downsample.comp:16` and `crates/renderer/shaders/bloom_upsample.comp:15`
- **Status**: OPEN — new
- **Description**: Both shipped bloom shaders explicitly defer the
  Jimenez/Kawase filter upgrade and say *"See the M58 row in ROADMAP.md for
  tracking"* / *"Upgrade target tracked in ROADMAP.md M58 row"*. The M58 row
  sits unannotated in the *planned*-milestone table and describes
  `Kawase-blur bloom (5-pass dual filter, ~2 ms total)` as future scope — it
  records neither that the bloom sub-slice shipped (Session 33, `33f48b5`,
  `HISTORY.md:2609`) nor the box-filter-for-now decision the shaders point at.
  The neighbouring M55 row *does* carry exactly this treatment
  (**"Fog slice shipped 2026-07-26→08-01 (Session 62)…"**), so the convention
  exists and M58 was simply missed.
- **Evidence**:
  - `crates/renderer/shaders/bloom_downsample.comp:10`–`16` — the box-vs-13-tap rationale and the ROADMAP pointer
  - `crates/renderer/shaders/bloom_upsample.comp:13`–`15` — same pointer
  - `ROADMAP.md:812` — the row, with no shipped annotation and no mention of the deferral
  - `ROADMAP.md:809` — the M55 row's shipped-slice annotation, the pattern to copy
  - `HISTORY.md:2609` — `33f48b5` "M55 volumetrics + M58 bloom + M-LIGHT v1"
- **Impact**: A dangling cross-reference: two shipping shaders cite a tracking
  location that tracks nothing, so the deliberate box-filter decision (and
  the composed 5× pyramid DC gain documented at `bloom_upsample.comp:18`–`35`,
  which the eventual re-tune must account for) is recorded only in shader
  comments. Whoever picks up M58's remaining scope has no signal that the
  bloom slice is already live and that `BLOOM_INTENSITY` was tuned against an
  unnormalised pyramid.
- **Suggested Fix**: Annotate the M58 row in the M55 style: bloom slice
  shipped Session 33, current filter is a 4-tap box down / 4-tap box + add up
  over `BLOOM_MIP_COUNT = 5`, add site relocated after composite by #2796,
  remaining M58 scope = Jimenez 13-tap/9-tap (needs the SIGGRAPH 2014 slides
  in-repo per the no-guessing rule) + DOF + motion blur + 3D-LUT grading +
  AgX/Tony McMapface, and note that the intensity re-tune is coupled to the
  pyramid's DC gain.

---

#### REN-2026-08-30-D17-03: the Disney anisotropic lobe (#1250/#1254) is unreachable from every importer, and the code comment that justifies it ("no BGSM/BGEM/inline-NIF field maps to them") is only half true — the *enable* bit exists in both formats, the *magnitude* is what is missing


- **Severity**: LOW
- **Dimension**: Disney BSDF (coverage / stale rationale)
- **Location**: `byroredux/src/material_translate.rs` (lines 575-581, `anisotropic: 0.0`), `crates/renderer/shaders/include/pbr.glsl` (`distributionGGXAniso` line 41, `deriveAxAy` line 71), `crates/renderer/shaders/include/lighting.glsl` (aniso branch, lines 189-206). Contradicting evidence: `crates/bgsm/src/bgsm.rs` (`aniso_lighting` field line 88, parsed line 254), `crates/nif/src/shader_flags.rs` (`skyrim_slsf2::ANISOTROPIC_LIGHTING` line 149, `fo4_slsf2::ANISOTROPIC_LIGHTING` line 256)
- **Status**: NEW (recast of a coverage gap, not a re-file — no `anisotrop` hit in `issues.json` or in `AUDIT_RENDERER_2026-08-27.md`)
- **Description**: `translate_material` hardcodes `anisotropic: 0.0` under the comment *"Disney-BSDF-only parameters with no source-format equivalent (no BGSM/BGEM/inline-NIF field maps to them) … Reachable only via `mat.set` (Cornell harness)."* The "reachable only via `mat.set`" half is accurate and verifiable: `grep -rn "anisotropic" --include=*.rs byroredux/src crates/` shows the only writers of `Material::anisotropic` are `byroredux/src/commands/scene.rs:963` (the `mat.set` console command) and `byroredux/src/cornell.rs:1495`. Every importer path leaves it at `0.0`, so the `mat.anisotropic > 0.0` branch in `shadowableLightRadiance` (lighting.glsl:189) never taken on loaded game content, and `distributionGGXAniso` / `deriveAxAy` execute only in the Cornell box.

  The "no source-format equivalent" half is wrong as written. `BgsmFile::aniso_lighting` is a parsed `bool` (bgsm.rs:254), and both `skyrim_slsf2` and `fo4_slsf2` define `ANISOTROPIC_LIGHTING = 0x0020_0000`. What no format supplies is a *strength scalar* — which makes the current `0.0` the right call under the no-guessing policy, but for a different reason than the comment gives.
- **Evidence**:
  - `byroredux/src/asset_provider/material.rs:1652` lists `aniso_lighting` in its own inventory of BGSM fields that are decoded but not forwarded — so the field's existence is already known one module away from the comment that denies it.
  - `crates/nif/src/shader_flags.rs:412` asserts `fo3nv_f2::ALPHA_DECAL == skyrim_slsf2::ANISOTROPIC_LIGHTING` — the same bit means two different things across families.
  - The #1254 `clamp(anisotropic, 0.0, 1.0)` guard, the #1250 `ax == ay` degeneracy, and the `0.025²` α-floor were all re-derived and verified correct this sweep (see the clean list below); they simply guard a branch nothing reaches.
- **Impact**: Two things. (a) Audit signal: the `#1250` / `#1254` regression guards are green but cover no shipping content, so "anisotropic GGX verified" overstates what is actually exercised — worth knowing before anyone spends a session re-auditing that lobe. (b) A future reader acting on the comment as written would conclude the source formats carry nothing at all and stop looking, when in fact only the magnitude is missing.
- **Suggested Fix**: Correct the comment at `material_translate.rs:575-581` to state the real situation — enable bit present in BGSM (`aniso_lighting`) and in SLSF2 for Skyrim/FO4, magnitude absent from every format, therefore deliberately not synthesised — and cross-reference `asset_provider/material.rs:1652`. Do **not** wire a fabricated strength. If the lobe is ever to be reached from content, the enable bit must be read through the `TextureSlotLayout` gate that `dedicated_shader.rs:170` already uses for the sibling SLSF2 bits, because bit 21 is `Alpha_Decal` on FO3/FNV (shader_flags.rs:412) and an ungated read would turn every FNV decal into an anisotropic surface.

---

---

#### REN-2026-08-30-D17-04: `ImportedMaterial::lighting_effect_2` is documented as the Skyrim *backlight* scalar; `nifly` — the reference checked into `/mnt/data/src/reference/` — names the same wire field `rimlightPower`, which is what the shader actually consumes it as


- **Severity**: LOW
- **Dimension**: Disney BSDF (Bethesda lighting-response family — doc vs. consumer contradiction)
- **Location**: `crates/nif/src/import/material/mod.rs` (doc block, lines 643-647), `crates/nif/src/blocks/shader.rs` (`parse_skyrim`, lines 927-928), `crates/renderer/shaders/include/lighting.glsl` (`bethesdaRimFactor` line 98, `bethesdaBackFactor` line 106). Reference: `/mnt/data/src/reference/nifly/include/Shaders.hpp:647-648`, `/mnt/data/src/reference/nifly/src/Shaders.cpp:468-471`, `/mnt/data/src/reference/nifxml/nif.xml:6605-6609`
- **Status**: NEW. Distinct from #3448 (which is about `bethesdaRimFactor`'s `0.0 → 0.25` clamp floor) and from #3452 (FO4 `FLT_MAX` sentinel). This is the *identity* of the Skyrim fallback field, not its clamping.
- **Description**: The ByroRedux doc block says:

  > `BSLightingShaderProperty.lighting_effect_2` — Skyrim backlight scalar (BSVER < FO4, gated by `SLSF2_Back_Lighting`). Drives the back-lit translucency term on hair / foliage / fabric edges. Default 0.0 = no backlight.

  `nifly` reads the same two floats at the same offsets for the same version window (`stream.GetVersion().User() <= 12 && Stream() < 130`) and names them `softlighting` (default `0.3f`) and **`rimlightPower` (default `2.0f`)** — `Shaders.hpp:647-648`, `Shaders.cpp:468-471`. `nif.xml` agrees on the defaults (`Lighting Effect 1` default `0.3` range `0..10`; `Lighting Effect 2` default `2.0` range `0..1000`). So slot 2 is the **rim-light power**, and Skyrim has no authored backlight strength at all.

  The shader already implements it correctly:
  `bethesdaRimFactor` uses `exponent = mat.rimlightPower > 0.0 ? mat.rimlightPower : mat.lightingEffect2;` (lighting.glsl:100-102) — the FO4 field first, the Skyrim field as the fallback — and `bethesdaBackFactor` deliberately does **not** read `lightingEffect2`, with the correct in-code justification *"Skyrim's slot-7 back-light map has no separate strength scalar"* (lighting.glsl:108-110). The importer doc is the only thing that is wrong, and it contradicts the consumer it feeds.
- **Evidence**:
  - `nifly` field order and version gate: `Shaders.cpp:468-471` — `if (User() <= 12 && Stream() < 130) { Sync(softlighting); Sync(rimlightPower); }`. ByroRedux `parse_skyrim` reads `lighting_effect_1` then `lighting_effect_2` at the same position (`crates/nif/src/blocks/shader.rs:927-928`) — same two floats, so the mapping is 1:1.
  - `nifly` public accessors confirm the semantics: `GetRimlightPower()` returns `rimlightPower`, `GetSoftlight()` returns `softlighting`, `GetBacklightPower()` returns the FO4-only `backlightPower` (`Shaders.cpp:668-680`).
  - Secondary, same doc block: `lighting_effect_1`'s *"Default 0.0 = no SSS contribution"* is not the format default either — nifly and nif.xml both ship `0.3`. Harmless in practice (`parse_skyrim` always reads the wire value, so the struct default is only reached by non-BSLSP materials whose `SOFT_LIGHTING` bit is clear anyway), but it makes the doc a bad source for anyone reasoning about unauthored materials.
- **Impact**: A reader who trusts the doc will "fix" the shader — either by moving `lightingEffect2` from `bethesdaRimFactor` into `bethesdaBackFactor`, or by re-gating it on `MAT_FLAG_BACK_LIGHTING` — and break the Skyrim rim path, which is currently correct. This family already produced two filed defects (#3448, #3452) in five days; a doc that misidentifies one of its three Skyrim-reachable fields is an active trap for the next fix in the same file.
- **Suggested Fix**: Rewrite the two doc blocks at `crates/nif/src/import/material/mod.rs:638-647` to match `nifly`: `lighting_effect_1` = Skyrim soft-lighting / subsurface width (nifly `softlighting`, format default 0.3, gated by `SLSF2_Soft_Lighting`), `lighting_effect_2` = Skyrim rim-light power (nifly `rimlightPower`, format default 2.0, gated by `SLSF2_Rim_Lighting`), and state explicitly that Skyrim authors **no** backlight strength — which is why `bethesdaBackFactor` uses a unit fallback. Cite `nifly Shaders.cpp:468-471` inline so the next reader does not have to re-derive it. Doc-only; no shader change (the shader is right).

---

## Verified clean (no finding)

Every item below was re-derived against current code this sweep; none is
reported as a finding.

**Flag catalog / gating.** All `MAT_FLAG_*` bits are generated from
`crates/renderer/src/shader_constants_data.rs:401-429` into
`shaders/include/shader_constants.glsl:159-161` via `crates/renderer/build.rs`
— no hand-declared literals shader-side (#1357 migration intact; parity pinned
by `crates/renderer/src/shader_constants.rs:1168-1170`). `MAT_FLAG_PBR_BSDF`
(bit 5) has exactly **one** shading gate site,
`lighting.glsl:220`; `triangle.frag:1549` is the `viewMaterialLobe` debug
colour, not a shading path. No FNV/FO3/Skyrim legacy branch reaches the Disney
lobe: `ImportedMaterial::is_pbr` is set only by the BGSM/`.mat` merge arms
(`asset_provider/material.rs:963`, `:1295`) and the Cornell harness.

**Disney lobe math.** `disneyDiffuseSplit` (pbr.glsl:196-249) reproduces
`EvalDisneyDiffuse` term-for-term: `Rr = 2·roughness·HdotL²`,
`Fretro = Rr(FL+FV+FL·FV(Rr−1))`, `Fd = (1−0.5FL)(1−0.5FV)`,
`Fss90 = 0.5·Rr`, `ss = 1.25(Fss(1/(NdotL+NdotV) − 0.5) + 0.5)`,
`o.diffuse = albedo · mix(Fd+Fretro, ss, subsurface) · (1/π)` (line 245),
`o.sheen = FH · sheen · sheenColor` — additive, **not** `/π` (line 246, #1249/#1252).
The #2819 luminance-normalised sheen tint (`albedo / dot(albedo, (0.3,0.6,0.1))`)
is present and correct.

**Clustered rescale (#2243 / `c4cb2614`).** `lighting.glsl:229` reads
`diffuseBrdf = (dd.diffuse + dd.sheen) * PI * (1.0 - metalness);` — the whole
lobe, not diffuse alone. The pin
`disney_sheen_keeps_its_relative_weight_in_canonical_direct_path`
(`shader_contract_tests.rs:2196`) holds, including its negative assertion that
`triangle.frag` carries no duplicate synthetic-sun BRDF arm — confirmed, the
only `disneyDiffuseSplit` call in the tree is lighting.glsl:222.

**DALC irradiance→radiance (#2244).** `pathEnvironmentRadiance` returns
`sampleDalcCube(rayDir) * (1.0 / PI)` (lighting.glsl:357); pin
`bounded_path_converts_dalc_irradiance_to_environment_radiance`
(`shader_contract_tests.rs:2219`) holds, as does the #2472 sibling that
extends the conversion to the sky-mix and interior-fallback arms.

**IOR / F0.** `dielectricF0FromIor` (pbr.glsl:144-150) clamps `η ≥ 1e-3`
(#1253) and is the sole F0 source at every dielectric site
(triangle.frag:1403-1404, pbr.glsl:473, :496). `DEFAULT_DIELECTRIC_IOR = 1.5`
(`crates/core/src/ecs/components/material.rs:13`) reproduces the pre-#1248
`vec3(0.04)`. `GLASS_SURFACE_BEHAVIOR` is `{roughness 0.10, metalness 0.0,
ior 1.45}` (material.rs:43-47) and `apply_surface_behavior` (material.rs:1116-1120)
writes **only** those three scalars — authored `texture_path` / `normal_map` /
`glow_map` / `uv_scale` / alpha survive; guard
`glass_behavior_preserves_authored_map_overlay` (material.rs:1393) present.

**Anisotropic GGX.** `distributionGGXAniso` (pbr.glsl:41-47) degenerates
*exactly* to `distributionGGX`: with `ax = ay = α` and the orthonormal-basis
identity `HdotX² + HdotY² = 1 − NdotH²`, the denominator collapses to
`[1 + NdotH²(α²−1)]/α²` and the prefactor `1/(π·α²)` gives
`α²/(π[1+NdotH²(α²−1)]²)` — byte-for-byte the isotropic form (#1250).
`deriveAxAy` (pbr.glsl:71-86) clamps `anisotropic` to `[0,1]` before the sqrt,
so `aspect = sqrt(1 − 0.9a) ≥ sqrt(0.1)` and no `sqrt(<0)` is reachable at
`anisotropic = 1.0` (#1254). The `0.025²` α-floor round-trips to the same
`roughness ≥ 0.025` that `specularAaRoughness`'s `0.025⁴` α² clamp enforces.

**#1147 Phase 2b sibling independence.** `MAT_FLAG_TRANSLUCENCY` (6),
`MODEL_SPACE_NORMALS` (7), `THICK_OBJECT` (8), `MIX_ALBEDO` (9) each gate their
own branch with no cross-activation: MSN at triangle.frag:491 (with the
separate `MSN_HAS_AUTHORED_Z` bit 12 at :494), thickness at :2933, mix-albedo
at :2946, all nested strictly inside the bit-6 test. (The bit-6 branch itself
is unreachable — see D17-01 — but the *gating* is structurally correct.)

**Bethesda lighting-response family, remaining fields** (the #3448/#3452 sibling
sweep). `subsurface_rolloff` / `lighting_effect_1`: `bethesdaDiffuseLightFactor`
uses `width = subsurfaceRolloff > 0 ? subsurfaceRolloff : lightingEffect1`, and
`width == 0` yields `wrapped == front` — a no-value 0.0 is an exact no-op, not
an extreme. `backlight_power`: `strength = backlightPower > 0 ? backlightPower :
1.0` — 0.0 falls back to Skyrim's documented unit convention, not to a clamp
floor. `fresnel_power`: **not** a #3448-class defect — `parse_skyrim` already
substitutes `5.0` for the unauthored Skyrim case with an explicit #2589
rationale (`crates/nif/src/blocks/shader.rs:962-970`), `Material::default()`
and `GpuMaterial::default()` both ship `5.0`, and the no-Material draw path uses
`unwrap_or(5.0)` (`byroredux/src/render/static_meshes.rs:718`), so the
`clamp(_, 0.25, 16.0)` floor in `fresnelSchlickPower` is never reached from a
sentinel. `grayscale_to_palette_scale`: defaults `1.0` at all four levels and is
consumed as `clamp(_, 0.0, 1.0)` (triangle.frag:868, :1160). Map lanes:
`lightingMask` and `backLightingMap` both default to `vec3(1.0)` when the index
is 0 (triangle.frag:2671-2680), so an unmapped flag is neutral rather than
black. GLSL/Rust field order and offsets 396-428 match and are pinned
(`crates/renderer/src/vulkan/material.rs:1890-1898`,
`shaders/include/bindings.glsl:234-243`).

**Soft shadows.** `sun_angular_radius` ships as `GpuCamera.skyTint.w`
(`bindings.glsl:295`, uploaded unconditionally at
`crates/renderer/src/vulkan/context/draw.rs:2314-2319`, so interiors get it
too); shipping default `0.020` rad (`byroredux/src/env_translate.rs:1005`) with
the `< 0.10` debug assert at `byroredux/src/render/sky.rs:151-159`. Both
directional arms take a single tangent-plane cone sample scaled by
`skyTint.w` (triangle.frag:3334-3339, :3514-3516) with the small-angle
derivation documented at the second site. Determinism is per-pixel-per-frame
with no true RNG: `hash2_pixel_frame(uvec2(gl_FragCoord.xy), …frameCount…)` in
the ReSTIR arm (:3320-3322) and `interleavedGradientNoise(gl_FragCoord.xy,
frameCount + …)` in the legacy arm (:3457-3458) — both pure functions of pixel
and frame, so TAA history stays valid. The fast opaque shadow query uses
`gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT`
(`shadow_common.glsl:35`); `shadow_transport.glsl`'s alpha-aware walker
deliberately omits `TerminateOnFirstHit` because it must step past
non-covering layers. Interior XCLL emits one ordinary directional `GpuLight`
with `color_type.w == 2.0` and `params[2] == VisibilityMask::FULL`
(`byroredux/src/render/lights.rs:165-197`), pinned by
`directional_source_contract_tests` (lights.rs:304+) — same cone-sampled,
visibility-tested path as an exterior sun. The disocclusion fallback is not
black: `reprojValid == false` sets `histLen = 1.0; accum = frameContribution;`
(triangle.frag:3399-3402), and `visibility` initialises to `vec3(1.0)` so a
shadow-ray skip past the fade ramp lands on the unshadowed BRDF
(triangle.frag:3293-3295).

---

#### REN-2026-08-30-D18-02: the save-registry exclusion justification for `CloudSimState` asserts the exact inverse of `#803`'s code


- **Severity**: LOW
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:300`
- **Status**: NEW
- **Description**: The not-persisted allow-list entry reads
  `("CloudSimState", "cloud-scroll accumulator, freshly seeded at [0,0] by every apply_worldspace_weather call (see its own #803 doc)")`.
  Both `apply_worldspace_weather` branches do the opposite: they seed it **only when
  absent**, precisely so the accumulator survives.
- **Evidence**:
  - `world_setup.rs:346-348` (WTHR branch) — *"Insert a default-zero state on first
    exterior load only; subsequent loads reuse the existing accumulator so clouds
    resume drift across interior visits"*, implemented as
    `if world.try_resource::<CloudSimState>().is_none() { world.insert_resource(CloudSimState::default()); }`.
  - `world_setup.rs:718-720` (`insert_procedural_fallback_resources`) — the identical
    `is_none()` guard, commented *"same survives-transitions pattern as the WTHR path"*.
  - `cell_loader/sky_params_cleanup_tests.rs:75-93` pins the survival property directly.
- **Impact**: Documentation only today (the cloud scroll offset is cosmetic and
  self-corrects). But this is the justification a future save-completeness reviewer
  reads to decide the resource needs no snapshot entry, and it rests on a property the
  code deliberately does not have — a save/load round-trip does snap the four cloud
  layers back to `[0,0]`, which the stated reason claims already happens every
  worldspace change.
- **Suggested Fix**: Replace the reason with the true one (cosmetic per-frame scroll
  accumulator, wrapped to `[0,1)` by `rem_euclid`, no gameplay observability), or
  register it if the visible snap on load is judged unacceptable.

---

#### REN-2026-08-30-D18-03: `environmentSky`'s doc cites a `triangle.frag` line `#3323` renamed, and mislabels the window-portal escape as a "background write"


- **Severity**: LOW
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `crates/renderer/shaders/include/lighting.glsl:345-347`
- **Status**: NEW
- **Description**: The `#3162` irradiance-units comment justifying why `skyTint` is
  left untouched by the `1/PI` conversion says: *"`skyTint` is already rendered sky
  radiance (see `triangle.frag`'s `skyColor = skyTint.rgb` background write)"*. No
  such line exists at HEAD. `#3323` (commit `19813460`'s predecessor set) rewrote it to
  `vec3 skyColor = exteriorSkyTint.rgb;` (`triangle.frag:1685`), and it is not a
  background write at all — it is the glass window-portal escape branch, which the
  same `#3323` comment block explicitly warns must **not** be generalised
  (*"Do not swap the rest of this shader onto it: everything else reading a stale
  exterior sky from inside is the `#2226` leak"*).
- **Evidence**: `grep -n "skyTint" crates/renderer/shaders/triangle.frag` returns no
  assignment of `skyColor` from `skyTint`; the live `skyTint` consumers are the two
  RT-miss blends (`triangle.frag:1850`, `:2188`), the two `sunAngularRadius` reads
  (`:3336`, `:3514`), and `include/raytrace.glsl:46`.
- **Impact**: The unit-space argument that keeps the Skyrim-DALC and FO3/FNV/Oblivion
  escape paths at parity now points at a line that does not exist, in a branch with
  the opposite interior/exterior contract. A reader following the citation to confirm
  the units invariant lands in the one place the codebase says is a special case.
- **Suggested Fix**: Re-point at the actual radiance-space evidence — the RT-miss
  blend at `triangle.frag:1850` (`skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5`) and its
  `raytrace.glsl:46` twin — and drop the "background write" wording.

### Verified clean (Dimension 18)

- **`REN-2026-08-27-D18-01` is FIXED, not re-filed.** The stale pre-`#1199`
  parenthetical claiming `unload_cell` removes `SkyParamsRes` is gone
  (`weather.rs:723-734`, the `+11` in the diff since `969d81c8`); the replacement
  states the World-lifetime contract and names `#3323`'s dependence on it. Recast as a
  regression guard: any edit re-asserting per-cell release of `SkyParamsRes` breaks
  `exterior_sky_tint`.
- **`#3323` `exterior_sky_tint` end-to-end.** Uploaded every frame from live state
  (`context/draw.rs:2321-2332` reads `sky_params.exterior_zenith_color`, which
  `render/sky.rs:67-71` re-reads from the live `SkyParamsRes` per call — no cached
  or init-time copy). Never zero on an interior: the interior arm of
  `build_sky_params` (`sky.rs:88-113`) sets it explicitly while leaving everything
  else at `SkyParams::default()`; the "no exterior loaded this session" case falls
  back to `SkyParams::default().zenith_color`, and `GpuCamera::default()` carries the
  matching `[0.15, 0.3, 0.6, 0.0]` (`gpu_types.rs:551`). No black/magenta portal is
  reachable — the shader consumer (`triangle.frag:1685-1697`) multiplies by
  `max(texColor.rgb, vec3(0.15))` and never indexes a texture with it.
- **The interior must keep updating.** `weather_system`'s `SkyParamsRes` write block
  (`weather.rs:707-722`) has no interior gate — only the `CellLightingRes` write below
  it does (`:775-786`, `#782`) — and `apply_worldspace_weather` is called from exactly
  one site, `assemble_exterior_streaming` (`world_setup.rs:950`), so an interior load
  cannot clobber the surviving exterior sky.
- **Clock.** `GameTimeRes::tick` is monotone by construction: non-finite / non-positive
  `real_seconds` and `time_scale <= 0` are all rejected, and `advance_hours` rejects
  negatives and carries whole days with `saturating_add` + `rem_euclid`
  (`game_time.rs:53-69`). Both prompt-named pins exist and are live:
  `bootstrap_hour_prefers_the_persistent_live_clock` (`world_setup.rs:1217`) and
  `insert_procedural_fallback_resources_preserves_advanced_game_time` (`:1198`).
- **Sun arc / TOD easing / fade ordering.** `compute_sun_arc` is driven by the CLMT
  `tod_hours` tuple, not hardcoded hours (`weather.rs:121-158`); `build_tod_keys` +
  `pick_tod_pair` handle the pre-midnight wrap through a single monotonic compare
  (`:160-183`); the WTHR cross-fade blends the *target's own* TOD sample against the
  live one **after** both TOD lookups (`:536-633`), and `tod_slot_night_factor` keeps
  fog distance on the same slot pair as the palette (`#897`).
- **Cloud layers.** All four are live and independently gated on
  `tile_scale_N > 0.0 && elevation > 0.0` with distinct drift multipliers
  (`composite.frag:341-405`; `weather.rs:735-773`), scroll rate derived from the WTHR
  `wind_speed` byte via `cloud_scroll_rate_from_wind` (`#1033`), accumulators wrapped
  with `rem_euclid(1.0)`. Note the projection is a view-direction dome
  (`dir.xz / max(elevation, 0.05)`), so there is no camera-translation parallax by
  design — the four layers' visible parallax comes from their differing scroll rates,
  which is what the code says it does.
- **Disabled-WTHR fallback produces no NaN / no black.** `apply_neutral_exterior_fallback`
  installs the documented `procedural_fallback_cell_lighting` constants
  (`env_translate.rs:1259-1282`), and the completed-transition latch
  (`WeatherTransitionRes.done`, `weather.rs:454-461`) still freezes `elapsed_secs`,
  so the `REN-D15-NEW-07` INFINITY→NaN path stays closed. (The hardcoded `hour = 6.0`
  in that function is the subject of D18-01 above.)
- **Interior XCLL directional contract.** `directional_source_contract_tests` exists
  and is the live pin (`byroredux/src/render/lights.rs:304`).
- **Known, documented approximation — not filed.** The RT/GI sky fill uses only
  `skyTint` (zenith) blended against cell ambient (`raytrace.glsl:42-46`,
  `lighting.glsl:334-352`), while the composite paints a three-colour
  zenith/horizon/`SKY_LOWER` gradient (`composite.frag:302-340`); `horizon_color` and
  `lower_color` never reach `GpuCamera`. Both shader sites document this as the
  intended approximation, so it is recorded here rather than re-filed as a defect.

## Dimension 19

---

#### REN-2026-08-30-D19-03: the two POM marchers now agree on the height *channel* but disagree on the height *mip*


- **Severity**: LOW
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/renderer/shaders/include/material_sampling.glsl` (`sampleParallaxHeight`) vs `crates/renderer/shaders/include/ray_hit.glsl` (`resolveRayHitUV`)
- **Status**: NEW
- **Description**: `#3530` correctly made both marchers honour `heightInAlpha`, but
  they still fetch at different LODs. The secondary-ray marcher is explicit and
  uniform — `textureLod(textures[nonuniformEXT(parallaxIdx)], currentUV, 0.0)` at
  all three of its fetch sites (`ray_hit.glsl:337`, `:346`, `:353`). The raster
  marcher uses implicit-LOD `texture(...)` inside a loop with a data-dependent
  `break`, i.e. sampling with implicit derivatives under non-uniform control flow,
  which the GLSL/Vulkan contract leaves undefined.
- **Evidence**: `material_sampling.glsl` — `sampleParallaxHeight` is
  `texture(textures[nonuniformEXT(idx)], uv)` and is invoked at `:109` (pre-loop),
  `:117` (inside the loop, after the `break` at `:112`), and `:125` (post-loop).
  Its own sibling function `perturbNormal` and the primary base-colour fetch have no
  such divergence problem because they are not inside a march.
- **Impact**: On a distant or steeply-foreshortened surface the raster pass marches a
  mip-blurred height field while its reflection marches the sharp mip-0 one, so the
  reflected UV displacement disagrees with the direct one — the same class of
  raster/reflection divergence `#3530` set out to close, one axis over. The
  undefined-derivative aspect is pre-existing (it predates `#3530`) and is not
  observably broken on current drivers, so this is filed at LOW.
- **Suggested Fix**: Compute the LOD once before the loop from the entry UV
  (`textureQueryLod` or an explicit `log2` of the UV footprint) and switch
  `sampleParallaxHeight` to `textureLod`, matching `ray_hit.glsl`'s discipline while
  keeping mip-appropriate filtering. Per the "no speculative Vulkan/shader changes"
  rule, land it behind an A/B capture rather than blind.

---

#### REN-2026-08-30-D19-04: `parallax_alpha_height_bit_is_masked_and_honoured_by_every_reader` cannot enforce the "every reader" it is named for


- **Severity**: LOW
- **Dimension**: Tangent-space & normal maps
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2012-2067`
- **Status**: NEW
- **Description**: The test's doc comment states the invariant in the strongest terms
  (*"**every** reader must mask it before using the value as a bindless index:
  `textures[0x8000000N]` is a wildly out-of-bounds descriptor read"*), but the test
  is a three-file whitelist (`material_sampling.glsl`, `ray_hit.glsl`,
  `triangle.frag`) and its per-file assertion is
  `src.contains("& ~PARALLAX_ALPHA_HEIGHT_BIT")` — an *at-least-once* substring check,
  not a check that every read site masks. A fourth shader added later that reads
  `mat.parallaxMapIndex` raw is not in the list and is not caught; a fourth *unmasked*
  read added to one of the three listed files also passes, because the file already
  contains one masked read elsewhere.
- **Evidence**: The invariant currently *does* hold — a repo-wide grep for
  `parallaxMapIndex` across `crates/renderer/shaders/` returns exactly four read sites
  (`triangle.frag:228`, `:238`, `:1569`; `ray_hit.glsl:296`, `:298`) plus the struct
  declaration in `include/bindings.glsl:125`, and all of them mask. So this is a
  guard-strength gap, not a live defect.
- **Impact**: The one mechanism protecting against an out-of-bounds bindless
  descriptor read cannot actually fail on the shape of mistake it exists to catch.
  Given `#3530` is three days old and the bit will accrue readers (`water.frag` and
  the RT hit shaders are the obvious next ones), the whitelist will silently go stale.
- **Suggested Fix**: Enumerate `crates/renderer/shaders/**/*.{frag,vert,comp,glsl}`
  at test time, and for every file containing `parallaxMapIndex` assert that each
  occurrence is either the `include/bindings.glsl` declaration or is immediately
  followed by `& ~PARALLAX_ALPHA_HEIGHT_BIT` / `& PARALLAX_ALPHA_HEIGHT_BIT`. The
  sibling `NORMAL_ALPHA_SPEC_BIT` (same value, same hazard) deserves the same
  treatment in the same pass.

### Verified clean (Dimension 19)

- **`#3530` masking — the prompt's highest-value check — is correct at every site.**
  A repo-wide grep finds exactly four shader reads of `parallaxMapIndex`, and all four
  mask bit 31 before using it as an index or as a "is a height map bound" test:
  `triangle.frag:228` (POM gate), `:238` (the raw value handed to
  `parallaxDisplaceUV`, which re-masks internally at `material_sampling.glsl:49`),
  `:1569` (the `RENDER_DEBUG_MATERIAL_ROLE` view), and `ray_hit.glsl:296-298`. No
  unmasked `textures[0x8000000N]` read is reachable.
- **Both marchers honour the channel.** `material_sampling.glsl:33`
  (`heightInAlpha ? texel.a : texel.r`, used at all three fetch sites) and
  `ray_hit.glsl:336-355` (`heightInAlpha ? heightTexel.a : heightTexel.r`, likewise
  all three). Gate parity too: both bail on a zero masked index, a non-positive
  `parallaxHeightScale`, and `DBG_BYPASS_POM`, and both read the debug bits from the
  same `floatBitsToUint(jitter.z)` source.
- **`#1496` relative-position discipline holds in the new code.** The POM call passes
  `fragWorldPosRel` (`triangle.frag:236`), not the absolute `fragWorldPos`
  reconstructed at the top of `main()`; the only `worldPos` consumer inside
  `parallaxDisplaceUV` is the `dFdx`/`dFdy` derivative-tangent fallback. Both
  `perturbNormal` call sites (`triangle.frag:511`, `:522`) likewise pass
  `fragWorldPosRel`, as do the two `geometricNormal` / flat-shading derivative pairs
  (`:175-188`).
- **`perturbNormal` default-on, `DBG_BYPASS_NORMAL_MAP = 0x10` opt-out intact.**
  Both call sites are gated only on `(dbgFlags & DBG_BYPASS_NORMAL_MAP) == 0u`
  (`triangle.frag:466`, `:519`); the constant is generated from
  `shader_constants_data.rs:605` into `include/shader_constants.glsl:213` (value `16u`)
  rather than hand-copied. The `0x20` slot is explicitly retired as `DBG_RESERVED_20`.
- **The `#786` `CalcTangentSpace` swap is still correct.** `extract_tangents_from_extra_data`
  reads the **bitangent** half (offset `num_verts * 12 + i * 12`) into
  `Vertex.tangent.xyz` and uses the tangent half only for the sign
  (`tangent.rs:69-108`), matching nifly `Geometry.cpp:2084-2085`; the blob is rejected
  outright unless `blob.len() == num_verts * 24`.
- **Sign convention and Z-up→Y-up lockstep.** All three producers derive the sign
  through the single shared `crate::types::bitangent_sign(n, t, b)` helper
  (`tangent.rs:108`, `:335`, `:537`), so the operand order cannot drift. The BS inline
  path converts the tangent xyz through the same
  `byroredux_core::math::coord::zup_to_yup_pos` used for positions and normals and
  passes `.w` through unchanged (`bs_tangents_zup_to_yup`, `tangent.rs:350-359`) —
  correct, because `(x,y,z) → (x,z,-y)` has determinant `+1`, so handedness is
  preserved. No import path converts the normal without converting the tangent.
- **`VF_TANGENTS | VF_NORMALS` gating is on the vertex descriptor, not on BSVER.**
  `blocks/tri_shape/bs_tri_shape.rs:375` — `vertex_attrs & VF_TANGENTS != 0 &&
  vertex_attrs & VF_NORMALS != 0` — with the stride accounting at `:311-315` matching
  (`+4` normal + bitangent-Y, `+4` tangent + bitangent-Z). The four-branch precedence
  ladder in `import/mesh/bs_tri_shape.rs:192-227` (SSE-reconstructed → inline →
  `synthesize_tangents` → `synthesize_tangents_yup`) is intact, including `#2817`'s
  `normals_authored && uvs_authored` authorship gate that stops a fabricated
  `[0,1,0]` placeholder from being synthesised into a "tangent" basis.
- **`perturbNormal` Path-1 degenerate guards.** The post-Gram-Schmidt
  `dot(Tproj, Tproj) < 1e-8` bail (`#2815`) and the mid-triangle `w ∈ (-1,1)`
  re-clamp to `±1` (`#2512`) are both present and match the sibling guards in
  `parallaxDisplaceUV` and `getRayHitTangentFrame`.
- **Not re-filed per instructions**: `#3177` (Z-up `synthesize_tangents` never
  normalizes N), `#3176` (degenerate-tangent guard emits a zero tangent), `#3071`
  (slot-7 back-lighting map), `#3305`, `#3073` (`parallax_height_scale` /
  `parallax_max_passes` bypass the canonical `Material` with duplicated `0.04` / `4.0`
  defaults — re-confirmed still accurate; `legacy_properties.rs:281-283` adds two more
  copies of that magic pair, which is within `#3073`'s existing scope).
- **Not a tangent-space bug**: no finding here should be read as an explanation for
  "chrome posterized walls" — that remains the magenta-checker placeholder × a
  correctly-loaded normal map (`tex.missing` first).

---

#### REN-2026-08-30-D20-01: `depth.stats` runs a full-resolution depth decode inside the frame-blocking exclusive debug system

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`), `crates/core/src/ecs/components/camera.rs` (`Camera::analyze_depth_field`), `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_finish_readback`)
- **Status**: Open
- **Description**: Both halves of the depth-capture round trip do O(width × height) work on threads that own the frame. `depth_capture_finish_readback` builds `samples: Vec<f32>` by `chunks_exact(4).map(f32::from_le_bytes).collect()` at the top of `draw_frame`, before the swapchain acquire. `DepthStatsCommand::execute` then hands that whole slice to `analyze_depth_field`, which walks every sample and does `codes[band].insert(z.to_bits())` into a per-band `HashSet<u32>` — and it runs inside `DebugDrainSystem`, documented at `crates/debug-server/src/system.rs:1` as "Late-stage exclusive system … Runs after all other systems, with exclusive access to the World", i.e. inside the frame.
- **Evidence**:
  - `depth_capture.rs` readback: `let samples: Vec<f32> = slice[..expected].chunks_exact(4).map(...).collect();` — `expected = width * height * 4`, and `extent = self.frame_extents.render`, so 1920×1080 is 2 073 600 samples / 8.3 MB, 3840×2160 is 8 294 400 / 33.2 MB.
  - `camera.rs` `analyze_depth_field`: `let mut codes: Vec<HashSet<u32>> = vec![HashSet::new(); edges.len() - 1];` then one `insert` per non-background sample.
  - Dispatch route: `crates/debug-server/src/evaluator.rs:430` (`reg.execute(world, expr)`) is called from `eval_request`, driven by `DebugDrainSystem`.
  - The result also *stays* resident: `depth_capture_result` holds the `Vec<f32>` until the next `depth.stats` `take_result()`, and `depth_capture_staging` is never shrunk or freed except at teardown (`ensure_depth_capture_staging` only grows) — so one `depth.stats` pins roughly `2 × w × h × 4` bytes for the process lifetime.
- **Impact**: A single `depth.stats` costs one full-frame hash-set build on the render thread plus a multi-megabyte allocation inside `draw_frame`. The resulting hitch lands in the very `CpuFrameTimings` / metrics surfaces the operator is reading next to it, so the diagnostic perturbs the numbers it sits beside. Diagnostic-only and one-shot per invocation, hence Low.
- **Suggested Fix**: Replace the per-band `HashSet<u32>` with a per-band `Vec<u32>` plus `sort_unstable()` + `dedup()` at the end — identical `distinct_codes`, no hashing, and 4 bytes/sample peak instead of a hash table's load-factor overhead. Optionally give `analyze_depth_field` an explicit sample stride (reported in the output) so a 4K capture can be analysed at a fixed budget.

---

---

#### REN-2026-08-30-D20-03: the new depth-capture path's two ordering invariants are held by comments only, with no source-scan guard

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs`, `crates/renderer/src/vulkan/context/draw.rs:1708` + `:3684`
- **Status**: Open (missing regression guard)
- **Description**: Both fence and layout invariants verify **clean today**, but neither is pinned. (a) `depth_capture_finish_readback()` at `draw.rs:1708` sits after the `wait_for_fences(&[in_flight[frame], in_flight[prev]], …)` at `draw.rs:1624-1636`, which waits on *both* FIF fences (`MAX_FRAMES_IN_FLIGHT == 2`), so the previous frame's copy is genuinely retired — the same discipline `screenshot_finish_readback` has, one line above. (b) `depth_capture_record_copy(cmd)` at `draw.rs:3684` sits immediately after `copy_depth_to_history(cmd)`, which is what makes its documented `DEPTH_STENCIL_READ_ONLY_OPTIMAL` precondition true. Move either call and the failure is silent-to-`cargo test`: a stale/garbage readback in case (a), a validation-layer layout error or corrupt samples in case (b).
- **Evidence**:
  - `grep -rn "depth_capture" --include="*.rs" crates byroredux | grep -i test` returns exactly one hit: the `unsafe fn` safety-doc scanner added to `frame_upscaler.rs:1328,1368` under `#3308`. There is no test that pins where either call site sits.
  - No `depth.stats` test in `byroredux/src/commands_tests.rs` (45 test fns, none reference `DepthStats` or `DepthCapture`). `analyze_depth_field` itself *is* unit-tested (`camera.rs:549,579,605`) — only the plumbing is untested.
  - The repo already uses source-scan guards for exactly this class of cross-file invariant: `egui_pass.rs::dependency_chain_tests`, `resize.rs::egui_pass_rebuilds_fully_on_swapchain_format_change`, `resize.rs::egui_framebuffer_recreate_failure_destroys_the_taken_pass`, `post_passes.rs`'s `record_post_passes_has_no_error_propagation_after_the_svgf_latch`, and `frame_upscaler.rs`'s own safety-doc scanner.
- **Impact**: A future refactor of `draw_frame`'s tail (the region has been restructured three times: `#1748`, `#2258`, `#3426`) can move either call with no test signal. The capture exists to be trusted as ground truth against `depth_resolution_at`; a silently-stale one is worse than no capture.
- **Suggested Fix**: Add a `#[cfg(test)]` source scan over `include_str!("draw.rs")` asserting (a) the byte offset of `depth_capture_finish_readback()` is after the `wait_for_fences` call, and (b) `self.depth_capture_record_copy(cmd)` follows `self.copy_depth_to_history(cmd)` with no other `self.` statement between them. Same shape as the existing `egui_pass` / `post_passes` scanners.

---

---

#### REN-2026-08-30-D20-04: the `bench:` line reports 12 of the 14 GPU brackets, and its `tlas_ms=` is the host-side number

- **Severity**: Low
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/app_events.rs:888-926` (bench GPU array), `byroredux/src/main.rs:91-104` (`BENCH_GPU_KEYS`)
- **Status**: Open
- **Description**: `gpu_timers.rs` owns 14 brackets (`QUERIES_PER_FRAME == 28`, `BIT_SKIN_DISPATCH … BIT_PRESENTATION`), and every other consumer surfaces all 14 — `systems/metrics.rs:143-195` (debug-UI grid), `systems/debug.rs::gpu_breakdown`, `context/mod.rs:2234-2266` (the `SkinCoverageStats` fill). The bench summary copies only 12: `gpu_tlas_build_ms` and `gpu_caustic_splat_ms` are absent from both the value array and `BENCH_GPU_KEYS`, despite the comment above the array claiming "Full per-pass GPU breakdown."
- **Evidence**:
  - `app_events.rs:894-908` — the 12-element array: `skin_dispatch, skin_blas_refit, taa, upscale, main_render, svgf, composite, ssao, bloom, volumetrics, cluster_cull, presentation`. No `gpu_tlas_build_ms`, no `gpu_caustic_splat_ms`.
  - `main.rs:91` `const BENCH_GPU_KEYS: [&str; 12]` and `main.rs:116 fn bench_gpu_inactive_token(active: [bool; 12])` — so the `gpu_inactive=` token cannot name either missing bracket either.
  - `app_events.rs:850`: `let tlas_ms = ft.tlas_build_ns as f64 / n / 1e6;` — `FrameTimings`, i.e. the **host** TLAS build cost. It is printed inside the CPU group `[fence= … tlas_ms= … submit=]`, but `gpu_tlas_build_ms` never appears anywhere on the line, so `tlas_ms=` is the only `tlas` token a sweep harness can extract.
  - `main.rs:205-211` (`bench_gpu_keys_match_the_reported_bracket_order`) asserts `len() == 12` and spot-checks indices 4 and 11 — it pins the current 12 rather than catching the gap.
- **Impact**: The FSR benchmark matrix and the four sweep harnesses that parse this line have no device-side TLAS-build or caustic-splat number. `gpu_timers.rs`'s own doc calls out first-cell-load TLAS spikes as the thing the bracket exists to catch, and the caustic bracket is one of the five Phase-7 brackets added specifically to close the "438 ms unaccounted" gap — neither is measurable from a bench run. The `tlas_ms=`/`gpu_tlas_build_ms` name collision invites reading the host number as the device one.
- **Suggested Fix**: Widen the array, `BENCH_GPU_KEYS`, and `bench_gpu_inactive_token`'s parameter to 14, appending `tlas_build` and `caustic_splat` as `gpu_tlas_build=` / `gpu_caustic_splat=` (append, so existing `key=<float>` extractors keep matching), and raise the `len()` assertion to 14. Consider renaming the CPU token to `cpu_tlas_ms=` in the same pass, or leave it and note the distinction in the comment.

---

---

#### REN-2026-08-30-D20-05: `depth.stats` contradicts `analyze_depth_field`'s explicit degenerate-camera contract

- **Severity**: Trivial
- **Dimension**: Debug/Telemetry
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)
- **Status**: Open
- **Description**: `analyze_depth_field` returns early on `self.near <= 0.0 || self.far <= self.near` with `total` populated but `cleared == 0`, `invalid == 0`, `bands` empty — a contract its own test pins as "must report nothing rather than … emit bands it cannot justify" (`camera.rs:602-608`). The command does not honour that: it computes `geometry = stats.total - stats.cleared - stats.invalid`, which on that path equals the full sample count, and *also* prints "(no geometry in frame — every sample is background)" because every band has `samples == 0`. The two lines contradict each other and neither says the camera was rejected.
- **Evidence**:
  - `camera.rs:322-325`: `if self.near <= 0.0 || self.far <= self.near { return stats; }` with `stats.total` already set from `encoded.len()`.
  - `depth.rs`: `stats.total - stats.cleared - stats.invalid` for the `geometry=` field, then `if stats.bands.iter().all(|b| b.samples == 0)` → "(no geometry in frame …)".
  - Reachability is narrow: it needs `near <= 0` or `far <= near` on the live `Camera`, which no CLI flag or `FOV_SETTING_ID` slider produces. Filed as Trivial for that reason, not because the mismatch is theoretical — the contract is explicit and tested on the analytic side.
- **Impact**: A misconfigured camera reports a full frame of "geometry" with no bands, which reads as a broken readback rather than a rejected camera — the opposite of what `analyze_depth_field`'s doc says a disagreeing capture means.
- **Suggested Fix**: In `execute`, short-circuit on `stats.bands.is_empty() && stats.total > 0` with an explicit "degenerate camera (near={}, far={}) — analysis rejected" line before the per-band table.

---

## Verified clean (no finding)

- **GPU timers, `#2821` `_active` regression guard — HOLDS.** All 14 brackets are `_active`-gated at every reader: `systems/metrics.rs::gpu_bracket_ms` (14/14), `systems/debug.rs::gpu_breakdown` (14/14), `commands/assets.rs` (`ms(value, active)` closure), `commands/world_info.rs::CtxUpscalerCommand` (now `format_gpu_bracket_ms`, fixed since the last sweep), `context/mod.rs::fill_upscaler_telemetry` (`map_or((0.0, false), …)`), `app_events.rs` bench (`gpu_inactive=` companion token). `snapshot_from_bits` sets each `_ms` only under its bit.
- **New presentation bracket (`Q_PRESENTATION_START/END = 26/27`).** Paired in `post_passes.rs::record_presentation_pass` (`:1107` start, `:1127` end), both inside the same `if let Some(ref mut timers)`; `cmd_presentation_end` sets `BIT_PRESENTATION`; `QUERIES_PER_FRAME == 28` accounts for it; `only_the_set_bit_is_reported_active` asserts `!snap.presentation_active`.
- **Timer pool discipline.** One `VkQueryPool` per FIF slot; host-side `reset_query_pool` after the both-fence wait in `read_and_reset`; batched read deliberately without `WAIT` (documented, correct — WAIT on unwritten queries blocks forever); `GpuPerFrameTimers::new` returns `Ok(None)` when `caps.gpu_timers_supported()` is false and every draw-path use is `if let Some(ref mut timers)` — no unwrap; `destroy()` null-checks and is in the allocator-**independent** teardown block (`teardown.rs:209-219`, `REG-06`/`#1638` invariant intact).
- **Depth capture (a) fence-before-read.** `draw.rs:1624-1636` waits `in_flight[frame]` *and* `in_flight[prev]` — with `MAX_FRAMES_IN_FLIGHT == 2` that is every slot — so the frame-N copy is retired when frame N+1 reads it. The non-coherent `invalidate_mapped_memory_ranges` (`#2740`/REN-D4-04) is present and uses the shared `aligned_flush_range` helper, matching the screenshot sibling.
- **Depth capture (b) staging teardown.** `destroy_depth_capture_staging()` is called at `teardown.rs:244`, while `self.allocator` is still `Some` (it is taken well below, after `destroy_allocator_owned_resources` at `:267`), so both the `VkBuffer` and the `gpu_allocator` allocation are released. The in-frame regrow path (`ensure_depth_capture_staging` → `destroy_depth_capture_staging`) is sound because of the both-fence wait above. Extent is captured at record time (`depth_capture_pending_readback = Some(extent)`) and `slice.len() < expected` is checked, so a resize between record and readback cannot over-read — the REG-02 / `#1634` invariant the screenshot path documents.
- **Depth capture (c) command registration.** `byroredux/src/commands/mod.rs:37` `mod depth;`, `:52` `use depth::*;`, `:68` `registry.register(DepthStatsCommand);`. Reachable from `byro-dbg`: `tools/byro-dbg/src/main.rs:79-84` falls through to `DebugRequest::Eval`, and `crates/debug-server/src/evaluator.rs:424-435` looks the first whitespace token up in `CommandRegistry` before Papyrus evaluation (the `#518` dot-name path).
- **Depth capture (d) no-frame behaviour.** `depth.stats` never blocks: `DepthCaptureBridge::request` is a `store(true, Release)` and `take_result` a `lock().take()`; with no result yet it returns the "armed" line immediately. Under `--bench-hold` frames keep rendering (`app_events.rs:1085` skips `event_loop.exit()` and the loop keeps ticking), so the second invocation reports. No hang, no timeout path needed.
- **Depth image extent.** `depth_capture_record_copy` copies at `self.frame_extents.render`, which is exactly what `create_depth_resources` is given (`resize.rs:246-249`), so the `cmd_copy_image_to_buffer` extent cannot exceed the image. `find_depth_format` picks `D32_SFLOAT` or `D16_UNORM` — no stencil aspect, so the DEPTH-only barrier `subresource_range` is valid.
- **egui pass mechanics.** `loadOp = LOAD`, `initialLayout == finalLayout == PRESENT_SRC_KHR`; recorded after `record_post_passes` (i.e. after presentation, per `#3426`) and before `screenshot_record_copy`; supplies its own `in_dep` (`#1433`) and an explicit `out_dep`; `dependency_chain_tests` pins both halves against `presentation.rs`. Empty-primitive path skips begin/end entirely and is layout-neutral. RP begin/end stays balanced on `cmd_draw` failure (REG-05 / `#1637`). Resize is format-gated (`#2475`) with a full rebuild on format change and pass-destroy on framebuffer-recreate failure, both source-scan-tested in `resize.rs:1650-1720`. `Option<EguiPass>` is `take()`n and `destroy()`d at `teardown.rs:182-184`, before the device and before the allocator `Arc::try_unwrap` leak guard, with `#1427`'s final `free_textures` flush.
- **FSR destroy path (`#2829` pairing).** `create`/`recreate` allocate exactly `output_images` / `output_views` / `output_allocations` plus the SDK `context`; `destroy_device_objects` drops the context and `destroy_allocations` drains all three vectors. Ordering is honoured at runtime: `destroy_device_objects` at `teardown.rs:239` precedes `destroy_allocations` at `teardown.rs:128` (reached via `destroy_allocator_owned_resources`, called at `:267`), matching the "SDK context first" doc contract.
- **`dispatches_skipped`** confirmed as the skin-coverage counter in `skin_compute.rs`, not a `GpuPerFrameTimers` field. Not touched here.
- **No `mem` / `mem.stats` command exists** — confirmed absent from `build_command_registry`.

## Note (not filed as a finding)

`cargo fmt --check` fails on `byroredux/src/systems/metrics.rs:217` (the `#3467`
`geometry_rebuild` insert, indented 4 instead of 8) and
`crates/renderer/src/vulkan/context/draw.rs:3016` (the `#3469` `.filter`/`.map`
chain). Both are new since `969d81c8`, but the repo already carries eight other
pre-existing `fmt` diffs across `byroredux` + `byroredux-renderer`, so this is
existing repo-wide drift rather than a Dimension-20 regression. A single
`cargo fmt` pass clears all of them.

---

#### REN-2026-08-30-D23-01: both authoritative FSR docs still carry "UI composited before upscale" as open scope after #3426 closed it


- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `docs/engine/fsr3-troubleshooting.md` (lines 74–79), `docs/engine/fsr3-upscaler-integration-plan.md` (lines 3–7, 30–33, 137, 152, 158, 641, 735–738)
- **Status**: NEW
- **Description**: Commit `b28acb0c` (#3426) moved the Scaleform/Ruffle overlay draw out
  of the geometry pass and into `PresentationPipeline::dispatch`, i.e. after tone-map and
  after upscale, at output resolution. Both documents the audit skill names as
  authoritative for this dimension still describe the pre-#3426 world, including an
  operator-facing troubleshooting entry that tells the reader the ghosting is expected and
  "the fix is moving it after upscale".
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` (`UiOverlayDraw`, `record_overlay`,
    `dispatch(..., overlay: Option<UiOverlayDraw>)`) and
    `crates/renderer/src/vulkan/pipeline.rs` (`create_ui_pipeline`, now built against the
    presentation render pass with a single colour-blend attachment) implement the move; the
    `ui_overlay_composites_after_the_tone_map_draw` test in `presentation.rs` pins it and
    also pins that `context/geometry_pass.rs` no longer contains `pipeline_ui`.
  - `fsr3-troubleshooting.md:77` — "**The Scaleform/Ruffle UI overlay.** It is still
    composited *before* the upscale, so it goes through temporal reconstruction… the fix is
    moving it after upscale."
  - `fsr3-upscaler-integration-plan.md:5-7` — "Four items are carried as known scope rather
    than done: … the two phase-4 items below (transparency split, UI composited after
    upscale) remain open"; `:33` — "the Scaleform/Ruffle overlay + reticle are still
    composited before upscale rather than after (4.5)"; `:735-738` — "Until the UI moves,
    the Scaleform overlay is temporally reconstructed along with the scene and writes no
    mask".
  - The reticle half of item 4.5 is also done and always was post-presentation: the only
    crosshair in the tree is `crates/debug-ui/src/panels.rs` (`show_crosshair`), drawn by
    the egui pass, which `context/draw.rs:3721` records *after* the presentation pass.
- **Impact**: Doc-only. But this dimension's whole method is "verify the premise against
  current code", and these two files are the premise source. A future auditor or fixer
  reading them will re-file a closed item, or chase UI ghosting that the code no longer
  produces. Exactly the stale-premise class `feedback_audit_findings` exists for.
- **Needs RenderDoc**: no
- **Suggested Fix**: In `fsr3-troubleshooting.md`, delete the UI bullet from the
  "expected to ghost" list (or rewrite it as "the overlay is composited after upscale since
  #3426 and is never reconstructed"). In `fsr3-upscaler-integration-plan.md`, move 4.5 from
  carried scope to complete in the status header, §"Phase 4 landed", and the phase-5
  deferral note, leaving 4.1 (transparency split) and the FP32 permutation as the genuine
  carried items.

---
- **Cross-dimension corroboration**: Found independently four times — also as *D8-01*, *D11-06* and *D4-02*. `#3426` invalidated the same claim in two FSR documents plus the frame-graph prose; one fix closes all four.

---

#### REN-2026-08-30-D23-02: `is_fsr_dispatch_active()` promises "actually dispatching this frame", but `force_native_debug` blits while it still returns true


- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`VulkanContext::is_fsr_dispatch_active`, ~line 1516) vs `crates/renderer/src/vulkan/context/post_passes.rs` (`record_upscale_pass`, `force_native_debug`, line 994)
- **Status**: NEW
- **Description**: `is_fsr_dispatch_active()` is the single accessor #2518 introduced so
  that "is FSR's projection jitter in play?" has exactly one answer. Its doc states the
  cases it covers "fall back to an **unjittered** native blit". `record_upscale_pass` adds a
  third suspension case the accessor does not know about: when
  `render_debug_requires_raw_output(self.render_debug_flags, self.render_debug_mode.shader_value())`
  is true it passes `force_native_blit: true` into `FrameUpscaler::record`, which takes the
  bridge branch and returns without dispatching — while `is_fsr_dispatch_active()` stays
  `true`, so the jitter gate at `draw.rs:2039-2066` still applies the FSR sub-pixel offset
  to the projection. A raw-output debug view is therefore rendered *jittered but never
  reconstructed*, then `LINEAR`-blitted render→output. This is structurally the same
  condition #2519 identified for the dispatch-failure path and closed there with
  `new_dispatch_failure` → `signal_temporal_discontinuity`.
- **Evidence**:
  - `post_passes.rs:994-1016` computes `force_native_debug` locally and passes it only into
    `record`; it is never read at the jitter site. `grep -rn render_debug_requires_raw_output`
    returns exactly one production call site.
  - `frame_upscaler.rs:445-460` — `if force_native_blit || !self.is_fsr_dispatch_active()`
    → `record_native_blit(..., SHADER_READ_ONLY_OPTIMAL)` → `return`, with
    `dispatched_this_frame = false` and no `dispatch_failure` latch.
  - Because `dispatched_this_frame` stays false, `draw.rs:3900-3908` never calls
    `FsrTemporalState::mark_dispatch_completed`, so the jitter index freezes AND
    `reset_pending` keeps its last value (`false` after any prior successful dispatch). The
    first frame back at `RENDER_DEBUG_FINAL` therefore dispatches with `reset: false`
    against reconstruction history that is stale by the length of the debug session.
  - `crates/renderer/src/vulkan/context/render_debug.rs:9-14` — `set_render_debug_mode`
    only logs and assigns; it does not call `signal_temporal_discontinuity`, which is what
    every other history-invalidating transition in the renderer does
    (`context/mod.rs:2039`).
- **Impact**: Debug-tooling only, never on a shipping frame. Raw debug views carry a fixed
  sub-pixel offset and one to two frames of stale-history reconstruction appear on the way
  back to `Final`. It is filed because the accessor's *documented contract* is now false at
  one of its call sites, and that contract is what keeps the jitter, the DOF gate and the
  `DBG_VIZ_FSR_TEMPORAL` view from drifting apart again.
- **Needs RenderDoc**: no — entirely source-provable.
- **Suggested Fix**: Either fold the raw-output predicate into `is_fsr_dispatch_active()`
  (it already has `self.render_debug_flags` / `render_debug_mode` in scope), so the frame is
  unjittered like every other non-dispatching frame; or, if a jittered raw view is wanted,
  say so in the accessor's doc and have `set_render_debug_mode` call
  `signal_temporal_discontinuity` on any transition that crosses the
  `render_debug_requires_raw_output` boundary.

---

---

#### REN-2026-08-30-D23-03: `PresentationPipeline::recreate` is dead code; the sole resize path open-codes destroy + `new`


- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/presentation.rs:686` (`PresentationPipeline::recreate`)
- **Status**: NEW
- **Description**: `recreate` carries the non-obvious contract for rebuilding this pass —
  capture the `VulkanContext`-owned `health_buffers` before `destroy` overwrites them,
  re-read the borrowed (not owned) `overlay_pipeline_layout`, then `Self::new`. Nothing
  calls it. `recreate_swapchain_core` open-codes the same sequence
  (`resize.rs:1007-1050`: `presentation.take()` → `destroy` → `upscaler.recreate` →
  `PresentationPipeline::new(..., &health_handles, ...)`). Because both the struct and the
  method are `pub` inside `pub mod presentation`, rustc raises no dead-code warning.
- **Evidence**: `grep -rn "presentation" $(git ls-files '*.rs') | grep -i recreate` yields
  exactly one hit, `resize.rs:1050`, and that is the `.context("recreate presentation
  pipeline")` string on the `PresentationPipeline::new` call — not a call to `recreate`.
- **Impact**: Two copies of one lifecycle contract, one of which is never exercised by any
  test or run. A future change to the health-buffer or overlay-layout ownership rules can be
  made in the unused copy and appear correct.
- **Needs RenderDoc**: no
- **Suggested Fix**: Delete `recreate`, or make `recreate_swapchain_core` call it (it is
  the shape that documents the contract). Deleting is the smaller change; the resize site's
  own comments already carry both invariants.

---
- **Cross-dimension corroboration**: Found independently three times — also as *D11-05* and *D5-05*.

---

#### REN-2026-08-30-D23-04: `UI_PIPELINE_DYNAMIC_STATES`' contract comment points at a call site, a field, and a const that #3426 removed


- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:840-849` and `:960-972` (`UI_PIPELINE_DYNAMIC_STATES`, `create_ui_pipeline`)
- **Status**: NEW
- **Description**: The `#663` contract block instructs the next editor that the overlay call
  site "lives in `vulkan/context/draw.rs` (post-`cmd_bind_pipeline(pipeline_ui)`)" and that
  a `_UI_PIPELINE_DYNAMIC_STATES_LEN` const assert "at the call site" fires when the list
  grows. After #3426 none of those three exist: the call site is
  `presentation.rs::record_overlay`, there is no `pipeline_ui` symbol anywhere in the tree,
  and the live compile-time guard is named `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`. The
  guard itself is correct and in the right place — only the pointer to it is wrong, and the
  pointer is the entire mechanism by which the contract is discovered.
- **Evidence**:
  - `grep -rn "pipeline_ui" crates byroredux` → four hits, all inside comments
    (`presentation.rs:869` is a test asserting its *absence* from `geometry_pass.rs`;
    `pipeline.rs:844`, `:963`, `:965` are this stale block).
  - `grep -rn "_UI_PIPELINE_DYNAMIC_STATES_LEN" $(git ls-files '*.rs')` → one hit,
    `pipeline.rs:969`, inside the same comment.
  - `presentation.rs::record_overlay` contains the real
    `const _UI_OVERLAY_DEFENSIVE_STATE_INVARIANT: () = { assert!(UI_PIPELINE_DYNAMIC_STATES.len() == 2, …) }`
    plus the matching `cmd_set_viewport` / `cmd_set_scissor` pair.
  - Secondary, same block: `create_ui_pipeline`'s doc now says "`extent` is therefore the
    output extent", but the body is `let _ = extent;` — the parameter has been inert since
    viewport/scissor went dynamic (#578) and is now a misleading signal that the overlay
    pipeline is extent-bound (it is not, which is why a resize can rebuild it safely).
- **Impact**: Doc-only, but it is a doc whose stated job is to route a future editor to the
  one place that must change in lockstep. As written it routes them to a file that no longer
  contains the overlay draw.
- **Needs RenderDoc**: no
- **Suggested Fix**: Repoint the two comment blocks at
  `presentation.rs::record_overlay` / `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`, and either
  drop the `extent` parameter from `create_ui_pipeline` or note in the doc that it is
  retained only for signature symmetry.

---
- **Cross-dimension corroboration**: Found independently four times — also as *D11-04*, *D8-02* and *D5-07*: the `#663` UI dynamic-state contract, `pipeline.rs`'s UI-pipeline contract, and the `UI_PIPELINE_DYNAMIC_STATES` comment all still point at the retired `pipeline_ui` field and its former geometry-pass call site.

---

#### REN-2026-08-30-D23-05: the presentation pass's incoming `SUBPASS_EXTERNAL` dependency no longer describes all of the pass's consumers after #3426 (observation — needs validation run)


- **Severity**: LOW (latent; no live hazard found by source inspection)
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/presentation.rs` (`create`, the `incoming` `vk::SubpassDependency`)
- **Status**: NEW — **OBSERVATION ONLY, NOT A PROPOSED EDIT**
- **Description**: The incoming dependency is
  `src = COMPUTE_SHADER|TRANSFER|COLOR_ATTACHMENT_OUTPUT / SHADER_WRITE|TRANSFER_WRITE|COLOR_ATTACHMENT_WRITE`
  → `dst = FRAGMENT_SHADER|COLOR_ATTACHMENT_OUTPUT / SHADER_READ|COLOR_ATTACHMENT_WRITE`.
  Both scopes, and the long `#2465` / `#2143` comment blocks around them, were written when
  this pass contained exactly one fragment-only fullscreen draw sampling the upscale output.
  #3426 added a second draw into the same subpass that additionally reads a vertex buffer
  and an index buffer (`VERTEX_INPUT` / `INDEX_READ` / `VERTEX_ATTRIBUTE_READ`) and the
  scene instance SSBO in the **vertex** stage (`ui.vert`, `set = 1, binding = 4`). Neither
  stage is in the dst scope, and the `#2143` block's own rule for the *outgoing* dependency
  — "the dst scope below names those two consumers rather than being maximally wide, so it
  stays a description of the frame graph. A third consumer means extending it" — was not
  applied to the incoming side when the third consumer arrived.
- **Evidence**:
  - `presentation.rs::record_overlay` records `cmd_bind_vertex_buffers`,
    `cmd_bind_index_buffer`, `cmd_draw_indexed` inside the open render pass; `ui.vert`
    declares `layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer`.
  - `post_passes.rs::record_presentation_pass` sources those handles from
    `self.mesh_registry.get(ui_quad)` and `self.scene_buffers.descriptor_set(frame)`.
  - **Why no live hazard is claimed**: the UI quad's vertex/index buffers are written once
    at `register_ui_quad` (`context/resources.rs:362`) through `mesh_registry.upload`, i.e.
    a separate fenced one-time submit, not into this command buffer. The instance SSBO is a
    host-mapped write (`scene_buffer/upload.rs::upload_instances` → `mapped_slice_mut`),
    made visible by `vkQueueSubmit`'s implicit host-write dependency, not by an in-command
    barrier. The UI *texture* read in `ui.frag` is a fragment-stage `SHADER_READ` behind a
    `TRANSFER_WRITE` producer, which the existing `TRANSFER → FRAGMENT_SHADER / SHADER_READ`
    limb already covers. So the gap is descriptive, not (today) a hazard.
  - Also noted and *not* acted on: the `#2465` "MEASURED, deliberately unchanged" block
    records a `BYRO_VALIDATION=1` run of 300 FNV-exterior frames dated **2026-08-14** — i.e.
    before #3426 landed. That measurement no longer covers this pass's current contents.
- **Impact**: The dependency is no longer a truthful description of the frame graph, which
  is the property its own comments claim make narrow scopes safe here. If a future change
  makes any of the three newly-read resources a same-command-buffer write (a GPU-side
  instance-buffer compaction, a per-frame UI mesh rebuild), the missing limbs become a real
  WAR/RAW with nothing in `cargo test` able to see it.
- **Needs RenderDoc**: **yes** — validation-layer run required before *any* scope change,
  and a fresh `BYRO_VALIDATION=1` (sync validation) pass over a frame with a menu open is
  needed to re-establish the `#2465` measurement for the post-#3426 pass. No barrier edit is
  proposed here.
- **Suggested Fix**: None proposed. Minimum action: re-run the `#2465` measurement with the
  overlay actually drawing (`--menu` route, `docs/smoke-tests/m48-menu-load.sh`) and record
  the result in the existing comment block with its new date, so the next reader is not
  relying on a pre-#3426 measurement.

---

---

#### REN-2026-08-30-D23-06: no regression guard pins the resize ordering that keeps the presentation descriptor off a destroyed upscale view


- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:1005-1051` (`recreate_swapchain_core`), test module at `resize.rs:1394+`
- **Status**: NEW
- **Description**: `FrameUpscaler::recreate` is an unconditional `destroy` + `Self::new`
  (`frame_upscaler.rs:1005-1024`), so every resize and every preset switch replaces the
  output `VkImage`/`VkImageView` handles. `PresentationPipeline` writes those views into its
  descriptor sets exactly once, in `create`/`write_inputs`. The only thing preventing the
  presentation descriptor from naming a destroyed view is the source ordering in
  `recreate_swapchain_core`: `presentation.take()` → `destroy` → `upscaler.recreate()` →
  `PresentationPipeline::new(..., &upscaled_views, ...)`. That ordering is load-bearing, is
  the highest-value source-provable invariant in this dimension, and has no test.
  `resize.rs`'s test module already uses exactly this static-source-landmark technique for
  three sibling invariants (#654, #2141, #2142, #2156), so the mechanism is established.
- **Evidence**:
  - `resize.rs:1005-1012` — the comment states the invariant ("Presentation descriptors
    reference the upscaler's output views, so retire presentation before replacing those
    views") but nothing asserts it.
  - `frame_upscaler.rs:1013-1024` — `unsafe { self.destroy(device, allocator) }; *self = Self::new(...)?;`
    → new image/view handles on every call, including a same-output-extent preset switch
    (Quality→Performance changes only `render`, but the outputs are recreated anyway).
  - `presentation.rs::write_inputs` is called only from `create`; there is no
    `rebind_upscaled_views` equivalent to `composite.rs::rebind_hdr_views`.
  - The existing four static-order tests in `resize.rs`'s `mod tests` cover the swapchain
    view handoff, the SSAO failure rebind, the water set-2 rebind and the upscaler-switch
    rollback — but none cover this pair.
- **Impact**: A reordering (e.g. hoisting `upscaler.recreate` above the `presentation.take()`
  so the `allocator` borrow reads more naturally) would leave every presentation descriptor
  sampling a destroyed image view on the first post-resize frame. Invisible to `cargo test`,
  and on the default render path.
- **Needs RenderDoc**: no
- **Suggested Fix**: Add a static-source test to `resize.rs::mod tests` in the style of
  `ssao_recreate_failure_rebinds_binding_7_to_the_placeholder`: assert
  `find("unsafe { presentation.destroy(&self.device) }")` < `find("upscaler.recreate(")` <
  `find("PresentationPipeline::new(")` inside `production_src()`, with a message naming the
  descriptor-vs-view lifetime as the reason.

---

## Verified clean (no finding)

Recorded so the next sweep does not re-derive them.

**Preset switching (`--upscaler` / `r.upscaler`) — clean, and the strongest part of this dimension.**
`byroredux/src/app_step.rs::step_upscaler_switch` drains a `PendingUpscalerSwitch` resource
from `app_events.rs:716`, inside `about_to_wait` and **before** `render_one_frame`
(`app_events.rs:766`) — the swap cannot land mid-frame.
`resize.rs::set_upscaler_mode` early-returns on an unchanged mode, takes
`device_wait_idle` first, tears TAA down and hands composite back its raw HDR views before
they disappear, then routes the whole thing through `recreate_swapchain`. Every
render-extent-derived resource is rebuilt from `self.frame_extents` recomputed at
`resize.rs:160` by `FrameExtentSet::for_output`, and `FsrTemporalState` is rebuilt at
`resize.rs:165-173` so the jitter phase count tracks the new scale ratio (18 phases at
Quality, 32 at Performance — pinned by
`fsr_jitter_phase_count_tracks_every_scale_ratio`). The `#2156` rollback arm restores the
previous mode, rebuilds, and rebuilds TAA if `previous == Taa`; `FrameUpscaler::destroy` is
idempotent, so the rollback's second `recreate` over a zombie upscaler is safe. A failure of
both is fatal and exits the event loop, which is correct given the #1211 guard would
otherwise freeze the window.

**Presentation framebuffers vs FSR outputs on resize — clean.** `PresentationPipeline` is
destroyed and fully reconstructed inside `recreate_swapchain_core`; framebuffers come from
`self.swapchain_state.image_views` and the descriptor from the freshly-returned
`upscaler.output_views()`. `image_health_buffers` are deliberately *not* swapchain-sized and
are re-handed to the rebuilt pipeline (`resize.rs:1036-1038`). Framebuffer indexing uses
`image_index` (swapchain image count) and descriptor indexing uses `frame`
(`MAX_FRAMES_IN_FLIGHT`) — correctly separated in `dispatch`. See D23-06 for the missing
guard on the ordering.

**UI overlay cannot read a stale/half-written upscale target.** `record_overlay` samples
only the bindless texture array (`ui.frag`, `set = 0, binding = 0`); it never reads
`upscaledScene`. It draws in the same subpass *after* `cmd_draw(cmd, 3, 1, 0, 0)`, so
rasterization order supplies the blend ordering, and it re-binds both its descriptor sets
because the presentation layout's set 0 is layout-incompatible with the scene layout — that
disturbance is handled, not inherited. Viewport/scissor are re-set to the **output** extent
(the old geometry-pass draw used `frame_extents.render`, which was the bug #3426 fixed).
`ui.frag` writes only `location = 0`, matching the single-attachment pass; the blend is
`SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA` against an sRGB swapchain, which is the exact inverse of
the `R8G8B8A8_SRGB` sampler read of Ruffle's capture. `firstInstance = instance_index` on a
direct `vkCmdDrawIndexed` needs no `drawIndirectFirstInstance` feature.

**Jitter — one source, not two.** `draw.rs:2039-2066` is a single `match` on
`renderer_config.upscaler`: the `Taa` arm calls `taa_jitter`, the `Fsr3` arm reads
`FsrTemporalState::current().ndc`. They are mutually exclusive and the Y-sign convention is
pinned in both directions by `taa_and_fsr_negate_jitter_y_the_same_way` and
`fsr_pixel_jitter_flips_vulkan_projection_y`. The sequence advances only via
`mark_dispatch_completed`, gated on `take_submitted_dispatch()` after `queue_submit`
(`draw.rs:3900-3908`), so a recorded-but-unsubmitted frame cannot desynchronise projection
from reconstruction. `fsr_motion_vector_scale` returns both-negative render dimensions with
no display-resolution or jitter-cancellation flags, matching the engine's
`current_uv - previous_uv` motion texture; pinned by
`motion_adapter_converts_current_uv_minus_previous_to_fsr_pixels`. (D23-02 is the one
divergence, and it is debug-mode-only.)

**Exposure — `NO_EXPOSURE_RESOURCE_FALLBACK` is genuinely 1.0 and used on both sides.**
`exposure.rs:39` declares it `= 1.0` with the SDK derivation
(`PrepareRgb`'s zero-texel rewrite) in the doc; `post_passes.rs:1063-1066` is the sole
consumer, `self.exposure.as_ref().map_or(NO_EXPOSURE_RESOURCE_FALLBACK, |v| v.value())`;
`frame_upscaler.rs` passes `exposure: inputs.exposure.map(...)` so an absent resource
becomes a null FFX resource. Both directions pinned by
`absent_resource_fallback_matches_the_sdk_substitution` and
`presentation_falls_back_to_the_shared_constant` (which greps `post_passes.rs` for a
regression back to `DEFAULT_EXPOSURE = 0.85`). #2833 is closed and guarded.

**Dispatch-failure fallback renders, is telemetry-reported, and `BYRO_FSR_FORCE_DISPATCH_FAIL=1` still reaches it.**
`frame_upscaler.rs::record` has three fallback entries, all of which record a real
`cmd_blit_image` of the render-resolution scene into the output image — never a blank frame,
never an early `return` before the blit:
(1) `force_native_blit || !is_fsr_dispatch_active()` → blit from `SHADER_READ_ONLY_OPTIMAL`;
(2) `fsr_frame == None` → latch + blit from `SHADER_READ_ONLY_OPTIMAL` (#2146);
(3) SDK `Err` → `record_fsr_depth_restore` + blit from `GENERAL` (#2140/#2519), pinned by
`recovery_blit_acquires_the_output_from_the_layout_the_dispatch_left`. Every
`dispatch_failure` assignment is paired with `new_dispatch_failure = true` and enforced by
`every_dispatch_failure_latch_raises_the_temporal_discontinuity_edge`, which also asserts
`post_passes.rs` consumes it. `record` is deliberately infallible so it cannot bypass #917's
no-advance-on-unsubmitted-dispatch invariant. The fault injector is live:
`crates/fsr3-sys/src/lib.rs:168-204` reads `BYRO_FSR_FORCE_DISPATCH_FAIL` through
`env_flag_is_set(var_os(...))` into a `OnceLock` and returns a synthetic error *before*
touching the SDK; documented in `docs/engine/fsr3-troubleshooting.md:34-36`. (#2825's
"`=0` and empty both mean on" premise is **stale** — the predicate is now
`env_flag_is_set`, not `is_some`.)

**Resource-state contract (observe-only, per this dimension's rules).** Read and not
edited: `record_fsr_barriers_before` (four execution-only `SHADER_READ_ONLY_OPTIMAL`
no-transition barriers on colour inputs via `fsr_input_read_barrier`,
depth `DEPTH_STENCIL_READ_ONLY_OPTIMAL → SHADER_READ_ONLY_OPTIMAL`, output
`SHADER_READ_ONLY_OPTIMAL → GENERAL`), `record_fsr_barriers_after` (both restored), and
`record_native_blit`. The output image lands in `SHADER_READ_ONLY_OPTIMAL` on **all three**
paths, which is exactly the layout `PresentationPipeline::write_inputs` declares in its
`VkDescriptorImageInfo`; first-frame steady state is established by `initialize_outputs`
(`UNDEFINED → SHADER_READ_ONLY_OPTIMAL`). The `CHAIN-D2-02 / #2139` block records the
900-frame three-preset `BYRO_VALIDATION=1` clean run against FSR 3.1.4 and correctly labels
it evidence-not-proof, with a re-run trigger on SDK bump. No barrier finding filed. (See
D23-05 for the one place the *measurement's* date no longer covers the code.)

**FFI safety (`crates/fsr3-sys/src/lib.rs`).** Only two `unsafe fn` exist —
`Context::create` (line 379) and `Context::dispatch` (line 408) — and both carry a
`# Safety` section. `impl Drop for Context` is safe-by-signature and its `unsafe` block is
commented. The #2829 delta is correct and an improvement: `byro_fsr3_context_destroy` now
frees the wrapper and nulls `*context` on **every** path, which matters precisely because
`FrameUpscaler::recreate` destroys and rebuilds on every resize and preset switch, so a
persistently-failing destroy used to compound once per switch. Overlaps /audit-safety
Dimension 1; nothing new filed.

**Bench harness — changed, but behaviour-preserving for the measured columns.** Contrary to
a naive reading of the delta, both scripts *were* edited (#3347 sanity gates + #3467
`gridcross` in `fsr-bench-matrix.sh`; #2821 `gpu_inactive` handling in
`fsr_bench_report.py`) and no re-bench artifact landed. I checked whether that invalidates
the live bench-of-record (`docs/audits/BENCH_stepped-camera_34074b93.tsv`,
`# harness=4de5e78e engine=34074b93`) and concluded it does not:
- The TSV column order is unchanged for fields 1–23; `gpu_inactive` is appended **last**,
  and the two `cut`-based gates use `-f19` (entities) and `-f23` (state_hash), which match
  the header positions.
- `fsr_bench_report.py` reads by header name, so a 23-column archived TSV parses
  identically; `inactive_columns()` returns an empty set when the column is absent, and a
  bracket excluded from `render_sum` contributes the same 0 a measured zero did — the
  arithmetic of `render`, `render recovered` and `net recovered` is unchanged. Only the
  `*` / `n/a` display markers are new.
- The five default scenes' invocations are byte-identical; `gridcross` is deliberately
  excluded from `SCENES` until its floor is calibrated (the script says so, and its
  placeholder floor of `0` is explicitly labelled uncalibrated).
- `set -uo pipefail` with `REJECTED=()` and `${#REJECTED[@]}` is safe on bash ≥ 4.4 (5.3.9
  here).
Cross-commit comparisons therefore still mean something. The one thing worth a line in the
next ROADMAP refresh is that the harness commit has moved off `4de5e78e`, so the
"byte-identical harness" phrasing at `ROADMAP.md:325` describes the R6a-stale-17 control
pair only and should not be carried forward unqualified.

**Carried scope, not findings (per instruction).** The FP32 SDK permutation remains
**unexercised** — it needs a device without `shaderFloat16` and the dev box has one;
`frame_upscaler.rs:399` only *reports* which permutation is live
(`let permutation = if self.shader_float16 { "fp16" } else { "fp32" }`), the SDK reads the
physical device directly and offers no override. `native-aa`'s −9%…+4% cost is expected and
not reported. #3247 (bloom-relocation barriers around `scene_color`) is OPEN and not
re-filed. The #2520 `UpscalerMode::Taa` degenerate same-extent blit is documented,
non-default, and deliberately left.

---

**Severity count: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 6 LOW**

---

## Prioritized Fix Order

**Correctness first.**

1. **`D19-01`** — census the Oblivion `APPLY_HILIGHT2` normal-map DDS formats,
   then gate `PARALLAX_ALPHA_HEIGHT_BIT` on `normal_has_alpha` (already in
   scope at the bit-set site). Everything else in the `#3530` cluster depends
   on this decision.
2. **`D6-01`** — arbitrate the normal-map alpha channel between POM height and
   the specular mask. The precedence is a *content* question that needs the
   Oblivion authoring convention sourced, not a code question; do not pick a
   side by inspection. Land a `#[test]` pinning that both bits are never set
   for one draw.
3. **`D18-01`** — stop `apply_neutral_exterior_fallback` clobbering an
   authored directional. While fixing, check whether the same
   `CellLightingRes`-without-`SkyParamsRes` desync reaches real exterior cells
   loaded with no `WeatherDataRes`, or only the `--cornell-sun` harness; the
   report asserts only the latter.
4. **`D9-01`** — requeue (or hard-fail) a dropped first-sight `bind_inverses`
   upload instead of swallowing the `Err`.
5. **`D17-01`, `D17-02`** — the translucency SSS term and the soft-shadow
   emitter-disk radius source.
6. **`D13-01`** — decide whether TAA resolving only pre-composite direct HDR is
   the intent now that FSR Quality is the default reconstruction path.

**Then the missing guards** (`D3-01` `DBG_*` exhaustion, `D3-02` hardcoded
mirror `SOURCES` lists, `D7-01` `hash_gpu_material_fields` completeness,
`D23-06` resize ordering, `D20-03` depth-capture ordering). None is a live
defect; each is the guard that would have caught one.

**Then the ledger** (`D5-01`, `D5-02`, `D5-03`) — `memory-budget.md` is
designated authoritative and is currently wrong by 2× on `MAX_LIGHTS`.

**Then the `#3426` documentation sweep.** The twelve surviving `#3426` LOW
findings (`D3-03`, `D4-01`, `D4-04`, `D4-05`, `D8-04`, `D8-05`, `D11-01`,
`D23-01`, `D23-03`, `D23-04`, `D23-05`, `D23-06`) collapse into roughly six
edits: the two FSR documents, `shader-pipeline.md`'s submission table,
`renderer.md`, the `pipeline_ui` "see also" pointers, and the four `# Safety`
contracts still naming composite as the swapchain writer. Doing these as one
pass is far cheaper than as twelve issues.

## Needs-RenderDoc

No Vulkan device was available. Two observations stop at the observation and
propose no edit:

* **`D4-05`** — `presentation.rs`'s "`#2465` — MEASURED, deliberately
  unchanged" justification predates `#3426`, which added three new access
  types to that pass. The measurement that justified the current dependency no
  longer covers what the pass does. Re-measure under `BYRO_VALIDATION=1`
  before touching anything.
* **`D23-05`** — the presentation pass's incoming `SUBPASS_EXTERNAL`
  dependency description no longer enumerates all of the pass's accesses after
  the overlay draw joined it. Whether the *declared* dependency is also
  incomplete (as opposed to just its comment) is not decidable from source.

Also carried, not findings: the **FSR FP32 permutation is untested** (it needs
a GPU without `shaderFloat16` and the dev box has one), and `native-aa`'s
expected net performance loss is by design.

## Dimension Coverage

All 23 dimensions were walked. Dimensions producing **no findings**, stated
explicitly rather than omitted:

| Dimension | Result |
|---|---|
| 12 — Command buffer recording | **No findings.** Render-pass balance, the post-`#3426` frame tail, the `#3308` depth-capture placement outside the render pass, the three-spellings-deep `needs_two_sided_blend_split` predicate and all four of its pins, and `#3401`'s zero-index guard all verified clean. |
| 14 — Caustic splat | **No findings.** One `CAUSTIC_FIXED_SCALE` symbol across both writers and the composite divide, integer-atomic accumulation, macro-based source selection, and the glass/water double-count gate all correct. |
| 15 — Water | **No findings.** Authored `fresnel_f0` (not the glass IOR), per-frame `sun_direction` upload, the `water_caustic.rs` / `water.rs` lifecycle split, and a real `WATERLINE_HYSTERESIS` band all correct. Visual behaviour unverified — no device or game data. |
| 21 — Cornell-box RT harness | **No findings.** No Cornell-only material shortcut; `glass()` goes through `apply_surface_behavior(GLASS_SURFACE_BEHAVIOR)` and the dragon override round-trips through the real `translate_material`. One candidate finding (NaN sentinels leaking from `Material::default()`) was raised and **disproved**. |
| 22 — Light animation | **No findings.** The `canonical_light_animation_flags` / `canonical_light_shadow_flags` mirrored pair is correct, its opposite defaults are documented as deliberate (`#2517`), and the consumer reads `animation_flags`, never raw `LightSource.flags`. A candidate "third un-canonicalized per-game seam" (the `LIGHT_FLAG_SPOT` shape classifier) was raised and **disproved** — it is a correctly-factored third boundary. |

Dimensions **1** (AS correctness) and **4** (synchronisation) found **zero
correctness defects**; their findings are observability and stale prose only.

## Stale Premises Dropped

Per the project's audit-hygiene rule, premises that no longer hold were
dropped rather than reported. Recorded here so the next sweep does not
re-derive them:

* A sixth `struct GpuInstance` mirror site — the extra `grep` hit in
  `skin_vertices.comp` is a comment, not a declaration. Five mirrors is correct.
* An `INSTANCE_FLAG_*` bit-uniqueness gap — already pinned by
  `instance_flag_bits_match_scene_buffer_consts` (`#1190`).
* NaN metalness/roughness sentinels reaching `GpuMaterial` from the Cornell
  probes — `impl Default for Material` ships already-resolved values.
* `#3469`'s cached skinned device address being able to go stale — the buffer
  has no in-place realloc path; the cache is rebuilt with the slot.
* `fresnel_power` as a `#3448` sibling — `#2589` already fixed it.
* The `#1497` `static_frames` progressive-alpha floor — deleted by `e5d02f83`,
  cannot recur.

## Note for the audit-suite owner

Dimension 1 filed `D1-02`: the `audit-renderer` SKILL's own Dimension 1
checklist carries two stale claims, and that staleness already produced a
false "re-verified as unchanged" line in the 2026-08-27 report. The SKILL file
is itself audit input and needs the same freshness discipline as the code.
Separately, `_audit-severity.md` **does** exist at
`.claude/commands/_audit-severity.md` and was used for every severity call in
this report — one dimension agent's filesystem search missed it and reported
otherwise; that report is incorrect.
