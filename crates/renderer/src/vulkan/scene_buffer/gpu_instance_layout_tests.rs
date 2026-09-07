//! `GpuInstance` byte-layout pinning tests.
//!
//! Reflection-based checks that the shader's `GpuInstance` struct matches the
//! host `#[repr(C)]` member offsets and that every shader naming the type
//! stays in lockstep.
//!
//! The shader-*source* contract tests that had accumulated here — which assert
//! GLSL semantics rather than struct layout — moved to [`super::shader_contract_tests`]
//! under #2977.

use super::shader_contract_tests::parse_rust_struct_fields;
use super::*;
use ash::vk;
use std::mem::offset_of;
use std::mem::size_of;

/// Regression guard for the Shader Struct Sync invariant (#318 / #417).
/// The `GpuInstance` struct is duplicated across **five** GLSL
/// sources — `include/bindings.glsl` (the shared copy `triangle.frag`
/// `#include`s since #1583/#1590), `triangle.vert`, `ui.vert`, (since
/// the caustic pass #321) `caustic_splat.comp`, and (#1498)
/// `water.vert` — and must stay
/// byte-for-byte identical with the Rust definition. Any drift here
/// silently corrupts per-instance data on the GPU. Verified offsets
/// come from the explicit `// offset N` comments inside those shaders.
/// See the `feedback_shader_struct_sync` memory note for the lockstep
/// update protocol (grep for `struct GpuInstance` in the shaders tree
/// before touching this struct).
#[test]
fn gpu_instance_is_160_bytes_std430_compatible() {
    // R1 Phase 6 collapsed the per-material fields onto the
    // separate `MaterialTable` SSBO. What's left here is
    // strictly per-DRAW: model (64 B) + 4 mesh refs +
    // bone_offset + flags + material_id + avg_albedo (kept
    // for caustic compute reads off its own descriptor set)
    // packed into 7 vec4 slots = 112 B, plus one more vec4 slot
    // (8 B `skinned_vertex_address` + 8 B `_reserved`) added by
    // #2219 for skinned-instance RT hit-normal reconstruction = 128 B,
    // plus two more vec4 slots (`morph_delta_address` +
    // `morph_weight_address` + `morph_target_count` + `_reserved2`)
    // added by #3231 for GPU morph-target blending = 160 B.
    assert_eq!(
        size_of::<GpuInstance>(),
        160,
        "GpuInstance must stay 160 B to match std430 shader layout"
    );
}

/// Regression for #1028 / R-D6-01, updated for #markarth-precision.
/// `GpuCamera` must stay 368 B — three `mat4` (192 B) plus eleven
/// trailing 16-byte vectors (176 B): `position`, `flags`, `screen`, `fog`,
/// `jitter`, `sky_tint`, `sun_direction` (#1210 Phase A+B),
/// `dof_params`, `render_origin` (#markarth-precision), `render_debug`,
/// `exterior_sky_tint` (#3323).
/// Every shader that re-declares `CameraUBO` (`triangle.vert`,
/// `triangle.frag`, `water.vert`, `water.frag`, `cluster_cull.comp`,
/// `caustic_splat.comp`) must match this size — pre-#1028 some
/// were a `vec4` short, which this test would not have caught (no
/// Rust-side drift) but the audit did; the `.spv`-level check is the
/// `uniform_block_size_by_name` reflection pin in `reflect.rs`. This
/// pin at least catches the Rust-side regression so the doc-
/// comment stays honest. (#1492 — an earlier revision of this doc
/// named `ssao.comp`/`composite.frag` as CameraUBO readers; neither
/// declares `CameraUBO` — they use their own param blocks in
/// origin-relative space.)
#[test]
fn gpu_camera_is_368_bytes() {
    assert_eq!(
            size_of::<GpuCamera>(),
            368,
            "GpuCamera must be 368 B (352 B + 16 B exterior_sky_tint vec4, #3323) to match \
             the CameraUBO declaration in every re-declaring shader (triangle.vert, triangle.frag, \
             water.vert, water.frag, cluster_cull.comp, caustic_splat.comp — each pinned against the \
             shipped .spv by the reflect.rs uniform_block_size_by_name check, which also covers the \
             include-only consumers skin_vertices/ssao/volumetrics_inject). exterior_sky_tint is \
             APPENDED at the end; every re-declarer must carry the full field list through it."
        );
}

