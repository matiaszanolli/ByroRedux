// Single source of truth for constants that live in both Rust and GLSL.
// This file is included (via include!) by both:
//   - crates/renderer/src/shader_constants.rs   (library)
//   - crates/renderer/build.rs                   (build script → generates shader_constants.glsl)
//
// When updating a value here, rebuild with `cargo build -p byroredux-renderer`
// to regenerate the GLSL header, then recompile the affected GLSL shaders.

// Cluster grid
pub const CLUSTER_TILES_X: u32 = 16;
pub const CLUSTER_TILES_Y: u32 = 9;
pub const CLUSTER_SLICES_Z: u32 = 24;
pub const CLUSTER_NEAR: f32 = 0.1;
pub const CLUSTER_FAR_FLOOR: f32 = 10_000.0;
pub const CLUSTER_FAR_FALLBACK: f32 = 50_000.0;
// Raised from 32 → 128 after the LIGH `falloff_exponent` plumb-through
// (which extended the per-light visible range to `radius * 2.5`)
// exposed densely-lit FO4 interior cells overflowing the cap. The
// cluster cull's `atomicAdd` gives arbitrary slot ordering when the
// actual light count exceeds the cap, so adjacent clusters drop
// DIFFERENT subsets of lights — producing visible tile boundaries on
// floors / walls (Institute Bioscience cargo room was the canonical
// regression). Buffer grows from 3456 * 32 * 4 = 442 KB to 3456 * 128
// * 4 = 1.7 MB — trivial against the multi-GB VRAM budget.
//
// 128 is sized for vanilla FO4 / FO76 dense-lighting interiors: live
// observation on Bioscience saw ~80-100 cluster-overlapping lights on
// the densest tiles after the range extension; 128 leaves headroom
// for modded / FO76-scale public-event scenes. Future workgroup-local
// sort would let us keep the cap lower by ordering by distance —
// deferred until the cap proves insufficient on Starfield / FO76
// content.
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 128;

// Vertex layout (global SSBO)
pub const VERTEX_STRIDE_FLOATS: u32 = 26;
// position(0..3) color RGBA(3..7) normal(7..10) uv(10..12) — see
// crates/renderer/src/vertex.rs.
pub const VERTEX_NORMAL_OFFSET_FLOATS: u32 = 7;
pub const VERTEX_UV_OFFSET_FLOATS: u32 = 10;

// Skinning — see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH`
// for the vanilla-content survey that fixes this ceiling at 144 (FO76 prewardress = 133).
pub const MAX_BONES_PER_MESH: u32 = 144;

// Skin-compute workgroup width. Both `skin_vertices.comp` (1 invocation per
// vertex) and `skin_palette.comp` (1 invocation per bone slot) run a 1D
// 64-wide dispatch; the Rust dispatch group-count math in
// `vulkan/skin_compute.rs` (`*.div_ceil(SKIN_WORKGROUP_SIZE)`) re-exports
// this same const so the layout qualifier and the group count can't drift.
// Distinct from `WORKGROUP_X = 8` (the 2D image-pass tile width) — skinning
// is a flat 1D dispatch. Emitted with no `u` suffix so it works in the
// `layout(local_size_x = SKIN_WORKGROUP_SIZE)` qualifier. #1758 / TD7-001.
pub const SKIN_WORKGROUP_SIZE: u32 = 64;

// Material kind enum (GpuMaterial.materialKind).
// Authoritative Rust-side values live in `scene_buffer/constants.rs`. #1401.
pub const MATERIAL_KIND_GLASS: u32 = 100;
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;
pub const MATERIAL_KIND_NO_LIGHTING: u32 = 102;

// TLAS instance shadow-ray mask buckets (the 8-bit mask AND'd against a
// ray query's cullMask — see the extension-point comment at
// `acceleration/tlas.rs`'s `instance_custom_index_and_mask` site). Every
// instance gets exactly one bucket; every EXISTING ray query still passes
// cullMask=0xFF (matches every bucket, no behavior change) except the new
// interior-godray two-pass shadow ray in `volumetrics_inject.comp`, which
// queries `SHADOW_MASK_OPAQUE` alone first, then `SHADOW_MASK_GLASS` alone
// with a bounded tMax. Only 2 of 8 bits used; remaining bits are reserved
// for future per-light-type segregation (per the original REN-D8-NEW-07
// extension-point note).
pub const SHADOW_MASK_OPAQUE: u32 = 0x01;
pub const SHADOW_MASK_GLASS: u32 = 0x02;

