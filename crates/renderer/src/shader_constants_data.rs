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
pub const VERTEX_STRIDE_FLOATS: u32 = 25;
// position(0..3) color(3..6) normal(6..9) uv(9..11) — see crates/renderer/src/vertex.rs.
pub const VERTEX_NORMAL_OFFSET_FLOATS: u32 = 6;
pub const VERTEX_UV_OFFSET_FLOATS: u32 = 9;

// Skinning — see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH`
// for the vanilla-content survey that fixes this ceiling at 144 (FO76 prewardress = 133).
pub const MAX_BONES_PER_MESH: u32 = 144;

// Material kind enum (GpuMaterial.materialKind).
// Authoritative Rust-side values live in `scene_buffer/constants.rs`. #1401.
pub const MATERIAL_KIND_GLASS: u32 = 100;
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;
pub const MATERIAL_KIND_NO_LIGHTING: u32 = 102;

// Glass / IOR ray budget. The per-frame atomic ray pool for the glass
// IOR refraction path; when exhausted, glass fragments drop to the
// cheaper Fresnel-only fallback. The old 8192 (≈2048 IOR fragments)
// starved on any large/close glass — a full-screen pane or a hand-held
// sphere blew it in a 16×16 px patch, and the binary IOR/fallback split
// painted a per-fragment stipple. Raised to cover a screenful of glass on
// the dev-class GPU (RTX 4070 Ti, 12 GB) so refraction actually engages;
// still bounded so a pathological all-glass cell can't run unbounded.
pub const GLASS_RAY_BUDGET: u32 = 1048576;
pub const GLASS_RAY_COST: u32 = 4;

// One-bounce GI: max lights evaluated (with a shadow ray each) at a GI
// bounce hit. Caps the per-GI-hit cost in dense cells; the bounce only
// needs the dominant nearby lights, so a small cap keeps colour bleed
// correct without scanning hundreds of lights per hit.
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

// M55 — volumetric far plane. Must match `volumetrics::DEFAULT_VOLUME_FAR`
// (Rust side) and the `params.volume_extent.x` value passed to the
// injection compute pass; otherwise the slice→view-distance mapping
// disagrees and fog appears compressed or stretched. With Phase 3
// pre-integration the per-fragment cost is now ONE sampler3D tap, so
// no step-count dial is needed in `composite.frag` — quality scales
// with the froxel resolution and dt set on the host. Consumed by
// `composite.frag` (slice math) and `volumetrics_integrate.comp` (dt =
// VOLUME_FAR / FROXEL_DEPTH).
pub const VOLUME_FAR: f32 = 200.0;

// Per-instance flag bits on `GpuInstance.flags` (lower 16 bits — the
// upper 16 bits pack the terrain-tile slot per
// `INSTANCE_TERRAIN_TILE_SHIFT/MASK`). Authoritative Rust-side values
// live in `crates/renderer/src/vulkan/scene_buffer/constants.rs`; this
// shader-side mirror is pinned equal via
// `instance_flag_bits_match_scene_buffer_consts` so the two layers
// can't drift. See #1190 (TD4-NEW-01). The render-layer slot
// (bits 4..5) and the reserved PRESKINNED bit (bit 6) are not
// emitted as shader-side flags because nothing in GLSL reads them
// today; if they grow consumers, add the bit + a matching `#define`
// to keep the include the single source of truth.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
pub const INSTANCE_FLAG_FLAT_SHADING: u32 = 1 << 7;

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
///   * black   — IOR not allowed (rtLOD ≥ 1.0, !isGlass post-LOD-downgrade,
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
