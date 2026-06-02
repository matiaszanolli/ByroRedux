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
/// 196608 × 64 B = 12 MB/frame × 3 frames-in-flight = 36 MB total.
/// Slot 0 is a reserved identity fallback (used by rigid vertices
/// through the sum-of-weights escape hatch and by `SkinnedMesh` bones
/// that failed to resolve). The remaining slots are assigned
/// sequentially per skinned mesh, with each mesh consuming
/// `MAX_BONES_PER_MESH` (144) slots for simplicity. That gives 1364
/// skinned-mesh slots per frame. The compute shader has an early-out
/// for unused slots (see `skin_palette.comp:33-34,68-76`) so unused
/// tail headroom is free at dispatch time; only the SSBO bytes are
/// paid up-front, and 36 MB is < 1 % of the 6 GB VRAM target.
///
/// History of bumps (each driven by a real-content overflow):
/// - Pre-M41.0: 4096 bones (~31 meshes). Suited the no-NPC-spawn
///   baseline. FNV Prospector with M41.0 Phase 1b NPC spawn exposed
///   "rendered the first ~4 actors then dropped the rest".
/// - M41.0 → #1284-step-1: 32768 bones / 226 slots. Covered ~36 NPCs
///   at 7 skinned meshes each plus rigid scene content. FNV
///   `FreesideAtomicWrangler` (Atomic Wrangler casino, densest NPC
///   interior in FNV — Garret twins + dealers + escorts + patrons)
///   exposed 260 distinct skinned entities; 34 spilled and rendered
///   in bind pose with no RT shadows.
/// - #1284-step-1: 49152 bones / 340 slots. Picked from the static
///   estimate of ~260 entities × 1.3 headroom. Subsequently
///   under-shot the actual observed demand once the bone-palette
///   bottleneck cleared.
/// - #1284-step-2 (current): 196608 bones / 1364 slots. Sized from
///   the instrumented `overflow_attempts` counter (added to
///   `DebugStats` in the same change), which surfaced ~1040 over-cap
///   `allocate()` calls at Atomic Wrangler peak — far higher than the
///   static NPC × sub-mesh estimate suggested. NB: that counter is a
///   monotonic per-*call* spill count, not a per-frame distinct-entity
///   headcount (a stuck over-cap entity re-counts every frame, #1296),
///   so ~1040 is an upper bound on demand. 1364 sits comfortably above
///   it and gives ~4× headroom over the static estimate, covering Skyrim
///   Whiterun-Dragonsreach (5 885 entities) and FO4 Diamond City Market
///   without re-bumping.
///
/// The proper structural fix is variable-stride packing (M29.5 —
/// pack `bind_inverses` by actual bone count rather than reserving
/// `MAX_BONES_PER_MESH` slack per slot). This constant just buys
/// headroom until then. See `bone_palette` overflow path in
/// `byroredux/src/render/skinned.rs`.
pub const MAX_TOTAL_BONES: usize = 196608;

/// Slot 0 of the bone palette is always the identity matrix.
pub const IDENTITY_BONE_SLOT: u32 = 0;

