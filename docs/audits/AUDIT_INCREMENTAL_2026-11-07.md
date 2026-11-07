**HEAD**: 310444b69 · **Baseline**: AUDIT_INCREMENTAL_2026-10-20.md · **Audited**: renderer (volumetrics inject/composite shaders, GPU-struct lockstep, descriptor/pipeline-layout plumbing, SPIR-V parity, debug-view contract), per-game (FNV/FO3/Oblivion fxlight placement), tech-debt (test + doc hunks), runtime (golden/smoke additions, read only) · **Unchanged since baseline (skimmed)**: exterior Dims 1-4 (only `groundcover_blade.frag.spv`/`water.frag.spv` rebuilt from the shared include change, byte-identical to a fresh compile), nifal (`material_tests.rs` hunk only adds a layout pin; no `Material` path changed), ecs/save (no `Serialize`, no query, no lock change)

# Incremental Audit — 2026-11-07 (`--commits 27`, HEAD~27..HEAD)

Read-only. No engine launched; no issues created.

**Scope note.** The working tree was clean (`git diff HEAD --name-only` empty), so the default scope was empty. Fell back to the 27 commits dated 2026-11-07 (682b8bb30..310444b69), the whole #6726 / #6727 volumetric-lighting session. 14 of the 27 are docs-only.

## 1. Change summary

`git diff HEAD~27..HEAD`: 45 files, +1081 / -60. Code-bearing themes:

| Theme | Commits | Code |
|---|---|---|
| Alpha-aware froxel sun trace (VLM-SH-009) | 684bc9161, fdcd02c0a, e2b8e0e16 | `volumetrics_inject.comp` (`sunHitIsSolid`, `sunRayBlockedAlphaAware`), new `include/alpha_compare.glsl` (the comparison table moved out of `ray_hit.glsl`), new binding 32 (material table) plus the TextureRegistry's bindless set as pipeline-layout set 1, `GPU_MATERIAL_*_WORD` constants |
| Interior shaft dust (VLM-INT-007) | fbde91ab5, 3acec770b | `viaAuthoredAperture`, `INTERIOR_SHAFT_DUST_*`, sun-term-only |
| Open-sky sun dust experiment `BYRO_SUN_DUST` (off by default) | d79c92813, 405b02715, f603ff6a5 | `openSkySunDust`, `SUN_DUST_*`, host lane `wind_gust.w`; `wind_gust.z` carries the `BYRO_SUN_ALPHA` switch |
| `volumetric_radiance` debug view (id 19) | ce0193143 | `RenderDebugMode::VolumetricRadiance`, composite early return, `RENDER_DEBUG_MODE_MAX` 18 to 19 |
| FNV beam converters run live (VLM-INT-006) | 3bb692bbb | `is_fxlight_sprite_placement` in `synth_child.rs` |
| Labs, golden, smoke | f413cd15e, e7a8c2f45, ... | `cornell/{shaft_lab,godray_lab}.rs`, `golden_frames.rs` + `shaft_lab_60f.{png,capture}`, `exterior-shafts.sh` |

## 2. Routing map

Routed by `_audit-route.sh` (risk floor in brackets). Only dimensions whose Paths intersect the diff were run.

| Files | Owner / dimension audited | Result |
|---|---|---|
| `volumetrics_inject.comp`, `composite.frag`, `include/*.glsl`, `shader_constants*.rs`, `.spv` [HIGH] | renderer (shader/SPIR-V parity, GPU-struct lockstep, descriptors/pipeline layout), performance (hot path) | clean; see Section 5 |
| `vulkan/volumetrics.rs`, `volumetrics/init.rs`, `context/{init,init_stages,post_passes,resize}.rs`, `scene_buffer/upload.rs` [HIGH] | renderer, safety, concurrency | clean; no new `unsafe`, no lock or query change, destroy order untouched |
| `render_debug.rs`, `reflect.rs` [HIGH] | renderer (debug-view contract) | clean |
| `material_tests.rs`, `shader_contract_tests.rs` [HIGH] | renderer, nifal | pins added, no `Material` path changed |
| `cell_loader/references/synth_child*.rs`, `nif_import_registry_tests.rs` [MEDIUM] | per-game, performance | **INC-1** |
| `scene.rs` [MEDIUM] | per-game, ui Dim 6 | arg plumbing only, clean |
| `cornell/*.rs`, `golden_frames.rs`, `exterior-shafts.sh`, `workspace_hygiene_tests.rs` [LOW] | renderer Dim 12, regression, runtime, tech-debt | **INC-2, INC-3** |
| `crates/plugin/examples/baked_beam_census.rs` [HIGH] | esm | example output line only, clean; ceiling bump 8353 to 8356 matches the +3 lines |