// #1913 — these buckets are declared `u32` here but packed into the 8-bit
// `mask` field of `Packed24_8` at the TLAS instance build
// (`acceleration/tlas.rs`) via `as u8`, which silently truncates anything
// ≥ 0x100. A bucket that truncates to 0 is skipped by EVERY ray query
// regardless of `cullMask` — silent, total RT dropout (no shadows /
// reflections / GI) for that geometry, invisible to `cargo test` and to
// validation layers. Pin the 8-bit ceiling + nonzero + distinctness at the
// definition site so a future `SHADOW_MASK_FOLIAGE = 0x100` fails the build
// here instead of vanishing at runtime. Mirrors the 24-bit `ssbo_idx`
// guard in the same TLAS build (#957).
const _: () = {
    assert!(
        SHADOW_MASK_OPAQUE != 0 && SHADOW_MASK_OPAQUE <= 0xFF,
        "SHADOW_MASK_OPAQUE must be a nonzero 8-bit value (packed as u8 into Packed24_8)"
    );
    assert!(
        SHADOW_MASK_GLASS != 0 && SHADOW_MASK_GLASS <= 0xFF,
        "SHADOW_MASK_GLASS must be a nonzero 8-bit value (packed as u8 into Packed24_8)"
    );
    assert!(
        SHADOW_MASK_OPAQUE != SHADOW_MASK_GLASS,
        "SHADOW_MASK buckets must be distinct or ray-query narrowing collapses"
    );
};

// Glass / IOR ray budget. The per-frame atomic ray pool for the glass
// IOR refraction path; when exhausted, glass fragments drop to the
// cheaper Fresnel-only fallback. The old 8192 (≈2048 IOR fragments)
// starved on any large/close glass — a full-screen pane or a hand-held
// sphere blew it in a 16×16 px patch, and the binary IOR/fallback split
// painted a per-fragment stipple. Two megarays cover 524,288 IOR fragments
// at the four-ray worst-case claim: enough for close hero props while the
// fallback bounds full-screen bottle/pane overdraw.
pub const GLASS_RAY_BUDGET: u32 = 2_097_152;
pub const GLASS_RAY_COST: u32 = 4;

// First-bounce GI candidate pool. The shader ranks these locally, then stops
// after the first two VISIBLE contributors. Keeping eight candidates avoids a
// black bounce when the strongest one or two lamps are behind a wall without
// paying eight shadow rays on the common path.
pub const GI_HIT_LIGHT_CAP: u32 = 8;

// Caustic accumulation
pub const CAUSTIC_FIXED_SCALE: f32 = 65536.0;

// Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;
pub const WORKGROUP_Z: u32 = 8;

// Clustered light culling thread count (one warp/wavefront wide on
// every IHV: NVIDIA = 32, AMD = 64 wavefront but a 32-thread workgroup
// still maps cleanly to half a wave, Intel = 8/16/32 SIMD width
// negotiates fine at this size). Consumed by `cluster_cull.comp` via
// the `#include`d `#define` for both `layout(local_size_x = ...)` and
// the thread-strided light scan loop. Omitted `u` suffix on the
// generated `#define` so it can be used in the layout qualifier
// (GLSL allows int literals but not `uint` literals there).
pub const THREADS_PER_CLUSTER: u32 = 32;

// M58 — bloom contribution coefficient. 0.15 (≈4× the Frostbite
// SIGGRAPH 2015 default of 0.04) compensates for Bethesda content
// being LDR-authored: emissive surfaces sit in the 0–1 monitor-space
// range rather than HDR cd/m², so a Frostbite-default intensity reads
// as essentially-invisible. Hand-tuned downward from 0.20 on
// Prospector saloon (sun-lit windows + chandelier globes were
// producing halos that bled too far across walls); 0.15 keeps
// emissives obviously bloomed without flooding dim surfaces.
// Consumed by `composite.frag` via the `#include`d `#define`; mirrored
// here so Rust-side `bloom::DEFAULT_BLOOM_INTENSITY` stays in lockstep.
// See `feedback_color_space.md` for why we don't HDR-boost emissives
// globally instead.
pub const BLOOM_INTENSITY: f32 = 0.15;