/// #3763 (SAFE-2026-08-30-D6-02) — `GpuLight` is hand-duplicated across
/// four GLSL sources (`include/bindings.glsl`, `cluster_cull.comp`,
/// `caustic_splat.comp`, `volumetrics_inject.comp`,
/// `gpu_light_glsl_copies_stay_in_lockstep` in `shader_contract_tests.rs`
/// pins those four against each other), but unlike `GpuInstance` (160 B)
/// and `GpuCamera` (368 B) had no `size_of` pin against the Rust struct at
/// all. Four `[f32; 4]` fields (`position_radius` / `color_type` /
/// `direction_angle` / `params`) = 64 B.
#[test]
fn gpu_light_is_64_bytes() {
    assert_eq!(
        size_of::<GpuLight>(),
        64,
        "GpuLight must stay 64 B to match its four GLSL mirrors' std430 layout"
    );
}

#[test]
fn selected_ray_probe_is_144_bytes_std430_compatible() {
    assert_eq!(size_of::<GpuSelectedRayProbe>(), 144);
    assert_eq!(offset_of!(GpuSelectedRayProbe, control), 0);
    assert_eq!(offset_of!(GpuSelectedRayProbe, ids), 16);
    assert_eq!(offset_of!(GpuSelectedRayProbe, origin_tmin), 32);
    assert_eq!(offset_of!(GpuSelectedRayProbe, direction_tmax), 48);
    assert_eq!(offset_of!(GpuSelectedRayProbe, hit_visibility), 64);
    assert_eq!(offset_of!(GpuSelectedRayProbe, light_position_radius), 80);
    assert_eq!(offset_of!(GpuSelectedRayProbe, light_params), 128);
}

#[test]
fn gpu_instance_field_offsets_match_shader_contract() {
    assert_eq!(offset_of!(GpuInstance, model), 0);
    assert_eq!(offset_of!(GpuInstance, texture_index), 64);
    assert_eq!(offset_of!(GpuInstance, bone_offset), 68);
    assert_eq!(offset_of!(GpuInstance, vertex_offset), 72);
    assert_eq!(offset_of!(GpuInstance, index_offset), 76);
    assert_eq!(offset_of!(GpuInstance, vertex_count), 80);
    assert_eq!(offset_of!(GpuInstance, flags), 84);
    assert_eq!(offset_of!(GpuInstance, material_id), 88);
    assert_eq!(offset_of!(GpuInstance, ior), 92);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_r), 96);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_g), 100);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_b), 104);
    assert_eq!(offset_of!(GpuInstance, surface_id), 108);
    assert_eq!(offset_of!(GpuInstance, skinned_vertex_address), 112);
    assert_eq!(offset_of!(GpuInstance, _reserved), 120);
    assert_eq!(offset_of!(GpuInstance, morph_delta_address), 128);
    assert_eq!(offset_of!(GpuInstance, morph_weight_address), 136);
    assert_eq!(offset_of!(GpuInstance, morph_target_count), 144);
    assert_eq!(offset_of!(GpuInstance, _reserved2a), 148);
    assert_eq!(offset_of!(GpuInstance, _reserved2b), 152);
    assert_eq!(offset_of!(GpuInstance, _reserved2c), 156);
}