/// M29.6 — maximum number of `bind_inverses` first-sight uploads
/// scheduled per frame. The HOST_VISIBLE staging buffer for these
/// uploads is sized for this many concurrent slots; if more skinned
/// entities first-appear in a single frame, the renderer caps the
/// uploads at this count and defers the excess to the next frame.
///
/// Pre-#1198 this was 16 (matching the typical heavy-cell-load count).
/// Bumped to 227 (= `MAX_TOTAL_BONES (32768) / MAX_BONES_PER_MESH (144)`)
/// to match the then slot-pool capacity. The pre-fix cap produced a
/// one-frame bind-pose glitch when more than 16 skinned NPCs
/// first-sighted in a single frame (FO4 MedTek: 23 SkinnedMesh
/// entities; FO3 Megaton REFR spill on first entry). Per #1191's
/// identity-fallback contract the deferred entities rendered in bind
/// pose for one frame, then snapped to skinned pose on frame N+1.
///
/// #1284 follow-up: re-bumped to 1366 (= `196608 / 144`) so the
/// per-frame upload cap continues to match the slot-pool capacity
/// after the `MAX_TOTAL_BONES` bump. FNV `FreesideAtomicWrangler` is
/// the densest first-sight workload (~1040 over-cap `allocate()` calls
/// at Atomic Wrangler peak per the
/// `DebugStats::skin_pool_overflow_attempts` counter added in the same
/// change — a monotonic per-call spill count, an upper bound on demand
/// rather than a per-frame distinct-entity headcount; see #1296).
///
/// Staging buffer size at this value is
/// `1366 × MAX_BONES_PER_MESH (144) × 64 B ≈ 12 MB` — < 1 % of the
/// 6 GB VRAM target. With the bump the per-frame upload cap matches
/// the slot pool's actual capacity, eliminating the one-frame glitch.
pub const MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME: usize = 1366;

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
/// Compile-time guard: `instance_custom_index` in the TLAS instance struct
/// is a 24-bit field (`Packed24_8`).  If `MAX_INSTANCES` is ever bumped past
/// 2^24 the TLAS build will silently truncate SSBO indices and corrupt every
/// RT material/transform lookup.  Fail the build here so the bump author
/// knows to partition the TLAS first.  See tlas.rs + #957 / #1392.
const _: () = assert!(
    MAX_INSTANCES < (1 << 24),
    "MAX_INSTANCES exceeds the 24-bit instance_custom_index ceiling; \
     partition the TLAS or widen the encoding before raising this constant"
);

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
/// [`super::super::material::MaterialTable`] SSBO. 16384 × 304 B = 4.75 MB
/// per frame × MAX_FRAMES_IN_FLIGHT (2) = 9.5 MB total — well inside the
/// 4 GB total VRAM budget (`feedback_vram_baseline.md`).
///
/// Observed unique-material counts (post-Disney-PBR; #1248-#1251 added
/// ior / subsurface / sheen / sheen_tint / anisotropic, each a fresh
/// dedup-distinguishing axis):
/// * FNV / FO3 interior cell — 50-200
/// * FNV / FO3 3×3 exterior grid — 300-600
/// * Skyrim radius-3 (7×7 = 49 cells) Riverwood — 4000+ (exceeded the
///   prior 4096 cap; SAFE-22 / #797 cap-and-warn fired)
///
/// 16384 absorbs Skyrim radius-5 + DLC content + future Starfield/FO76
/// per-segment SubIndex materials with comfortable headroom. The cap-
/// and-warn safety net at `material.rs::MaterialTable::intern_by_hash`
/// still triggers on overflow (over-cap entries share material 0); the
/// overflow counter on the table surfaces how badly we're over.
pub const MAX_MATERIALS: usize = 16384;

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

/// `NiShadeProperty.flags == 0` flat-shading bit (#869). When set,
/// the fragment shader replaces the interpolated vertex normal with
/// the per-face derivative
/// `normalize(cross(dFdx(world_pos), dFdy(world_pos)))` so the mesh
/// reads as faceted. Off by default; set on a handful of Oblivion
/// architectural pieces that author NiShadeProperty.
///
/// Bit 7 sits between PRESKINNED (bit 6) and the render-layer slot
/// (bits 4..5 via [`INSTANCE_RENDER_LAYER_SHIFT`]) so no other reader
/// collides. Lives below the terrain-tile-index window (bits 16..31).
pub const INSTANCE_FLAG_FLAT_SHADING: u32 = 1 << 7;

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