// M55 — volumetric far plane in Bethesda world units. Must match
// `volumetrics::DEFAULT_VOLUME_FAR`
// (Rust side) and the `params.volume_extent.x` value passed to the
// injection compute pass; otherwise the slice→view-distance mapping
// disagrees and fog appears compressed or stretched. With Phase 3
// pre-integration the per-fragment cost is now ONE sampler3D tap, so
// no step-count dial is needed in `composite.frag` — quality scales
// with the froxel resolution and dt set on the host. Consumed by
// `composite.frag` (slice math) and `volumetrics_integrate.comp` (dt =
// VOLUME_FAR / FROXEL_DEPTH).
//
// The renderer deliberately preserves Gamebryo coordinates (70 units per
// metre). The original 200.0 value was documented and tuned as 200 metres but
// consumed directly beside world-space positions, truncating the volume at
// 2.86 m. 14,000 units restores the intended 200 m reach; volumetric density
// is converted from 1/m to 1/world-unit on the host side in volumetrics.rs.
pub const VOLUME_FAR: f32 = 14_000.0;

// Per-instance flag bits on `GpuInstance.flags` (lower 16 bits — the
// upper 16 bits pack the terrain-tile slot per
// `INSTANCE_TERRAIN_TILE_SHIFT/MASK`). Authoritative Rust-side values
// live in `crates/renderer/src/vulkan/scene_buffer/constants.rs`; this
// shader-side mirror is pinned equal via
// `instance_flag_bits_match_scene_buffer_consts` so the two layers
// can't drift. See #1190 (TD4-NEW-01). The reserved PRESKINNED bit
// (bit 6) is not emitted as a shader-side flag because nothing in
// GLSL reads it today; if it grows a consumer, add the bit + a
// matching `#define` to keep the include the single source of truth.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
// Bit offset/mask for the `RenderLayer` classification packed into
// bits 4..5 of `GpuInstance.flags` (#2045 / TD7-101). Previously
// hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in
// `triangle.frag` with no lockstep test, unlike every other
// `INSTANCE_FLAG_*` bit; pinned equal to
// `scene_buffer::constants::INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` via
// `instance_render_layer_bits_match_scene_buffer_consts`. Consumed by
// the fragment shader's `DBG_VIZ_RENDER_LAYER` debug-viz branch.
pub const INSTANCE_RENDER_LAYER_SHIFT: u32 = 4;
pub const INSTANCE_RENDER_LAYER_MASK: u32 = 0x3;
pub const INSTANCE_FLAG_FLAT_SHADING: u32 = 1 << 7;
// bit 8 — diffuse texture carries a genuine authored alpha channel
// (BC2/BC3/BC7/RGBA). Set CPU-side from the cached `handle_has_alpha`
// classification (`format_has_alpha`, which excludes BC1_RGBA). When
// CLEAR, `triangle.frag` pins `texColor.a` to 1.0 (unless an alpha test
// is active) so a BC1 3-colour-block texel (1-bit punch-through, not
// authored alpha) can't leak transparency into the discard / decalWeight
// / finalAlpha paths on a pure-blend mesh. See #1653.
pub const INSTANCE_FLAG_DIFFUSE_ALPHA: u32 = 1 << 8;

// Per-material flag bits on `GpuMaterial.materialFlags`. Authoritative
// Rust-side values live in `crates/renderer/src/vulkan/material.rs`
// (`material_flag::*`); this shader-side mirror is pinned equal via
// `material_flag_bits_match_material_consts`. See #1190. build.rs emits
// these as `#define`s into `shader_constants.glsl`, so `triangle.frag`
// MUST get them from the `#include` — never hand-write them.
//
// Bits 5-9 (the #1147 Phase 2a / #1248-#1250 Disney BSDF + SSS +
// model-space-normals suite) were previously hand-written `#define`s in
// `triangle.frag` with no lockstep test; #1285 brought them into the
// generated header alongside bits 0-4.
pub const MAT_FLAG_VERTEX_COLOR_EMISSIVE: u32 = 1 << 0;
pub const MAT_FLAG_EFFECT_SOFT: u32 = 1 << 1;
pub const MAT_FLAG_EFFECT_PALETTE_COLOR: u32 = 1 << 2;
pub const MAT_FLAG_EFFECT_PALETTE_ALPHA: u32 = 1 << 3;
pub const MAT_FLAG_EFFECT_LIT: u32 = 1 << 4;
pub const MAT_FLAG_PBR_BSDF: u32 = 1 << 5;
pub const MAT_FLAG_TRANSLUCENCY: u32 = 1 << 6;
pub const MAT_FLAG_MODEL_SPACE_NORMALS: u32 = 1 << 7;
pub const MAT_FLAG_TRANSLUCENCY_THICK_OBJECT: u32 = 1 << 8;
pub const MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO: u32 = 1 << 9;
// Non-occluding glass sheet/shell. This is a canonical behavior flag, not a
// BGEM provenance bit: any source translator may select it.
pub const MAT_FLAG_THIN_GLASS: u32 = 1 << 11;
// `MAT_FLAG_EFFECT_LI_SHIFT` — bit offset for the 8-bit
// `BSEffectShaderProperty.lighting_influence` byte packed into bits
// 16-23 of `materialFlags`. Extract via
// `float((mat.materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFFu) / 255.0`.
// Paired with `material_flag::EFFECT_LI_SHIFT` (Rust) and pinned by
// `material_flag_bits_match_material_consts`. See #890 Stage 2.
pub const MAT_FLAG_EFFECT_LI_SHIFT: u32 = 16;