/// R1 Phase 6 sentinel — the fields that USED to live on `GpuInstance`
/// and were collapsed onto the `MaterialTable` SSBO. If any of those names
/// grows back here, R1 is being undone.
///
/// #2433 (TD9-002) — this test used to build a `GpuInstance::default()` and
/// discard it. Since the struct derives `Default` that cannot fail under any
/// circumstance: it was permanently green regardless of the struct's shape,
/// and its comment deferred to "the list below", which was empty. (The stale
/// "112 B" the audit also flagged was already corrected by #2692.)
///
/// Deferring to the size guard was not sufficient either, which is why this
/// now asserts on names rather than being deleted as a duplicate:
/// `GpuInstance` carries two reserved regions today — 8 bytes of `_reserved`
/// at offset 120, and 12 bytes across `_reserved2a`/`_reserved2b`/
/// `_reserved2c` at offset 148 (#3231) — so per-material `u32` fields can be
/// reintroduced *into either padding region* with the struct still exactly
/// 160 B and every existing offset unchanged. The size and offset pins both
/// stay green through that; only a name check catches it.
#[test]
fn gpu_instance_does_not_re_expand_with_per_material_fields() {
    const GPU_TYPES_RS: &str = include_str!("gpu_types.rs");

    // Per-material state that lives on `GpuMaterial` (see
    // `vulkan/material.rs`) and must not reappear per-draw. Deliberately
    // excludes the three material-shaped fields `GpuInstance` keeps on
    // purpose: `texture_index` (per-draw base texture, and also a legitimate
    // `GpuMaterial` member), `ior`, and `avg_albedo_*` (read by the caustic
    // compute pass off its own descriptor set).
    const COLLAPSED_ONTO_MATERIAL_TABLE: &[&str] = &[
        "roughness",
        "metalness",
        "material_flags",
        "material_kind",
        "material_alpha",
        "emissive_mult",
        "emissive_r",
        "emissive_g",
        "emissive_b",
        "specular_strength",
        "specular_r",
        "specular_g",
        "specular_b",
        "diffuse_r",
        "diffuse_g",
        "diffuse_b",
        "ambient_r",
        "ambient_g",
        "ambient_b",
        "alpha_threshold",
        "alpha_test_func",
        "normal_map_index",
        "glow_map_index",
        "dark_map_index",
        "detail_map_index",
        "gloss_map_index",
        "parallax_map_index",
        "env_map_index",
        "env_mask_index",
        "parallax_height_scale",
        "parallax_max_passes",
        "uv_offset_u",
        "uv_offset_v",
        "uv_scale_u",
        "uv_scale_v",
    ];

    let fields = parse_rust_struct_fields(GPU_TYPES_RS, "pub struct GpuInstance");
    assert!(
        !fields.is_empty(),
        "failed to parse GpuInstance's fields — the guard below would be vacuous"
    );
    for banned in COLLAPSED_ONTO_MATERIAL_TABLE {
        assert!(
            !fields.iter().any(|f| f == banned),
            "`GpuInstance.{banned}` is per-MATERIAL state that R1 Phase 6 collapsed \
             onto the MaterialTable SSBO — reintroducing it per-draw undoes that \
             dedup (fields: {fields:?}). If this is deliberate, the material-table \
             rationale in `gpu_types.rs` needs rewriting first (#2433)."
        );
    }
}

/// Regression: #309 — `VkDrawIndexedIndirectCommand` is a Vulkan-
/// specified C struct that `cmd_draw_indexed_indirect` reads
/// directly from the device-side buffer. Its layout is part of
/// the Vulkan contract (20 bytes, five u32 fields in a fixed
/// order). Guard the size so a future `ash` upgrade that
/// accidentally renames / reorders fields breaks the test
/// instead of silently producing garbage draw params.
#[test]
fn draw_indexed_indirect_command_is_20_bytes() {
    assert_eq!(
        size_of::<vk::DrawIndexedIndirectCommand>(),
        20,
        "VkDrawIndexedIndirectCommand must be 20 bytes (5 × u32) per the Vulkan spec"
    );
}

