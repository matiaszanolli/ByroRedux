//! Scene-buffer sizing limits + GPU-side flag bits.
//!
//! Every `MAX_*` capacity ceiling, every `INSTANCE_FLAG_*` bit, every
//! material-kind discriminator. Owned in one file so a tunable change shows
//! up as a single diff.

use ash::vk;


/// Maximum lights we can upload per frame. The SSBO is pre-allocated to this size.
/// 512 lights × 48 bytes = 24 KB per frame — trivial.
pub(super) const MAX_LIGHTS: usize = 512;

/// Maximum bones we can upload per frame across all skinned meshes.
/// 32768 × 64 B = 2 MB/frame × 3 frames-in-flight = 6 MB total. Slot 0
/// is a reserved identity fallback (used by rigid vertices through
/// the sum-of-weights escape hatch and by `SkinnedMesh` bones that
/// failed to resolve). The remaining slots are assigned sequentially
/// per skinned mesh, with each mesh consuming `MAX_BONES_PER_MESH`
/// (128) slots for simplicity. That gives ~255 skinned meshes per
/// frame — covers ~36 NPCs at 7 skinned meshes each (skeleton + body
/// + 6 sub-meshes) plus rigid scene content. Pre-M41.0 the cap was
/// 4096 (~31 meshes) which suited the no-NPC-spawn baseline; once
/// M41.0 Phase 1b started spawning multiple actors per cell the
/// silent-bind-pose-fallback hid spawned NPCs (FNV Prospector
/// rendered the first ~4 actors then dropped the rest). The proper
/// fix is variable-stride packing (M29.5); this constant just buys
/// headroom until then. See `bone_palette` overflow path in
/// `byroredux/src/render.rs:216`.
pub const MAX_TOTAL_BONES: usize = 32768;

/// Slot 0 of the bone palette is always the identity matrix.
pub const IDENTITY_BONE_SLOT: u32 = 0;

/// Maximum instances per frame — `0x40000` (262144). Sized to
/// absorb the densest observed Skyrim/FO4 city cells (Solitude,
/// Whiterun draw distance, Diamond City — ~50K REFRs combined with
/// landscape + clutter) with ~5× headroom.
///
/// `triangle.frag:980` writes `(instance_index + 1) & 0x7FFFFFFFu`
/// into the low 31 bits of the mesh_id attachment, reserving bit
/// 31 (`0x80000000`) for the `ALPHA_BLEND_NO_HISTORY` flag consumed
/// by TAA and SVGF disocclusion. The encoding ceiling is therefore
/// `0x7FFFFFFF` (~2.1G) — `MAX_INSTANCES` is set well below that
/// to bound the persistent SSBO allocation, not the encoding.
/// The `gpu_instances.len() <= MAX_INSTANCES` debug_assert in
/// `vulkan::context::draw::draw_frame` enforces this contract; the
/// `max_instances_stays_within_mesh_id_encoding_ceiling` test
/// (`scene_buffer.rs`) pins the encoding ceiling separately so a
/// future `MAX_INSTANCES` bump past `0x7FFFFFFF` would force a
/// follow-up format flip just like the pre-#992 `R16_UINT` → `R32_UINT`
/// step.
///
/// `262144 × sizeof(GpuInstance) = 262144 × 112 B = 29.4 MB / frame
/// × 2 frames-in-flight = 58.8 MB` total — within the 6 GB RT-minimum
/// VRAM budget.
///
/// **History**:
///  - 8192 (original sizing for pre-M41 cells).
///  - 16384 (commit `fbba53e`, May 2026): live FNV cell render
///    showed transparent-draw flicker — fire / waterfall / particle
///    billboards popping in/out with the slightest camera move.
///    Depth-sorted tail spilled past 8192; sub-pixel camera moves
///    shuffled which entries fell off the cliff.
///  - 32767 / `0x7FFF` (commit `67da5af`): live Skyrim Markarth
///    render exceeded 16384. 32767 was the maximum the `R16_UINT`
///    mesh_id path could address without wrap-collapsing to the
///    sky sentinel.
///  - 262144 / `0x40000` (#992 / REN-MESH-ID-32): flip
///    `MESH_ID_FORMAT` to `R32_UINT` + shader-side bit 15 → bit 31
///    rework. Dense Skyrim/FO4 city cells (Solitude, Whiterun
///    draw, Diamond City — ~50K REFRs) saturated the R16 ceiling
///    and silently wrap-collapsed.
pub const MAX_INSTANCES: usize = 0x40000;

/// Maximum number of `VkDrawIndexedIndirectCommand` entries held in
/// the per-frame indirect buffer. Each entry is 20 bytes, so
/// `MAX_INSTANCES × 20 B ≈ 5.2 MB per frame × 2 frames-in-flight
/// ≈ 10.5 MB` total. Sized identically to `MAX_INSTANCES` (one
/// expression, one variable to bump) so the worst-case 1:1 mapping
/// (no per-mesh batching folds, every instance is its own indirect
/// draw) still fits — the comment used to say "must agree with
/// MAX_INSTANCES" but the two were independent literals that could
/// drift on the next bump. Real scenes with the instanced batching
/// from #272 emit a few hundred entries; the cap exists to bound
/// buffer allocation, not to throttle typical use. See #309 / #992.
pub const MAX_INDIRECT_DRAWS: usize = MAX_INSTANCES;

/// Maximum number of `GpuTerrainTile` slots held in the per-frame
/// terrain-tile SSBO. 1024 × 32 B = 32 KB per frame — one slot per
/// terrain-mesh entity. A 3×3 loaded-cell grid emits 9 tiles; larger
/// exterior loads stay well under the cap. Capped at 65535 by the
/// 16-bit index packed into `GpuInstance.flags` (bits 16..31). See #470.
pub const MAX_TERRAIN_TILES: usize = 1024;