// NOTE: `material_flag::BGSM_AUTHORED` (Rust-side bit 10) is
// NOT mirrored here — the shader is format-agnostic and doesn't
// branch on material provenance. BGSM → standardized PBR
// translation happens in `merge_bgsm_into_mesh`, which writes
// `metalness_override` / `roughness_override` on the raw-tier
// `ImportedMesh`; `translate_material` then resolves those into
// `Material.{metalness,roughness}`. The Rust-side flag rides
// through for debug-server inspection only.
// See `feedback_format_translation.md`.

// High bit OR'd into `GpuMaterial.glossMapIndex` to tell the fragment shader
// "the gloss/smoothness mask lives in the NORMAL map's ALPHA channel"
// (Skyrim/Gamebryo normal-alpha-as-spec). Set per-draw CPU-side in
// `byroredux::render::static_meshes`; the shader masks it off for the index
// (`glossMapIndex & ~NORMAL_ALPHA_SPEC_BIT`) and samples `.a` instead of `.r`.
// Lockstep with `triangle.frag` and `byroredux::material_translate`, which
// re-exports this value rather than re-declaring it (#1500 / REN2-15).
pub const NORMAL_ALPHA_SPEC_BIT: u32 = 0x8000_0000;

// Water motion-kind enum (WATR-driven, mapped per-WATR record).
// Lockstep with `water.frag` and `byroredux/src/cell_loader/water.rs`.
pub const WATER_CALM: u32 = 0;
pub const WATER_RIVER: u32 = 1;
pub const WATER_RAPIDS: u32 = 2;
pub const WATER_WATERFALL: u32 = 3;

// Debug-viz bit flags packed into `jitter.z` by the renderer
// (`parse_render_debug_flags_env` + `GpuCamera` upload). Runtime-set
// via `BYROREDUX_RENDER_DEBUG=<bitmask>` env var or console for
// renderer-artifact bisection. Branches collapse to free no-ops when
// the env var is unset. Consumed by `triangle.frag` via the `#include`d
// `#define`s; this file is the single source of truth.

/// 0x1 — bypass parallax-occlusion mapping in the base-UV sampler.
pub const DBG_BYPASS_POM: u32 = 0x1;

/// 0x2 — bypass detail-map blend on the base albedo.
pub const DBG_BYPASS_DETAIL: u32 = 0x2;

/// 0x4 — visualize per-fragment world-space normal as colour.
pub const DBG_VIZ_NORMALS: u32 = 0x4;

/// 0x8 — visualize per-fragment tangent presence:
///   * green = tangent present (vertex shader fed authored or synthesized
///     data → Path 1 in `perturbNormal` fires).
///   * red = zero tangent → screen-space derivative fallback (Path 2).
///
/// Added under #783 follow-up.
pub const DBG_VIZ_TANGENT: u32 = 0x8;

/// 0x10 — skip the per-fragment normal-map perturbation entirely;
/// lighting uses the geometric vertex normal. Use to bisect whether a
/// chrome / posterization artifact originates from `perturbNormal`
/// (Path 1 or Path 2 TBN bug) or from downstream specular / ambient
/// code. Default-on path runs `perturbNormal`; this bit is the opt-out.
/// 2026-05-03 / #786 closeout reinstated the default-on behaviour after
/// the convention-swap fix at 5dde345 + the BSTriShape inline-tangent
/// decode at b63ab0c.
pub const DBG_BYPASS_NORMAL_MAP: u32 = 0x10;