/// Regression: #309 — `upload_indirect_draws` clamps at
/// `MAX_INDIRECT_DRAWS` so a future bug that produces an
/// unbounded batch list can't overflow the indirect buffer.
/// `0x40000 × 20 B = 5.2 MB` per frame; the allocation matches.
///
/// Cap history: 8192 → 16384 → 0x7FFF (32767) → 0x40000 (262144,
/// post-#992 `R32_UINT` mesh_id). See the `MAX_INSTANCES` doc
/// comment for the encoding rationale; the cap remains sized
/// to match `MAX_INSTANCES` so the worst-case 1:1 mapping fits.
#[test]
fn indirect_buffer_capacity_matches_max_draw_constant() {
    let bytes_per_command = size_of::<vk::DrawIndexedIndirectCommand>();
    assert_eq!(bytes_per_command, 20);
    assert_eq!(
        bytes_per_command * MAX_INDIRECT_DRAWS,
        20 * 0x40000,
        "MAX_INDIRECT_DRAWS × sizeof(VkDrawIndexedIndirectCommand) \
             must match the per-frame indirect buffer allocation"
    );
}

/// Regression: `MAX_INSTANCES` must stay at or below the
/// `R32_UINT` mesh_id encoding ceiling (`0x7FFFFFFF`, with bit
/// 31 reserved for the `ALPHA_BLEND_NO_HISTORY` flag). Past
/// that ceiling the `(instance_index + 1) & 0x7FFFFFFFu`
/// encoding in `triangle.frag:1532` would wrap to meshId 0
/// (the "sky / no instance" sentinel) and shadow / reflection /
/// SVGF disocclusion queries against that instance silently
/// route to the wrong target. The `draw.rs` debug_assert
/// catches the same drift at runtime in debug builds; this
/// test pins the contract at build time so a future
/// `MAX_INSTANCES` bump past the encoding ceiling can't
/// accidentally trip the wrap.
///
/// Pre-#992 this test pinned `MAX_INSTANCES <= 0x7FFF` — the
/// `R16_UINT` encoding ceiling. Dense Skyrim/FO4 city cells
/// (Solitude, Whiterun draw, Diamond City) saturated it, so
/// the format flipped to `R32_UINT` and the ceiling moved to
/// `0x7FFFFFFF`. `MAX_INSTANCES` itself sits at `0x40000`
/// (~262K), with ~5× headroom past the worst observed scene.
#[test]
fn max_instances_stays_within_mesh_id_encoding_ceiling() {
    const MESH_ID_ENCODING_CEILING: usize = 0x7FFF_FFFF;
    const {
        assert!(MAX_INSTANCES <= MESH_ID_ENCODING_CEILING);
    }
}

/// Regression: pin `MESH_ID_FORMAT` at `R32_UINT` so a future
/// "save VRAM by going back to R16_UINT" attempt fails loudly
/// — pre-#992 that's exactly what the format was, and dense
/// Skyrim/FO4 city cells silently wrap-collapsed at 32767
/// instances. The +4.15 MB / 1080p / frame cost of `R32_UINT`
/// is trivial on the 6 GB RT-minimum target; "savings" here
/// would re-introduce the silent ghosting / flicker regression.
#[test]
fn mesh_id_format_is_r32_uint() {
    assert_eq!(
        super::super::gbuffer::MESH_ID_FORMAT,
        ash::vk::Format::R32_UINT,
        "MESH_ID_FORMAT must stay R32_UINT — the R16_UINT \
             predecessor capped at 32767 distinct instances and \
             dense city cells silently wrap-collapsed to meshId 0 \
             (the sky sentinel), misrouting every shadow / \
             reflection / SVGF disocclusion query against the \
             wrapped instance. See #992 / REN-MESH-ID-32."
    );
}

// ── GpuTerrainTile layout + GLSL lockstep (#2463 / REN-D3-2026-08-07-01) ──