/// `NoLighting` (FO3/FNV `BSShaderNoLightingProperty`): fullbright /
/// unlit surface — terminal screens, computer text, neon/sign faces,
/// HUD/scope overlays, blood-splat decals. In the original engine the
/// "no lighting" shader outputs the texture (× per-vertex color)
/// **directly**: no scene point/spot/directional lights, no ambient,
/// no GI bounce, no camera-distance term. The fragment shader branches
/// on this value to emit `texColor.rgb * vertexColor` and return.
///
/// Distinct from [`MATERIAL_KIND_EFFECT_SHADER`]: that path forces
/// additive transparency (z-write off) for glows/dust planes, whereas
/// NoLighting **preserves the authored blend / depth state** — most
/// NoLighting surfaces (terminal screens, signs) are opaque and must
/// write depth. Pre-fix these surfaces went through the full lit path
/// (`material_kind = 0`) and so picked up scene lighting plus a
/// camera-distance-dependent GI term that faded at the rtLOD tier —
/// the user-reported "self-illumination dims with distance" (2026-05-27).
///
/// Set by the NIF importer's `BSShaderNoLightingProperty` branch when
/// the mesh isn't already an engine-synthesized kind.
pub const MATERIAL_KIND_NO_LIGHTING: u32 = 102;

#[cfg(test)]
mod tests {
    use super::*;

    /// #869 — every `INSTANCE_FLAG_*` bit must be distinct and must
    /// not collide with the render-layer slot (bits 4..5) or the
    /// terrain-tile-index window (bits 16..31). If a future flag
    /// reuses a populated bit, both the CPU packing site
    /// (`context/draw.rs`) and the shader-side `& N` test would
    /// silently merge two unrelated meanings.
    #[test]
    fn instance_flag_bits_unique_and_outside_packed_windows() {
        let layer_window: u32 = INSTANCE_RENDER_LAYER_MASK << INSTANCE_RENDER_LAYER_SHIFT;
        let tile_window: u32 = INSTANCE_TERRAIN_TILE_MASK << INSTANCE_TERRAIN_TILE_SHIFT;
        let flags: &[(&str, u32)] = &[
            ("NON_UNIFORM_SCALE", INSTANCE_FLAG_NON_UNIFORM_SCALE),
            ("ALPHA_BLEND", INSTANCE_FLAG_ALPHA_BLEND),
            ("CAUSTIC_SOURCE", INSTANCE_FLAG_CAUSTIC_SOURCE),
            ("TERRAIN_SPLAT", INSTANCE_FLAG_TERRAIN_SPLAT),
            ("PRESKINNED", INSTANCE_FLAG_PRESKINNED),
            ("FLAT_SHADING", INSTANCE_FLAG_FLAT_SHADING),
        ];
        for (i, (a_name, a)) in flags.iter().enumerate() {
            // Each flag is a single bit.
            assert_eq!(a.count_ones(), 1, "{a_name} is not a single bit: {a:#b}");
            // No flag falls inside a packed-value window.
            assert_eq!(
                a & layer_window,
                0,
                "{a_name} ({a:#b}) collides with render-layer window {layer_window:#b}"
            );
            assert_eq!(
                a & tile_window,
                0,
                "{a_name} ({a:#b}) collides with terrain-tile window {tile_window:#b}"
            );
            // No two flags share a bit.
            for (b_name, b) in flags.iter().skip(i + 1) {
                assert_eq!(
                    a & b,
                    0,
                    "{a_name} ({a:#b}) and {b_name} ({b:#b}) share a bit"
                );
            }
        }
    }

    /// #869 / #1190 — `triangle.frag` tests the flat-shading bit via the
    /// generated `#define INSTANCE_FLAG_FLAT_SHADING 128u` (emitted from
    /// this constant into `shader_constants.glsl`; the bare `& 128u` literal
    /// the shader used pre-#1190 is now forbidden by
    /// `triangle_shaders_use_named_instance_flag_constants`). This pin keeps
    /// the *value* at 128 (bit 7): if the Rust-side constant ever shifts, the
    /// generated #define — and every shader reading it — would silently move
    /// to the wrong bit.
    #[test]
    fn flat_shading_bit_pinned_at_128_for_shader_constant() {
        assert_eq!(
            INSTANCE_FLAG_FLAT_SHADING, 128,
            "INSTANCE_FLAG_FLAT_SHADING feeds the generated shader #define; \
             it must equal 128 (bit 7) or every shader reading it moves bits"
        );
    }
}