/// 0x20 — RESERVED. Pre-#1035 (in the 77aa2de → 5dde345 window) this
/// bit was the opt-IN for `perturbNormal` while the default was off
/// (was named `DBG_FORCE_NORMAL_MAP`). After #786 closed (2026-05-03)
/// the default flipped back to on and the bit became a silent no-op.
/// Kept reserved so existing diagnostic scripts using
/// `BYROREDUX_RENDER_DEBUG=0x20` / `0x24` / `0x28` keep working as
/// no-ops; renamed at #1035 to make the no-op status explicit in the
/// bit catalog.
pub const DBG_RESERVED_20: u32 = 0x20;

/// 0x40 — visualize the per-entity content-class render layer driving
/// the depth-bias ladder. Tints fragments by layer:
///   * Architecture (0) → grey
///   * Clutter (1)      → cyan
///   * Actor (2)        → magenta
///   * Decal (3)        → yellow
///
/// The 2-bit layer is packed into `gpuInstance.flags` bits 4..5
/// (`INSTANCE_RENDER_LAYER_SHIFT` / `_MASK` on the Rust side).
pub const DBG_VIZ_RENDER_LAYER: u32 = 0x40;

/// 0x80 — glass IOR refraction passthru-loop diagnostic (#789
/// follow-up). Tints glass fragments by where the loop terminated:
///   * black   — IOR not allowed (rtLOD ≥ 2.0, !isGlass post-LOD-downgrade,
///     ray budget exhausted, isWindow not demoted).
///   * red     — IOR fired but ray escaped scene (sky fallback).
///   * yellow  — terminated on first hit, no passthru (different texture
///     from start — desk / wall / non-glass behind the surface).
///   * green   — passthru ×1, then non-self terminus (one self skip,
///     then real scene geometry).
///   * cyan    — passthru ×2 with non-self terminus (two self skips +
///     real geometry, e.g. through one stacked beaker to wall behind).
///   * magenta — budget exhausted, terminus STILL same-texture
///     (passthru never escaped the glass — three+ glass surfaces in a
///     row).
pub const DBG_VIZ_GLASS_PASSTHRU: u32 = 0x80;

/// 0x100 — disable specular antialiasing (`specularAaRoughness`).
/// Every per-light + RT-reflection BRDF site widens the authored
/// `roughness` by the screen-space normal-variance kernel before
/// feeding it to GGX/Smith. Setting this bit returns to the raw
/// authored roughness so the Kaplanyan-Hoffman 2016 bug-class
/// (corrugated normal map → bright/dark stripes at distance) can be
/// A/B'd against a regression suspect that turns out to be the spec-AA
/// itself. Default-on; this bit is the opt-out.
pub const DBG_DISABLE_SPECULAR_AA: u32 = 0x100;

/// 0x200 — disable half-Lambert wrap on interior-fill directional.
/// Interior cells upload the XCLL directional with `radius == -1` as
/// a "subtle aesthetic fill" (`render::compute_directional_upload`).
/// The default-on path uses half-Lambert (`dot(N,L) * 0.5 + 0.5`) for
/// the diffuse term so corrugated normal maps don't produce pitch-
/// black grooves where `NdotL → 0` (Nellis Museum was the canonical
/// regression — bright/dark stripes following corrugation period
/// across the entire hut interior). Specular still uses plain
/// `NdotL` so back-facing fragments don't get fake highlights.
/// Set this bit to A/B against the legacy Lambert path.
pub const DBG_DISABLE_HALF_LAMBERT_FILL: u32 = 0x200;

/// 0x400 — bypass the per-vertex color modulation of albedo
/// (`albedo *= fragColor`). Bethesda bakes per-vertex lighting / AO into
/// static-geometry vertex colors; on coarsely-tessellated meshes (e.g.
/// FNV casino floor tiles — `NV_TOPS_CasLoRmMid01` carries vertex-luma
/// 0.16–1.00 over only 40 verts) that baked term interpolates across
/// large triangles into hard-edged bright/dark patches whose boundaries
/// follow the triangulation. Set this bit to confirm a "lighting error
/// only on certain polygons, at a fixed position" is the baked
/// vertex-color term rather than a dynamic / RT-side cause (Tops floor
/// diagnosis 2026-05-27). Does not affect `SOURCE_EMISSIVE` vertex mode
/// (that path routes vertex color through the emissive accumulator).
pub const DBG_BYPASS_VERTEX_COLOR: u32 = 0x400;