/// `GpuTerrainTile` is a `#[repr(C)]` struct uploaded to the set 1 /
/// binding 10 SSBO and hand-mirrored in `include/bindings.glsl`, exactly
/// like `GpuInstance`, `GpuMaterial`, `GpuLight` and `GpuFogVolume` — but
/// unlike all of those it had no size pin, no offset pin, and no GLSL
/// lockstep check. The only thing coupling the two declarations was a
/// comment.
///
/// The asymmetry is what makes drift silent: the SSBO is *sized* from the
/// Rust side (`buffers.rs`'s `size_of::<GpuTerrainTile>() *
/// MAX_TERRAIN_TILES`) and the upload memcpy uses the Rust stride, while
/// the shader indexes `terrainTiles[tileIdx]` with the GLSL stride. Adding
/// a fourth layer role on the Rust side alone (say `layer_glow_index:
/// [u32; 8]`, a natural next step for LAND splatting) makes the strides
/// 128 vs 96, and every tile from index 1 onward reads misaligned bindless
/// texture indices — garbage diffuse/normal/specular layers across every
/// exterior cell. No test fails, no validation layer fires: the byte count
/// is legal either way.
#[test]
fn gpu_terrain_tile_is_144_bytes() {
    assert_eq!(
        size_of::<GpuTerrainTile>(),
        144,
        "GpuTerrainTile must stay 144 B (3 × uint[8], then #4057's two vec4 \
         affinity rows and the vec2+float+float canopy tail) to match the \
         std430 `struct GpuTerrainTile` in include/bindings.glsl. The shipped \
         triangle.frag.spv carries `ArrayStride 144` for this type; changing \
         the Rust side alone silently misaligns every tile after index 0. \
         It was 96 B until #4057 added §12.5's terrain receiver."
    );
    // std430 rounds a `vec4` member to a 16-byte boundary. Both vec4 rows
    // land on multiples of 16 by construction (96, 112) and the struct's
    // total is a multiple of 16, so the array stride needs no tail padding
    // the Rust side would have to reproduce by hand.
    assert_eq!(size_of::<GpuTerrainTile>() % 16, 0);
    assert_eq!(align_of::<GpuTerrainTile>(), 4);
}

#[test]
fn gpu_terrain_tile_field_offsets_match_shader_contract() {
    // Matches `OpDecorate ... ArrayStride 144` + members at 0 / 32 / 64 /
    // 96 / 112 / 128 / 136 / 140 in the shipped triangle.frag.spv.
    assert_eq!(offset_of!(GpuTerrainTile, layer_diffuse_index), 0);
    assert_eq!(offset_of!(GpuTerrainTile, layer_normal_index), 32);
    assert_eq!(offset_of!(GpuTerrainTile, layer_specular_index), 64);
    // The two vec4 rows have to sit on their std430 16-byte boundaries, or
    // GLSL reads the affinity table shifted by one lane — which yields
    // plausible density numbers over the wrong layers, not a crash.
    assert_eq!(offset_of!(GpuTerrainTile, cover_affinity0), 96);
    assert_eq!(offset_of!(GpuTerrainTile, cover_affinity1), 112);
    assert_eq!(offset_of!(GpuTerrainTile, cell_origin_xz), 128);
    assert_eq!(offset_of!(GpuTerrainTile, water_y), 136);
    assert_eq!(offset_of!(GpuTerrainTile, canopy_height), 140);
}

