**HEAD**: `00f580e09` · **Baseline**: `AUDIT_RENDERER_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dims 1, 2, 3 (constants/header layer), 4, 5, 6, 10, 11 — delta-scoped to `a2c24b16e..00f580e09` (116 commits, 24 touching renderer-adjacent paths) plus an unscoped real-data census of the DDS decode boundary (Dim 5) · **Unchanged since baseline (skimmed)**: Dim 3 struct layer (`gpu_types.rs`, `material.rs`, `bindings.glsl` — no commits); Dim 7 (`svgf`/`composite`/`taa`/`bloom`/`ssao` and shaders — no commits); Dim 8 (only the pure-move `volumetrics.rs` split, `a4ef215d4`); Dim 9 (only a comment, `a9d6508fb`); Dim 12 (`light.dump` lines and clippy one-liners)

# Renderer Audit — 2026-10-08 (all 12 dimensions, delta + DDS census)

This run is part of `/audit-suite --preset comprehensive`. It audits the 116 commits since the 10-05 baseline. The renderer-relevant delta is the fix wave for the baseline's own findings, plus one large shader change and one shader-regression repair.

**Delta contents**
- #5249 (`34c3adf14`): the transmission lobes take their own own-instance-skipping shadow trace.
- #5368 (`ebf1d0492`): the undefined `MAX_TRANSMISSION_SELF_SKIPS` repair and the rebuilt `triangle` SPIR-V pair.
- #5250 / #5251 / #5254 / #5252 / #5265: the baseline's remaining findings and the FIF-rider repoint.
- #5270 (`d194f7d60`): the one-time-submit helper's wait-failure arm.
- #2764 (`9a179472f`): the indirect group key now carries the split-eligibility predicate.
- #5094 / #5096 (`a4ef215d4`): the `volumetrics.rs` split and the `record_geometry_pass` size pin.
- #5097 (`3516618b9`): the `SHADER_DEFINES` table replaces the 1,415-line `build.rs` body.
- #5100 (`655b317c9`): one shared source-scan cut in `byroredux-core`.

**Method**
- Every dimension was analysed synchronously in this session; no sub-agents were used. Per-dimension notes are in `/tmp/audit/renderer/dim_1.md` … `dim_12.md`.
- No engine, GPU process or `BYRO_VALIDATION=1` run was launched. Every Vulkan-facing conclusion comes from source and guard-test reading.
- The DDS census used throwaway readers built *outside* the tree (`/tmp/audit/renderer/{ddsscan,ddsstat,nifref,glowprobe}`, path-depending on `crates/bsa`, `crates/nif` and `crates/core`). They only read vanilla archives under the game install directories. The working tree was left untouched; `git status` still shows only the pre-existing `audit-sync/BASELINE` change.
- Guards run:
  - renderer lib: 1383 / 0 / 1 (baseline 1379 / 0 / 1);
  - bin crate, rustc 1.96.0: 2705 / 0 / 55 (baseline 2648 / 0 / 52);
  - core `--features inspect`, targeted: `resolve_pbr_is_idempotent` and `glass_behavior_preserves_authored_map_overlay` ok;
  - fsr3-sys: 8 / 0;
  - NIFAL corpus `cross_game_translation_completeness` (`--ignored`, about 3 s): 1 / 0;
  - stale-SPIR-V gate `scripts/check-shader-artifacts.sh`: clean (36 shaders plus `triangle_early.frag.spv`, glslang 11:16.4.0; the script now accepts 16.2.0 and 16.4.0).

## Executive Summary

| Severity | NEW | Findings |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 1 | D5-01 (32-bpp `DDPF_RGB` DDS uploaded as R8G8B8A8 whatever the channel masks say: every Skyrim SE distant-terrain diffuse atlas has red and blue swapped) |
| MEDIUM | 1 | D5-02 (`DDPF_LUMINANCE` / L8 DDS rejected: the Oblivion glow-map role loses its texture) |
| LOW | 2 | D10-01 (the #5249 transmission trace ignores the glass visibility layer, contradicting its comment), D5-03 (stale `SAFETY` comment after the `as_chunks` migration) |

Already tracked and still live: #5211 (SVGF nearest-tap NaN re-accept — re-verified in the shader), #5210 (NIFAL doc/spec rot), #5365 (all-slots fence-wait throughput), #5158 and #5159 (lighting/noise).

**Baseline closure.** All five baseline findings are fixed and closed, and each fix was read in its diff.

| Baseline finding | Fix | State |
|---|---|---|
| REN-D10-2026-10-05-01 (transmission lobes unshadowed) | #5249 (`34c3adf14`) | closed; see Dim 2/10 below |
| REN-D1-2026-10-05-01 (TLAS scratch shrink target) | #5250 (`72028d4fd`) | closed; peak is `build.max(update)` |
| REN-D1-2026-10-05-02 (stale scratch-size comment) | #5251 (`a9d6508fb`) | closed |
| REN-D6-2026-10-05-01 (#4441 doc vs #5197) | #5252 (`46f59e1d2`) | closed |
| REN-D11-2026-10-05-01 (hand-typed S/K = 8.0) | #5254 (`bc1233207`) | closed; single-sourced |

**The fix wave landed cleanly, but one landing was broken for a day.**
- `34c3adf14` (#5249) used `MAX_TRANSMISSION_SELF_SKIPS`, which no file defined. `triangle.frag` and its early-test variant were uncompilable from source, while the committed `.spv` pair stayed frozen at a pre-fix intermediate. The stale early variant blackened early-eligible opaque surfaces in FNV interiors.
- `ebf1d0492` (#5368) defines the constant and rebuilds both binaries. `5c81a2f90` makes the parity gate accept both glslang versions, so the gate that would have caught the drift now runs on the developer host. HEAD is clean under that gate.
- This is a process note, not a finding. Source-shape tests are string checks and cannot see an undefined GLSL constant; `check-shader-artifacts.sh` is the only compile gate.

**The new findings come from a real-data census, not from the delta.** Both sit in `crates/renderer/src/vulkan/dds.rs`.
- The 32-bpp branch of `parse_dds` ignores the four channel masks. Across the vanilla archives the A8R8G8B8 / X8R8G8B8 layout is the common legacy form, and Skyrim SE ships all 9,326 of its distant-terrain diffuse atlases that way.
- The L8 luminance form is not handled at all.
- Prior audits recorded the DDS parser as "handles 32-bpp uncompressed RGBA/BGRA" (`AUDIT_FO3_2026-07-02.md`). The masks were never inspected; the parser's own test fixture (`make_uncompressed_header`) leaves them zero.

## RT Pipeline Assessment

**AS correctness (Dim 1).**
- The only code delta is #5250. `ensure_tlas_state` now records `build_scratch_size.max(update_scratch_size)` as the slot peak.
- The allocation at the grow site, the skinned shared scratch, its shrink peak and the refit assert already used `max(build, update)` since #5195. Every site the skill lists now agrees.
- The #2774 pin was re-pointed at the max form, so a revert to the build-only literal fails `scratch_tests.rs`.
- Static BLAS carry `update_scratch_size: 0`, which is correct: `STATIC_BLAS_FLAGS` has no `ALLOW_UPDATE`.
- #5270 (`d194f7d60`) was read in full.
  - On `ERROR_DEVICE_LOST` it keeps the historic disposal.
  - On any other `vkWaitForFences` failure it leaks the command buffer and the owned fence, and poisons the reusable fence's mutex through a caught panic raised while the guard is alive.
  - `panic = "unwind"` is pinned in `Cargo.toml`'s release profile, so `catch_unwind` is sound.
  - `blas_static.rs`'s `MaybeInFlight` arms keep their staging and BLAS allocations.
  - It cannot be fault-injected; the commit says so and pins the arm with `wait_failure_arm_disposes_only_on_device_loss`.

**Ray-query shading (Dim 2/10).**
- #5249's `traceShadowTransmittanceSkippingInstance`:
  - Own-instance hits are walked past with `advanceShadowRayPastHit`; the helper takes `inout`, and the loop uses the same advance-past transport as the alpha-skip walk.
  - The first foreign hit hands the remaining leg to `traceShadowTransmittance`.
  - An exhausted `MAX_TRANSMISSION_SELF_SKIPS` budget reports lit.
  - The finalize reuses shadow ray 0's origin, direction and `tMax`. They are set in the same `shadowFade > 0.01` block the new trace is gated on.
  - The trace sits inside `useRestir`, which is gated on `directShadowRayEnabled`, itself gated on `rtEnabled` (`sceneFlags.x > 0.5`).
  - `fragInstanceIndex` as the skipped index has precedent in the same shader (the glass `skipInstance` arguments and `terminusOnSelf`). That equivalence is the Dim 1 `instance_custom_index` contract.
- One divergence from the comment: the glass layer is never consulted for the transmission half (D10-01).
- The only `sin()` of absolute world position is the low-frequency translucency turbulence in `lighting.glsl`. It is benign, bounded by the `translucencyTurbulence` amplitude.
- The sun-disc luminance floor (`c60405083`) cannot produce NaN: a zero disc term multiplies out to exactly zero.

**Denoiser (Dim 7).** No code delta. #5211 is still live: `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb` in the sub-pixel nearest-tap fallback has no `isnan`/`isinf` test, while the bilinear loop does.

**Exposure / FSR (Dim 11).** #5254 single-sources `LIGHT_METER_CALIBRATION_K`, `SENSOR_SENSITIVITY_S` and `EXPOSURE_METER_S_OVER_K`. `exposure.rs` resolves its constants from the same data. The shader multiplies the define, and the source pin forbids a hand-typed `* 8.0`. `exposure_meter.comp.spv` is byte-identical because the define expands to the same literal. FSR core has no commits.

## GPU-Struct & Memory Assessment

**GPU structs.** No `#[repr(C)]` struct, shader mirror or `bindings.glsl` content changed. The size, offset, mirror, UBO and light-header pins all ran green:
- `gpu_instance_is_160_bytes_std430_compatible`, `gpu_camera_is_368_bytes`, `gpu_light_is_64_bytes`, `gpu_terrain_tile_is_176_bytes`;
- `gpu_material_size_is_432_bytes`, `gpu_material_field_offsets_match_shader_contract`;
- `every_shader_struct_is_classified` (the single `MirroredPendingGuard` entry is still `Reservoir`);
- `hash_gpu_material_fields_covers_every_gpu_material_field`, `no_file_states_a_stale_gpu_material_size`.