/// 0x800 — force ambient occlusion to 1.0 (disable both the screen-space
/// SSAO sample and the RT-AO term in `combinedAO`). Use to bisect whether
/// a hard-edged dark floor patch is AO over-darkening (vanishes with this
/// bit) versus a cast shadow / direct-light occlusion (persists). Paired
/// with `DBG_BYPASS_VERTEX_COLOR` these isolate the two most common
/// "lighting only on certain polygons" causes without touching shadows.
pub const DBG_DISABLE_AO: u32 = 0x800;

/// 0x1000 — revert point/spot lights to the pre-REND-#1451 attenuation:
/// the anti-pop-in cull window doing the ENTIRE attenuation job
/// (`atten = pow(clamp(1 − (d/R)², 0, 1), shape)`, `R = .w`). That
/// formula reads 75% at the authored radius (`d = R/2`) — the bright
/// near-zone ring (Lonesome Road / Ulysses Temple). Default-off path
/// now uses the OpenMW-style two-term model: a physical near-zone
/// falloff keyed to the AUTHORED radius (`knee = dofParams.z × .w`)
/// MULTIPLIED by a soft cull window that fades full→zero from the
/// authored radius out to `.w`. Set this bit to A/B the new model
/// against the legacy one in the same live session (no rebuild) while
/// running the REND-#1451 controlled bench. Also settable via the
/// `light.atten legacy on|off` console command (routes through the
/// `LightTuning` resource → `VulkanContext::light_atten_legacy`).
pub const DBG_LEGACY_LIGHT_ATTEN: u32 = 0x1000;

/// 0x2000 — disable multi-scatter energy compensation
/// (`multiScatterEnergyCompensation`, Fdez-Agüera 2019 / Filament). The
/// default-on path multiplies the single-scatter Cook-Torrance specular
/// lobe by `1 + F0·(1/Ess − 1)` to restore the energy lost to microfacet
/// masking as roughness rises — without it, rough conductors (brushed
/// steel, satin, cookware) progressively darken. The factor is a no-op at
/// low roughness (`Ess → 1`), so it cannot shift the RT reflection
/// roughness gate. Set this bit to A/B the compensated rough metal
/// against the legacy single-scatter look in one live session.
pub const DBG_DISABLE_MULTISCATTER: u32 = 0x2000;

/// 0x4000 — disable the SVGF spatial à-trous wavelet pass
/// (`svgf_atrous.comp`, Schied 2017 §4.3). The default-on path runs the
/// variance-guided edge-stopping wavelet filter after temporal
/// accumulation to remove the per-pixel GI variance the temporal pass
/// leaves behind (the noisy / slow-moiré floor). Setting this bit turns
/// every à-trous iteration into a pass-through copy, so the composite
/// samples the raw temporal-only result — the pre-Phase-4 look — for live
/// A/B in one session.
pub const DBG_DISABLE_ATROUS: u32 = 0x4000;

/// 0x8000 — disable ReSTIR-DI direct-shadow reservoir reuse and fall back
/// to the legacy per-frame WRS streaming-RIS shadow sampling. ReSTIR-DI
/// (Bitterli 2020) reuses shadow reservoirs across frames (and, in a later
/// phase, neighbours) so the direct soft-shadow estimate accumulates many
/// effective samples instead of re-randomising every frame — fixing the
/// "incredibly noisy + slow moiré" direct shadows the un-denoised WRS path
/// produced (`resFrameSeed = cameraPos.w`). Set this bit to A/B ReSTIR
/// against the legacy WRS path in one live session.
pub const DBG_DISABLE_RESTIR: u32 = 0x8000;