/// The shipped SPIR-V is the contract's other half. A Rust-side layout change
/// that never reached a `glslangValidator` run would pass every assertion
/// above and still read the tile array at the old stride on the GPU.
#[test]
fn shipped_triangle_frag_spv_carries_the_terrain_tile_stride() {
    let spv = include_bytes!("../../../shaders/triangle.frag.spv");
    // ArrayStride is `OpDecorate <id> ArrayStride <n>`: opcode 71, decoration
    // 6. Scan the word stream for any array decorated with 144 rather than
    // parsing the module — the point is only that the new stride is present
    // and the old one is not still describing this array.
    let words: Vec<u32> = spv
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
        .collect();
    let mut strides = Vec::new();
    let mut i = 5; // skip the 5-word header
    while i < words.len() {
        let len = (words[i] >> 16) as usize;
        let op = words[i] & 0xFFFF;
        if len == 0 {
            break;
        }
        if op == 71 && len == 4 && words[i + 2] == 6 {
            strides.push(words[i + 3]);
        }
        i += len;
    }
    assert!(
        strides.contains(&144),
        "no ArrayStride 144 in the shipped triangle.frag.spv — recompile it \
         after changing GpuTerrainTile (#4057). Found: {strides:?}"
    );
}

/// #3990 — the byte view `upload_instances` and `hash_instance_slice` take is
/// only sound while the struct has no implicit padding, and the three `u64`s
/// added by #2219 / #3231 are the only way padding could enter it: they raise
/// the alignment to 8, so a `u32` landing immediately before one at an odd
/// 4-byte offset would make the compiler insert four uninitialised bytes.
///
/// `gpu_instance_is_160_bytes_std430_compatible` would catch the resulting size
/// change, but nothing would say *why* — and the failure that matters is not
/// the size, it is that the byte view now contains uninitialised bytes (UB in
/// the upload copy and in the dirty-gate hash, and garbage in whichever std430
/// lane the padding lands). This pins the positional argument the
/// `unsafe impl NoUninit for GpuInstance` rests on, so the argument breaks
/// rather than the comment going quietly stale the way the old prose did.
#[test]
fn the_u64_fields_cannot_introduce_implicit_padding() {
    for (name, offset) in [
        (
            "skinned_vertex_address",
            offset_of!(GpuInstance, skinned_vertex_address),
        ),
        (
            "morph_delta_address",
            offset_of!(GpuInstance, morph_delta_address),
        ),
        (
            "morph_weight_address",
            offset_of!(GpuInstance, morph_weight_address),
        ),
    ] {
        assert_eq!(
            offset % 8,
            0,
            "{name} sits at offset {offset}, which is not 8-aligned — the \
             compiler has inserted implicit padding before it (#3990)"
        );
    }
    // The declared fields must tile the struct with nothing left over: the
    // last field ends exactly at the struct's size, and the size is a
    // multiple of the 8-byte alignment so the ARRAY stride needs no tail
    // padding either (the upload writes `count` contiguous instances).
    assert_eq!(
        offset_of!(GpuInstance, _reserved2c) + size_of::<u32>(),
        size_of::<GpuInstance>()
    );
    assert_eq!(align_of::<GpuInstance>(), 8);
    assert_eq!(size_of::<GpuInstance>() % align_of::<GpuInstance>(), 0);
}

/// The two most churn-prone GPU structs in the workspace must carry the
/// type-level no-uninit guard rather than a prose one, and their uploads must
/// go through the bound that enforces it (#3990 / #3761).
#[test]
fn the_churn_prone_gpu_structs_upload_through_the_nouninit_bound() {
    // Compile-time half: these calls do not typecheck without the impls.
    fn requires_no_uninit<T: crate::vulkan::buffer::NoUninit>() {}
    requires_no_uninit::<GpuInstance>();
    requires_no_uninit::<crate::vulkan::material::GpuMaterial>();

    // Call-site half: prose can be right and still leave the hazard one field
    // insertion away, which is what #3761 was added to remove.
    let src = include_str!("upload.rs");
    for call in [
        "self.instance_buffers[frame_index].write_mapped(device, &instances[..count])?;",
        "self.material_buffers[frame_index].write_mapped(device, &materials[..count])?;",
    ] {
        assert!(
            src.contains(call),
            "expected `{call}` — a hand-rolled copy_nonoverlapping here would \
             argue the no-uninit invariant in a comment again (#3990)"
        );
    }
}