No `(unrouted)` paths.

## 3. Findings

### INC-1: FXLightBeam NIF import is enabled for every game; only FNV was measured
- **Severity**: MEDIUM (per-game floor)
- **Dimension**: per-game (Oblivion, FO3), import pipeline
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:284-289` (caller at `:570`)
- **Changed in**: `synth_child.rs` (commit 3bb692bbb)
- **Status**: NEW (adjacent to open #6727, which tracks FNV only)
- **Description**: `is_fxlight_sprite_placement` now returns `false` for any model whose path contains `fxlightbeam` and has no LIGH data, so the placement loads its NIF instead of becoming a bare synthetic child. The predicate takes no game input. The commit's justification is FNV-specific (14 `FXLightBeamSuperWide01` in Vault11a; converter coverage measured on FNV meshes only), and it claims a beam the converters decline "is still dropped from the raster by the `IsFxMesh` texture rule".
- **Evidence**: A name scan of the shipped archives finds `fxlightbeam01.nif`, `fxlightbeam02.nif` and more in `Oblivion - Meshes.bsa`, and `fxlightbeam01..05.nif` in `Fallout - Meshes.bsa` (FO3) as well as FNV (Skyrim SE has none). `IsFxMesh` is set by `texture_path_is_fx_mesh` (`byroredux/src/components/render.rs:270`), a texture-path substring test (`effects\fx*`, `fxlightrays`, ...). I did not check what texture paths the Oblivion or FO3 beam NIFs use, and did not render either.
- **Impact**: If an Oblivion or FO3 beam NIF's texture path misses that substring list, the card now draws as real alpha-blended geometry (the beam converters, measured on FNV only, may also decline it). That is a visible content change in interiors that previously showed nothing there. If the path does match, the cost is one extra entity and one NIF import per placement.
- **Related**: #6727, VLM-INT-006
- **Suggested Fix**: Either gate the exemption on the game variant (FNV only until measured), or run the `fnv_baked_beam_converter_coverage`-style census on Oblivion and FO3 and add a per-game assertion to `baked_beam_cards_load_their_nif_while_fxlight_sprites_stay_synthetic`.

### INC-2: New dust test depends on the process environment, the very variable the experiment uses
- **Severity**: LOW
- **Dimension**: tech-debt (test hygiene)
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs`, `open_sky_sun_dust_rides_the_sun_term_only_and_fades_with_a_high_sun` (last assertion)
- **Changed in**: d79c92813
- **Status**: NEW
- **Description**: The test ends with `assert_eq!(open_sky_sun_dust_per_meter(), 0.0, "off unless BYRO_SUN_DUST is set")`. The function reads the live environment through a `OnceLock`.
- **Impact**: Anyone with `BYRO_SUN_DUST=<n>` exported (the documented recipe for the experiment) gets a failing `cargo test -p byroredux-renderer`, with a misleading message.
- **Suggested Fix**: Split the parse into a pure `fn(Option<&str>) -> f32` and test that; leave the `OnceLock` wrapper untested, or drop the assertion.

### INC-3: Doc rot from this window's edits (two places)
- **Severity**: LOW
- **Dimension**: tech-debt (doc rot)
- **Location**: (a) `crates/renderer/src/vulkan/volumetrics.rs`, `VolumetricsParams::wind_gust` doc; (b) same file, the doc block above `open_sky_sun_dust_rides_the_sun_term_only_and_fades_with_a_high_sun`
- **Changed in**: fdcd02c0a / 684bc9161 (a), d79c92813 (b)
- **Status**: NEW
- **Description**: (a) The `wind_gust` doc was updated for lane `z` but still says "The remaining lane is reserved", while lane `w` has carried `open_sky_sun_dust_per_meter()` since d79c92813. The ABI-pin comment near `reflect.rs:771` also names only `wind_gust.y`. (b) The `#4793 / #4785 ... every sun-visibility ray ...` doc paragraph that belonged to `sun_visibility_rays_are_gated_on_a_live_sun_term` now sits directly above the new `BYRO_SUN_DUST` test's own doc, so it documents the wrong test and the gate test lost its doc.
- **Impact**: A later author reads `wind_gust.w` as free and reuses it, or reads the wrong test as the #4793 gate.
- **Suggested Fix**: Document `w` in the `wind_gust` doc and the reflect pin comment; move the #4793 paragraph back above `sun_visibility_rays_are_gated_on_a_live_sun_term`.