/// 0x10000 — disable ReSTIR-DI **spatial** reservoir reuse (ReSTIR "P2",
/// Bitterli 2020 §5) while leaving the temporal reuse (`DBG_DISABLE_RESTIR`
/// path) active. The default-on path samples a small disk of neighbour
/// reservoirs from the *previous* frame's buffer around the reprojected
/// pixel, re-evaluates each neighbour's selected light against the **current**
/// surface (target pdf p̂), and combines them with the same 1/M streaming-RIS
/// estimator the temporal path uses — so a freshly disoccluded or fast-moving
/// pixel inherits many effective samples from its neighbourhood instead of
/// restarting from a single noisy frame. It also seeds the soft-shadow colour
/// EMA from valid neighbours on disocclusion (where temporal reprojection
/// fails), which is what visibly removes the "convergence resets on camera
/// motion" restart noise. Set this bit to A/B temporal-only ReSTIR against the
/// full spatiotemporal path in one live session.
pub const DBG_DISABLE_SPATIAL: u32 = 0x10000;

/// 0x20000 — #1874 diagnostic: visualise the per-fragment screen-space motion
/// vector (`outMotion`, the G-buffer velocity SVGF + TAA reproject with) as
/// colour, so the "ghosted diagonal double-image" can be root-caused live
/// without a RenderDoc capture. Encoding: `rg = 0.5 + motion.xy * scale`,
/// `b = 0.5` — a static camera reads flat grey `(0.5, 0.5, 0.5)` everywhere.
///
/// The decisive read for issue #1874's hypothesis H1 (a *spatially-uniform*
/// bad motion vector shared by SVGF and TAA): under this view a real camera
/// translation shows motion that **varies with depth** (near geometry tints
/// harder than far — parallax), whereas the suspected fault paints the **whole
/// screen one uniform non-grey tint** (a post-projection screen shift with no
/// depth dependence — a stale/jittered `prevViewProj`, not real motion). Park
/// the camera on the artifact and set this bit: uniform tint ⇒ camera-level
/// (`prevViewProj`/origin), depth-varying-but-localised-to-a-body tint ⇒
/// skinning. Diagnostic-only — gated entirely behind the debug bit, no effect
/// on normal rendering.
pub const DBG_VIZ_MOTION: u32 = 0x20000;

/// 0x40000 — disable ReSTIR-DI **temporal** reservoir and radiance-history
/// reuse while leaving current-frame sampling and (unless separately disabled
/// by [`DBG_DISABLE_SPATIAL`]) previous-frame spatial-neighbour reuse active.
/// This separates the two reuse dimensions for controlled evaluation:
///
/// - default: temporal + spatial reuse;
/// - `DBG_DISABLE_SPATIAL`: temporal-only;
/// - `DBG_DISABLE_TEMPORAL`: spatial-only;
/// - both bits: current-frame reservoir only.
///
/// Unlike `DBG_DISABLE_RESTIR`, this never selects the compile-time-gated
/// legacy WRS implementation. It keeps the same ReSTIR estimator and only
/// removes the centre-pixel temporal candidate plus its colour EMA history.
pub const DBG_DISABLE_TEMPORAL: u32 = 0x40000;

/// 0x80000 — display the fragment shader's resolved indirect-light signal
/// directly, before SVGF history and before multiplication by local albedo in
/// composite. This distinguishes "GI rays returned darkness" from
/// "denoising/compositing/exposure buried valid indirect energy".
pub const DBG_VIZ_RAW_INDIRECT: u32 = 0x80000;

/// 0x100000 — display final raster material classification. Opaque surfaces
/// are grey, alpha-tested surfaces green, alpha-blended surfaces red, and
/// glass blue. The diagnostic writes alpha=1 so blend-state membership is
/// visible as a solid classification rather than being obscured by the very
/// transparency defect under investigation.
pub const DBG_VIZ_MATERIAL_STATE: u32 = 0x100000;

/// 0x200000 — display only the stochastic ray-traced diffuse GI bounce,
/// excluding authored cell ambient, AO, reflections, direct light, and SVGF.
/// This is the decisive probe for whether the GI ray estimator contributes
/// energy in a real-content scene.
pub const DBG_VIZ_GI_BOUNCE: u32 = 0x200000;