/// Maximum number of unique materials per frame in the
/// [`super::super::material::MaterialTable`] SSBO. 4096 × 260 B = 1.04 MB
/// per frame × 3 frames-in-flight = 3.3 MB total — trivial.
///
/// Real interior cells dedup to 50–200 unique materials; a 3×3
/// exterior grid lands around 300–600. The cap is sized 6–10× over
/// the largest observed scene to absorb future content. See R1.
pub const MAX_MATERIALS: usize = 4096;

/// Per-frame stride for the shared ray-budget buffer (#683 / MEM-2-8).
/// Each frame's slot must start on a `minStorageBufferOffsetAlignment`
/// boundary; 256 covers every common desktop / mobile GPU
/// (NVIDIA = 16, AMD = 4, Intel = 16, mobile up to 256). The actual
/// payload is 4 bytes — the rest is alignment padding. Total buffer
/// at MAX_FRAMES_IN_FLIGHT = 2 is 512 bytes.
pub const RAY_BUDGET_STRIDE: vk::DeviceSize = 256;

/// Per-instance flag bits on [`GpuInstance::flags`].
/// Kept in lockstep with the inline comments in `draw.rs` flag assembly
/// and with the fragment shader's `flags & N` checks.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
/// Terrain splat bit — tells the fragment shader to consume the
/// per-vertex splat weights (locations 6/7) and sample the 8 layer
/// textures indexed by `GpuTerrainTile` at the tile index packed into
/// the top 16 bits of `flags`. See #470.
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
/// Bit offset for the terrain tile index inside `GpuInstance.flags`.
/// `(flags >> INSTANCE_TERRAIN_TILE_SHIFT) & 0xFFFF` yields the tile slot.
pub const INSTANCE_TERRAIN_TILE_SHIFT: u32 = 16;
pub const INSTANCE_TERRAIN_TILE_MASK: u32 = 0xFFFF;
/// Bit offset for the [`RenderLayer`](byroredux_core::ecs::components::RenderLayer)
/// classification inside `GpuInstance.flags`. Layer is a 2-bit value
/// (Architecture / Clutter / Actor / Decal); bits 4..5 are unused by
/// any other flag, so packing here is collision-free.
/// `(flags >> INSTANCE_RENDER_LAYER_SHIFT) & 0x3u` yields the
/// [`RenderLayer`] discriminant. Consumed by the fragment shader's
/// debug-viz branch (`DBG_VIZ_RENDER_LAYER = 0x40`).
pub const INSTANCE_RENDER_LAYER_SHIFT: u32 = 4;
pub const INSTANCE_RENDER_LAYER_MASK: u32 = 0x3;

/// **Reserved** — bit set by the M29 GPU-skinning compute pass on
/// instances whose vertex data lives at the pre-skinned offset
/// (driven by `skin_compute`) instead of the source mesh's authored
/// position. Phase 2 of skinning (RT side only) reads the per-frame
/// pre-skinned vertex slice via `bone_offset` indirection on the
/// hit path; Phase 3 (rasteriser) will gate the vertex-fetch on
/// this bit so the inline-skinning + compute-pre-skin paths can
/// coexist on the same mesh without one silently shadowing the
/// other.
///
/// Reserved here rather than at Phase 3 landing so the bit number
/// is stable across the intervening commits — content authoring
/// tools / debug overlays that already grew a `flags & 0x40` check
/// don't end up at a different bit after the reservation lands.
/// No production reader today; the flag is written as zero on
/// every draw command. See REN-D12-NEW-05 (audit 2026-05-09).
pub const INSTANCE_FLAG_PRESKINNED: u32 = 1 << 6;

/// Engine-synthesized material kinds for [`GpuInstance::material_kind`].
///
/// The low range (0..=19) is reserved for Skyrim+
/// `BSLightingShaderProperty.shader_type` values the NIF importer
/// forwards verbatim — `SkinTint`, `HairTint`, `EyeEnvmap`, etc.
/// (see #344). The high range (100..) is reserved for kinds the
/// engine classifies itself from heuristics against the NIF material.
///
/// `Glass` is the first such kind (#Tier C Phase 2): alpha-blend
/// material, metalness < 0.3, not a decal. The fragment shader branches
/// on this value to dispatch the RT reflection + refraction path —
/// replaces the pre-Phase-2 per-pixel `texColor.a` heuristic that
/// flickered across textures. Callers (`render.rs`) must compute the
/// kind BEFORE populating `DrawCommand.material_kind`.
pub const MATERIAL_KIND_GLASS: u32 = 100;

/// `EffectShader` (`#706` / FX-1): Skyrim+ `BSEffectShaderProperty`
/// surface — fire flames, magic auras, glow rings, force fields, dust
/// planes, decals over emissive cones. The fragment shader branches on
/// this value to short-circuit lit shading: no scene point/spot lights,
/// no ambient, no GI bounce reads — output is `emissive_color *
/// emissive_mult * texColor.rgba`. Without this branch, fires get
/// modulated by every nearby lantern + ambient term + RT GI bounce,
/// producing rainbow-tinted flames where Bethesda authored a pure
/// orange/yellow additive surface.
///
/// Callers (`render.rs`) override the base shader_type-derived kind
/// to this value when `Material.effect_shader.is_some()`. Pre-existing
/// effect-shader data (falloff cone, greyscale palette, lighting_influence)
/// captured via #345 rides through on the same instance — the variant
/// branch in the fragment shader is the missing renderer-side dispatch
/// (SK-D3-02 follow-up).
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;