## 4. Missing tests

- **Alpha-aware sun trace has no parity pin against `rayHitHasCoverage`.** `sunHitIsSolid` re-spells the coverage chain (texel alpha, forced 1.0 without diffuse alpha, `* materialAlpha * vertexAlpha`, `alphaComparePass`, blend and glass rule). The shared compare table is now one file (good), but the surrounding chain is a second copy that is not text-pinned. The one test that exercises it is `exterior-shafts.sh`, which needs a Vulkan device. The omissions (decal slots, parallax UVs) are documented in a comment but not asserted anywhere.
- **`shaft_lab_golden_frame` predates later shader changes.** The golden was captured at f413cd15e; 684bc9161 (alpha trace) and d79c92813..f603ff6a5 (dust) changed `volumetrics_inject.comp` afterwards. The test is `#[ignore]` (Vulkan) and I did not run it. The lab's slats are opaque, so it may well still pass, but nothing in this window shows it was re-run. Re-run `cargo test -p byroredux --release --test golden_frames -- --ignored shaft_lab_golden_frame`.
- **`is_fxlight_sprite_placement`** has a pure unit test but none per game (see INC-1).
- **`triangle_early.frag.spv`** has no `.frag` source of its own; I could not confirm it was rebuilt from `triangle.frag`'s early-Z variant (its size changed by 16 bytes in step with `triangle.frag.spv`, which is consistent but not proof).
- Existing, not new: **#6730** (`interior-godrays.sh` A10 fails: `--godray-lab` shows no sun shaft). Commit 3acec770b reports the marked-aperture path works, so that issue's status should be re-checked against it.

## 5. Checks that came back clean

- **SPIR-V freshness**: `volumetrics_inject.comp`, `composite.frag`, `triangle.frag`, `water.frag` and `groundcover_blade.frag` were recompiled with plain `glslangValidator -V` and are byte-identical to the committed `.spv`.
- **GPU material word constants**: `GPU_MATERIAL_STRIDE_WORDS = 108` and the six `*_WORD` offsets are pinned to `size_of`/`offset_of` by `gpu_material_word_constants_match_the_host_struct`, and that test passes.
- **Descriptor plumbing**: binding 32 is in `volumetrics_inject_layout_bindings`, and the pool is derived from the same bindings (`DescriptorPoolBuilder::from_layout_bindings`), so no pool shortfall. The bindless layout's stage flags include `COMPUTE` (`texture_registry/mod.rs:499`). Both pipeline-layout call sites (`init.rs`, `resize.rs`) pass the layout, `dispatch` binds `[set0, bindless]`, and `write_boundary_geometry` writes binding 32 for every frame in flight. Writes happen before the `boundary_geometry_written` latch.
- **Debug-view contract**: `RENDER_DEBUG_MODE_MAX` 19, `USER_MODES` length 20, the dense-value test, the composite raw-debug guard and the `OpBranchConditional` count 160 to 161 all agree. The tests pass.
- **Targeted tests**: 159 renderer `--lib` tests (name filters: `gpu_material_word`, `open_sky_sun_dust`, `interior_shaft_dust`, `composite_frag_spv`, `volumetric_debug_view`, `sun_visibility_rays`, `render_debug`, `user_mode`, `shader_contract`) passed. I did not run the `byroredux` bin tests or the Vulkan-gated tests.
- **Hot-path cost**: the alpha-aware walk runs only for froxels the binary trace already called blocked, under open sky, and is bounded at `MAX_ALPHA_SKIP_LAYERS` (8) queries; it has a same-binary off switch. Its measured share is recorded in the VLM-SH-008 doc commits. Not re-measured here.
- **Design note, not a finding**: `sunTermLive` and the `scattering_coef > 0` outer gate do not count the two dust terms, so a froxel with zero base scattering gets no shaft dust. That matches the dust's stated "extra on top of a medium" intent, and the exterior haze floor keeps open-sky froxels non-zero.

## 6. Next step

`/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-11-07.md`