/// Single source of truth for every `DBG_*` debug-viz bit, in emit order.
/// Both `build.rs` (GLSL header emit) and `shader_constants.rs`'s test
/// module (`generated_header_contains_all_defines` value-pin,
/// `triangle_frag_dbg_bits_not_redeclared` no-shadow guard, and
/// `dbg_bits_catalog_covers_every_dbg_constant`) drive off this single list,
/// so a new `DBG_*` constant can no longer land covered by only one (or
/// zero) of those three contracts. Pre-#1860 `build.rs` hand-emitted each
/// `DBG_*` `writeln!` separately and this catalog (then test-only, living in
/// `shader_constants.rs`) had drifted to 13 of 18 constants — the 5 newest
/// bits (`DBG_DISABLE_MULTISCATTER`/`ATROUS`/`RESTIR`/`SPATIAL`,
/// `DBG_VIZ_MOTION`) bypassed both the value-pin and the no-redeclare guard
/// silently. See #1482 (original catalog fix) / #1860 (this fix, moving the
/// catalog here so `build.rs` can drive its emit from it too).
pub const DBG_BITS: &[(&str, u32)] = &[
    ("DBG_BYPASS_POM", DBG_BYPASS_POM),
    ("DBG_BYPASS_DETAIL", DBG_BYPASS_DETAIL),
    ("DBG_VIZ_NORMALS", DBG_VIZ_NORMALS),
    ("DBG_VIZ_TANGENT", DBG_VIZ_TANGENT),
    ("DBG_BYPASS_NORMAL_MAP", DBG_BYPASS_NORMAL_MAP),
    ("DBG_RESERVED_20", DBG_RESERVED_20),
    ("DBG_VIZ_RENDER_LAYER", DBG_VIZ_RENDER_LAYER),
    ("DBG_VIZ_GLASS_PASSTHRU", DBG_VIZ_GLASS_PASSTHRU),
    ("DBG_DISABLE_SPECULAR_AA", DBG_DISABLE_SPECULAR_AA),
    (
        "DBG_DISABLE_HALF_LAMBERT_FILL",
        DBG_DISABLE_HALF_LAMBERT_FILL,
    ),
    ("DBG_BYPASS_VERTEX_COLOR", DBG_BYPASS_VERTEX_COLOR),
    ("DBG_DISABLE_AO", DBG_DISABLE_AO),
    ("DBG_LEGACY_LIGHT_ATTEN", DBG_LEGACY_LIGHT_ATTEN),
    ("DBG_DISABLE_MULTISCATTER", DBG_DISABLE_MULTISCATTER),
    ("DBG_DISABLE_ATROUS", DBG_DISABLE_ATROUS),
    ("DBG_DISABLE_RESTIR", DBG_DISABLE_RESTIR),
    ("DBG_DISABLE_SPATIAL", DBG_DISABLE_SPATIAL),
    ("DBG_VIZ_MOTION", DBG_VIZ_MOTION),
    ("DBG_DISABLE_TEMPORAL", DBG_DISABLE_TEMPORAL),
    ("DBG_VIZ_RAW_INDIRECT", DBG_VIZ_RAW_INDIRECT),
    ("DBG_VIZ_MATERIAL_STATE", DBG_VIZ_MATERIAL_STATE),
    ("DBG_VIZ_GI_BOUNCE", DBG_VIZ_GI_BOUNCE),
];

/// #1799 / PERF-D5-NEW-01 — compile-time gate for the legacy 16-slot WRS
/// reservoir arrays (`resLight[16]` / `resWSel[16]`) that `DBG_DISABLE_RESTIR`
/// A/Bs against. `DBG_DISABLE_RESTIR` is a RUNTIME bit read from a uniform, so
/// even on the ~100% of production frames that take the ReSTIR path and never
/// touch those arrays, the compiler still had to budget their per-invocation
/// register / local-memory footprint — the declaration + init loop ran
/// unconditionally, ahead of the runtime `useRestir` branch that gated
/// everything else about them. glslangValidator's preprocessor, unlike the
/// runtime branch, can actually eliminate dead code — but only if the
/// legacy-WRS source text is behind a `#if`, not an `if` on a uniform value.
///
/// `0` (default): the legacy WRS arm — declarations, streaming writes, and
/// pass-2 shadow-ray reads — is preprocessed OUT of `triangle.frag` entirely.
/// `useRestir` collapses to `rtEnabled` (no `dbgFlags` read for this bit) and
/// `DBG_DISABLE_RESTIR` becomes a no-op bit in this build.
///
/// `1`: restores the pre-fix behavior verbatim — the legacy arrays exist and
/// `DBG_DISABLE_RESTIR` again live-toggles between the two paths at runtime.
/// Flip this to `1` and recompile `triangle.frag` to A/B; per the mechanism
/// #1758 established for `SKIN_WORKGROUP_SIZE`, that A/B now costs a shader
/// recompile instead of a per-frame register tax on every production build.
pub const ENABLE_LEGACY_WRS: u32 = 0;