`build.rs` shrank from 1,455 lines to 66: the header is rendered from the `SHADER_DEFINES` table. The committed `shader_constants.glsl` equals the generated output (`git status` clean after a full build and test), and no shader hand-types an `INSTANCE_FLAG_*`/`MAT_FLAG_*` literal.

**Lifecycle (Dim 5).**
- No new GPU resource owner appeared.
- The `volumetrics.rs` split was verified as a pure move with a line-multiset diff. Only `pub(crate)` visibility, imports and the `mod` lines differ.
- `ec0e0c8b4` (#5310) `detach_victims_from_surviving_parents` is sound:
  - it drops the `Parent` read guard before taking the `Children` write;
  - same-sweep parents are skipped;
  - three tests pin it.
- The DDS decode boundary is the new area of concern (D5-01, D5-02).

**Negative census (no finding)**, on the vanilla archives:
- FourCC values are DXT1/3/5 only across Oblivion, Shivering Isles, FO3, FNV and Skyrim LE.
- No payload is shorter than its header's mip chain (0 of 91,890 non-DX10 files across 10 archives: Oblivion and Shivering Isles textures, FO3, FNV x2, Skyrim LE, Skyrim SE `Textures0/3/5/7`).
- Every cubemap carries all six faces and is square (Oblivion, FO3, FNV, Skyrim LE and SE `Textures0`–`4`: 0 partial, 0 non-square).
- No header declares a zero or over-8192 dimension (same archives).

## Findings

### CRITICAL

None.

### HIGH

#### REN-D5-2026-10-08-01: `parse_dds` ignores the channel masks of 32-bpp `DDPF_RGB` headers, so A8R8G8B8 / X8R8G8B8 files upload as R8G8B8A8 with red and blue swapped — including every Skyrim SE distant-terrain diffuse atlas
- **Severity**: HIGH
- **Dimension**: Memory/Lifecycle (texture decode)
- **Location**:
  - `crates/renderer/src/vulkan/dds.rs`, `parse_dds`, the `bpp == 32` arm of the `DDPF_RGB` branch ("32-bpp R8G8B8A8 uploads directly — zero-copy").
  - Consumers: `TextureRegistry::load_dds_with_clamp_and_color_space` / `enqueue_dds_with_clamp_and_color_space` (`crates/renderer/src/texture_registry/upload.rs`).
  - The affected Skyrim consumer: `spawn_btr_block` (`byroredux/src/cell_loader/terrain_lod_btr.rs`), which resolves `btr_diffuse_path`.
- **Status**: NEW. No open or closed issue matches the keyword searches listed under Process notes (#1542, #1074 and #4830 are adjacent but cover other arms). `AUDIT_FO3_2026-07-02.md` Dim 6 records the parser as handling "32-bpp uncompressed RGBA/BGRA", which was never checked against the masks.
- **Description**:
  - The 16-/24-bpp arm reads `dwRBitMask`…`dwABitMask` (file offsets 92..108) and decodes through `RgbExpand`.
  - The 32-bpp arm never reads them. It sets `format: R8G8B8A8_SRGB, expand: None` and borrows the file bytes.
  - A DDS with `R=0x00FF0000, G=0x0000FF00, B=0x000000FF` stores bytes in B,G,R,(A|X) order. Uploaded as R8G8B8A8, the sampler's `.r` is the file's blue.
  - For X8R8G8B8 (`A=0`) the fourth byte is the unused X byte, so alpha is sampled from it. It averages 0 across the 2,916 Tamriel atlases measured. `format_has_alpha(R8G8B8A8_SRGB)` is true, so `INSTANCE_FLAG_DIFFUSE_ALPHA` is set and the shader does not force `a = 1`. The LOD terrain draw is opaque with no alpha test, so the zero alpha is harmless today; the channel swap is the visible part.
  - The sibling DX10 path handles the same layouts correctly: DXGI 87/88/91 map to `B8G8R8A8_*`.
- **Evidence**:
  - Code: the `bpp == 32` arm in `parse_dds` (above).
  - Test gap: the only 32-bpp fixture, `make_uncompressed_header`, never sets the masks.
  - No compensation downstream: no `.bgr`/`.zyx`/`ComponentMapping` swizzle exists in the renderer shaders or image-view creation (grep).
  - Measured on the vanilla archives (header scan of every `.dds`; masks other than the RGBA order `R=0xFF, G=0xFF00, B=0xFF0000`):

    | Archive set | Layout | Count | What it is |
    |---|---|---|---|
    | Skyrim SE `Textures5/6/7.bsa` | X8R8G8B8 (`A=0`) | 9,326 | every terrain-LOD diffuse atlas under `textures/terrain/<world>/` (Tamriel 3,040; Solstheim 3,060; Soul Cairn 910; Apocrypha 990; Sovngarde 495; Skuldafn 340; the rest smaller) |
    | Skyrim SE | A8R8G8B8 | 6 + 11 + 2 + 3 + 1 (+ 641 flow maps, see below) | 6 tree-LOD atlases (`…/trees/<world>treelod.dds`), 11 DLC1 cubemaps, `sky/sun.dds` and `sky/sunglare.dds`, 3 lens-flare maps, `effects/highfrequencynormals.dds`; the 641 are `textures/water/skyrim.esm/flow.*.dds` |
    | Oblivion / Shivering Isles | A8R8G8B8 | 72 / 3 | lock-picking meshes, gate effects, 1×1 placeholders |
    | FO3 / FNV | A8R8G8B8 | 13 / 13 | including the WATR noise/foam maps `testwaternoisegrant.dds`, `wastelandmuckpoolnoise01.dds`, `toxicdumpwater01.dds`, `waterfoam01.dds` |
    | Skyrim LE | A8R8G8B8 | 1 | `effects/highfrequencynormals.dds` |

  - The masks, not the bytes, are what is right. On 2,916 Tamriel diffuse atlases the per-file mean of byte 2 minus byte 0 has p50 = +14.1 and p90 = +14.2 (of 255), and 2,554 files have byte 2 > byte 0. Decoded per the masks (R = byte 2), that is the warm, earthy tint Skyrim terrain has. Decoded the way the engine does (R = byte 0), it is a uniform 5 % shift toward blue.
  - Cross-check: the 21 level-32 `tamriel.32.*` atlases are authored with RGBA-order masks (so they decode correctly today), and their R is also above their B (73 vs 64 on the sample).
  - The 641 water flow maps are **not** consumed by the engine today: only the NIF-water `flow_map_index` is loaded, from the mesh, not these per-cell files (grep of `byroredux/src`). They are listed for completeness, not as impact.
- **Impact**:
  - Every Skyrim SE exterior that draws distant terrain from vanilla `.btr` LOD renders it with red and blue swapped, plus an alpha channel read from an unused byte. The magnitude per tile is modest (a few percent), but it is systematic, silent and affects a headline feature.
  - The tree-LOD atlases (6) and DLC1 environment cubemaps (11) are swapped the same way.
  - The WATR noise maps (FO3/FNV) and the Oblivion lock-picking/effect meshes are swapped too, with lower visibility.
  - Rated HIGH under the decision tree's "rendering correctness → at least HIGH" line. The per-tile hue shift is small, so a reviewer who reads this as "visual artifacts only" can down-rate it to MEDIUM.
- **Related**: #1542 (16-/24-bpp arm), #1074 (DXGI BGRA mappings), #4830 (mask hardening), REN-D5-2026-10-08-02.
- **Suggested Fix**:
  - Read the four masks in the 32-bpp arm.
  - Keep the zero-copy path only for the exact RGBA order with `A = 0xFF000000`.
  - Send every other combination (A8R8G8B8, X8R8G8B8, X8B8G8R8) through the existing `RgbExpand` machinery, which already accepts arbitrary masks and forces `A = 255` when the alpha mask is zero. `validate_expand_masks` already admits `bpp == 32`.
  - Alternatively map the exact BGRA masks to `B8G8R8A8_*` and force opaque alpha when `a_mask == 0`.
  - Pin it with fixtures that carry real masks (an X8R8G8B8 case asserting the channels land in the right slots and `A == 255`).

### MEDIUM

#### REN-D5-2026-10-08-02: `parse_dds` rejects `DDPF_LUMINANCE` (L8) DDS, so the Oblivion glow-map texture role loses its map for Mehrunes Dagon and the Oblivion Gate set
- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle (texture decode)
- **Location**: `crates/renderer/src/vulkan/dds.rs`, `parse_dds`: the final `else { bail!("Unsupported DDS pixel format (flags={:#x})", pf_flags) }` after the `DDPF_FOURCC` and `DDPF_RGB` branches. The consumer chain is in `byroredux/src/asset_provider/texture.rs` (`resolve_texture_view_with_clamp`, then `resolve_material_texture_handles_with_clamp`).
- **Status**: NEW (no open or closed issue matches "DDS luminance", "Oblivion glow map" or "Unsupported DDS pixel format" beyond #1542, which fixed the 16-/24-bpp RGB arm only).
- **Description**:
  - `DDPF_LUMINANCE` (`0x20000`) is neither `DDPF_FOURCC` nor `DDPF_RGB`, so the header falls to the `bail!` arm.
  - The queue path then logs `Failed to enqueue DDS '…': Unsupported DDS pixel format (flags=0x20000)`, hands back the checkerboard fallback handle, and `map_secondary_texture_handles` collapses an authored-but-failed secondary role to handle 0. The glow role is silently absent.
  - The existing `RgbExpand` machinery could decode the format: an 8-bit pixel with the luminance mask copied to R, G and B yields `(L, L, L, 255)`, which is what D3D9 L8 samples as.
- **Evidence**:
  - Census (`flags & 0x20000`, `bpp = 8`, `RBitMask = 0xFF`): Oblivion 415 files in `Oblivion - Textures - Compressed.bsa` (every name printed by the scan ends `_g.dds`, the glow-map convention), Shivering Isles 55, Skyrim LE and SE 1 each (`effects/noisevolume.dds`). FO3, FNV and every other archive scanned: 0.
  - These textures land in the glow role. `glowprobe` (a throwaway program calling `byroredux_nif::import_nif_scene`) shows, for `meshes\creatures\endgame\battle.nif`, `…\entry.nif`, `…\mehrunesdagon\mehrunesdagon.nif` and `…\weapon.nif`, `base` and `emissive` both set, with `emissive` pointing at the `_g` texture.
  - Exposure by exact path match over `Oblivion - Meshes.bsa` (8,032 NIFs): 8 NIFs reference 34 distinct L8 textures — `creatures/mehrunesdagon/dagon_g.dds`, `obliviongate/akportal/akgate001_g.dds`…`akgate008_g.dds`, `creatures/rat/rateye_g.dds`, `oblivion/citadel interior/citadelcolumn02b_g.dds`, a scorched-stone landscape glow and others. The matching is exact, so this is a lower bound; the remaining `_g` files may be referenced by DLC meshes or by flipbook sequences not matched.
  - `triangle.frag` consumes the role as the emissive mask (`mat.glowMapIndex != 0u`), so with handle 0 the authored glow shape is gone.
- **Impact**: Oblivion's iconic glow-mapped content — the Oblivion Gate portal frames, Mehrunes Dagon, rat eyes, the Citadel columns — renders without its authored self-illumination. Bounded to Oblivion and Shivering Isles; no crash and no checkerboard (the role collapses to "absent").
- **Related**: #399 (glow slot reaches the shader), #1542, REN-D5-2026-10-08-01.
- **Suggested Fix**: Accept `DDPF_LUMINANCE` at 8 bpp (and `DDPF_LUMINANCE | DDPF_ALPHAPIXELS` at 16 bpp) by building an `RgbExpand { src_bpp, r_mask = g_mask = b_mask = luminance mask, a_mask }`. Add header fixtures for both.

### LOW

#### REN-D10-2026-10-08-01: #5249's transmission trace hands its foreign-hit leg an opaque-only mask, so glass blockers never attenuate the transmission half, contradicting the comment
- **Severity**: LOW
- **Dimension**: Soft Shadows
- **Location**: `crates/renderer/shaders/include/shadow_transport.glsl`, `traceShadowTransmittanceSkippingInstance`: `uint mask = visibilityMask & VISIBILITY_MASK_ALL_OPAQUE;` and the handoff `return traceShadowTransmittance(at, direction, remaining, emitterRadius, mask);`.
- **Status**: NEW.
- **Description**:
  - The doc comment says the first foreign hit "defers to the shared alpha/glass-aware trace … so translucent blockers keep their tint semantics".
  - `traceShadowTransmittanceDetailed` runs its glass loop only when `(visibilityMask & VISIBILITY_LAYER_GLASS) != 0u`. The mask passed down has that bit cleared, so the glass loop returns `vec3(1.0)` immediately.
  - Alpha-tested opaque blockers still keep their semantics; glass panes do not.
- **Evidence**: before #5192 the transmission lobes were multiplied by the full `selectedRayVisibilityMask` visibility. After #5249 they are multiplied by `transmissionVisibility`, which never sees glass.
- **Impact**: visual only and narrow. A back-lit surface behind a window pane (the Skyrim back-light lobe, the FO4 translucency lobe) keeps its transmission lobe untinted and unattenuated, while its reflection half is correctly attenuated by the same pane.
- **Related**: #5249, #5192, #4946.
- **Suggested Fix**: pass the full `visibilityMask` to the foreign-leg `traceShadowTransmittance` (the self-skip walk can stay opaque-only), or correct the comment to say glass is deliberately ignored.

#### REN-D5-2026-10-08-03: the `as_chunks` migration left `load_shader_module`'s `SAFETY` comment naming `chunks_exact(4)`
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/pipeline.rs`, `load_shader_module`, the `SAFETY` comment: "the SPIR-V was 4-byte aligned by the `chunks_exact(4)` decode above".
- **Status**: NEW (residue of `b24cb46b6`, #5308).
- **Description**: the decode is `spv.as_chunks::<4>().0`. The behaviour is identical (the remainder is dropped either way, and the function asserts `spv.len().is_multiple_of(4)` first), but the comment now names a call that no longer exists.
- **Suggested Fix**: reword to name the `is_multiple_of(4)` assert and `as_chunks`.

#### Existing #5211: still live
- `svgf_temporal.comp`, the nearest-tap fallback: `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb;` has no `isnan`/`isinf` test. The bilinear loop's check is not repeated. No commit touched the file.

#### Existing #5210: carried
- Open. The NIFAL spec/doc rot is unchanged by this delta.

## Prioritized Fix Order

1. **D5-01.** Route non-RGBA-order 32-bpp DDS through `RgbExpand` (or the BGRA format) and add mask-bearing fixtures. A small change on existing machinery with the widest reach in this report.
2. **D5-02.** Accept `DDPF_LUMINANCE` through the same machinery.
3. **D10-01.** One-line mask fix or comment fix, whichever the owner prefers.
4. **D5-03.** Comment.
5. **#5211** (open, carried) and **#5210** (open, carried).

## Needs-RenderDoc / live validation

None of these was run: no engine or GPU process was launched in this audit.

- **D5-01.** A live A/B of a Skyrim SE exterior vista against the post-fix build, at the boundary where full-detail terrain meets `.btr` LOD, to confirm the hue seam disappears. No sync or pipeline change is involved.
- **D5-02.** The Oblivion Gate (`endgame/battle.nif`) and Mehrunes Dagon glow, before and after.
- **#5249.** The commit reports a 120-frame FNV interior run at RT tier 1 with zero validation errors; a before/after capture on translucent content (FO4 BGSM foliage, back-lit Skyrim NPCs in shadowed interiors) is still owed.
- **#5270.** The wait-failure arm cannot be fault-injected with the real `ash::Device`; its classification rests on the spec's return-code set for an infinite-timeout wait.
- **Carried from the baseline** (all still unrun):
  - #5188 fault-injection of `build_geometry_ssbo` on the reclaim arm;
  - #5187 / #5194 a GPU-assisted-validation exterior crossing and a forced-rebuild fault;
  - #5215 sync-validation with an active combustion emitter (occupancy bindings 24/25);
  - #5057 early-test A/B; #5062 binding-16 RAW; `GpuLight` 64 B ArrayStride on device;
  - `p6-loading-model.sh` under validation; #4890 rapid resize, #5072, #4889, #4885;
  - the `r.upscaler` switch, `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, resize under FSR, the default FSR Quality validation run;
  - the FP32 SDK permutation, which is untested.
- **Measuring TLAS `updateScratchSize` vs `buildScratchSize`** on the 4070 Ti and RADV (baseline D9-03 / D1-01 follow-up) remains the way to turn #5250 from a spec-driven correctness fix into a measured one.

## Stale skill premises (for the next `/audit-renderer` sync)

**Setup / Dim 4**
- Both mentions of `triangle_early.frag` (Setup step 3 and the Dim 4 G-buffer bullet) name a file that does not exist. The variant is `triangle_early.frag.spv`, compiled from `triangle.frag` with `-DBYRO_OPAQUE_EARLY_TESTS=1` (`_audit-validate.sh` already lists both as advisory).
- `group_state`'s last key limb is now `needs_two_sided_blend_split(b)` (#2764), not the raw glass flag. The skill's guard line for `needs_two_sided_blend_split` is still right; add the two `group_state_tests` (`opaque_glass_flag_does_not_split_groups`, `blended_glass_flag_still_splits_groups`).

**Dim 2 / Dim 10**
- The `traceShadowTransmittanceSkippingInstance` description ("defers to the alpha/glass-aware trace on the first foreign hit") is not true of the code: the handoff mask is opaque-only (D10-01).

**Dim 5**
- The textures bullet covers only inputs that `bail!`. Add the *accepted-but-misdecoded* class and the two measured gaps (D5-01, D5-02), the fact that `make_uncompressed_header` never sets masks, and the census recipe (header scan of every `.dds`: pixel-format flags, `bpp`, four masks) so the next run can re-measure.

**`_audit-common.md`**
- The source-scan bullet still calls `crates/renderer/src/source_scan.rs` and `crates/physics/src/source_scan.rs` "older crate-private copies". After #5100 both are re-export shims of `byroredux_core::source_scan`, and `strip_test_modules` also lives in core.
- Dedup step 1 says `--limit 400`; the suite cache held 113 open issues, all consulted.

**Process**
- `check-shader-artifacts.sh` now accepts glslang `11:16.2.0` and `11:16.4.0`; the parity claim ("proven in both directions over the full 37-artifact set") is the script's own header.

## Guard posture

| Dim | Guards confirmed (exist, not `#[ignore]`d, ran green) | Blind spot found |
|---|---|---|
| 1 | `acceleration` filter (148 tests), TLAS barrier pin, static-BLAS recovery (lib + bin), `wait_failure_arm_disposes_only_on_device_loss`, `blas_scratch_realloc_order_tests` | — |
| 2 | `shader_contract` incl. #5191/#5192/#5249 pins, `direct_shadow_rays_orient_their_origin_toward_the_light`, glass identity/IOR, `light_history` | the #5249 pin checks the skip walk's tokens, not the handoff mask (D10-01) |
| 3 | size/offset/mirror/UBO/light-header, stale-size scan, `nothing_before_the_final_grow_binds_the_scene_set` | — |
| 4 | post-pass, egui, depth capture, FIF contract, blend split + new `group_state_tests`, reflection | — |
| 5 | geometry compaction/rebuild, skin-slot drain, allow-lists, `detach_tests`, one-time helper pins | DDS fixtures never set pixel-format masks or `DDPF_LUMINANCE` (D5-01/-02) |
| 6 | spawner guard, core PBR idempotence/glass overlay, corpus NIFAL (`--ignored`, 1/0) | — |
| 7 | TAA/SVGF/bloom/jitter/aperture | no SVGF NaN pin (#5211 open) |
| 8 | water/volumetrics/caustic, split-half scan lists | — |
| 9 | push-constant size, stride, `palette`, morph weak-ref, bin overflow/rollback | — |
| 10 | BSDF/light source-shape pins, bin light-policy tests, overflow warn | see Dim 2 |
| 11 | lib `exposure|tonemap|upscal|post_passes`, fsr3-sys 8/0, bin FSR default, `exposure_tuning_defaults_match_the_renderer_constants` | — |
| 12 | `gpu_timers`, bracket coverage, debug-mode guards, bin bench keys (22), `mat_set_tests` | — |

## Process notes

- **Dedup.**
  - Open issues came from the suite cache (`/tmp/audit/issues.json`, 113 open, reused and not regenerated).
  - Closed-issue keyword searches: "DDS luminance", "glow map DDS 8-bit", "A8R8G8B8 DDS channel order", "32-bpp DDS masks", "DDPF_LUMINANCE", "Unsupported DDS pixel format", "Oblivion glow map", "L8 luminance texture", "terrain LOD diffuse color", "red blue swapped texture", "BGRA uncompressed DDS", "channel masks DDS", "distant terrain tint", "uncompressed 32-bit DDS", "shader undefined constant guard", "GLSL source compiles test", "check-shader-artifacts glslang version", "uncompilable shader".
  - Only the adjacent items named under D5-01 and D5-02 matched.
- **Baseline closure check.** #5249, #5250, #5251, #5252, #5254, #5265 and #5270 are all CLOSED (`gh issue view`), and each code fix was read in its diff.
- **Dropped candidates.**
  - A hostile-header concern on the new loose-`.mat` resolver (`parse_loose_mat`: `Index as u8` wrap, `f64 as f32` overflow to infinity): mod-only input, no vanilla sample; owner `/audit-parsers` or `/audit-nifal`.
  - The extra transmission-trace ray cost not being charged to the adaptive ray budget: bounded by the transmission-lobe gate and `shadowFade`; owner `/audit-performance`.
  - The 641 Skyrim `textures/water/skyrim.esm/flow.*.dds` maps: A8R8G8B8 and therefore mis-decoded, but not loaded by any engine path (so no impact today).
  - The `sin(… fragWorldPos.x …)` turbulence term: precision-benign at the supported coordinate ceiling.
  - `average_rgb`'s uncompressed arm also assumes RGBA order (`swap_rb = false` for `R8G8B8A8_*`); it inherits D5-01 and is fixed by the same change.
- **Out-of-scope pointers.**
  - `docs/engine/` has no page describing the per-lobe visibility convention of #5192/#5249 (comments only, in `lighting.glsl`, `triangle.frag` and `shadow_transport.glsl`).
  - The `GSDocMitchellHouse` "authored-empty band" noted in #5368 is content/cell-loader territory (`/audit-fnv`).
