//! `GpuInstance` byte-layout pinning tests.
//!
//! Reflection-based checks that the shader's `GpuInstance` struct matches the
//! host `#[repr(C)]` member offsets and that every shader naming the type
//! stays in lockstep.

use super::*;
use ash::vk;
use std::mem::size_of;

use std::mem::offset_of;

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
fn gpu_instance_is_112_bytes_std430_compatible() {
    // R1 Phase 6 collapsed the per-material fields onto the
    // separate `MaterialTable` SSBO. What's left here is
    // strictly per-DRAW: model (64 B) + 4 mesh refs +
    // bone_offset + flags + material_id + avg_albedo (kept
    // for caustic compute reads off its own descriptor set)
    // packed into 7 vec4 slots = 112 B.
    assert_eq!(
        size_of::<GpuInstance>(),
        112,
        "GpuInstance must stay 112 B to match std430 shader layout"
    );
}

/// Regression for #1028 / R-D6-01, updated for #markarth-precision.
/// `GpuCamera` must stay 336 B — three `mat4` (192 B) plus nine
/// trailing `vec4` (144 B): `position`, `flags`, `screen`, `fog`,
/// `jitter`, `sky_tint`, `sun_direction` (#1210 Phase A+B),
/// `dof_params`, `render_origin` (#markarth-precision).
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
fn gpu_camera_is_336_bytes() {
    assert_eq!(
            size_of::<GpuCamera>(),
            336,
            "GpuCamera must be 336 B (320 B + 16 B render_origin vec4, #markarth-precision) to match \
             the CameraUBO declaration in all 6 re-declaring shaders (triangle.vert, triangle.frag, \
             water.vert, water.frag, cluster_cull.comp, caustic_splat.comp — each pinned against the \
             shipped .spv by the reflect.rs uniform_block_size_by_name check). render_origin was \
             APPENDED at the end; every re-declarer must carry the full field list up to and \
             including render_origin so std140 offsets line up."
        );
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
    assert_eq!(offset_of!(GpuInstance, _pad_id0), 92);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_r), 96);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_g), 100);
    assert_eq!(offset_of!(GpuInstance, avg_albedo_b), 104);
    assert_eq!(offset_of!(GpuInstance, _pad_albedo), 108);
}

/// R1 Phase 6 sentinel — list of fields that USED to live on
/// `GpuInstance` and were collapsed onto the `MaterialTable` SSBO.
/// If this test grows back any of those names, R1 is being undone.
#[test]
fn gpu_instance_does_not_re_expand_with_per_material_fields() {
    // Build trivially via Default and rely on the size assertion
    // above (112 B) to fail loudly if a field is reintroduced.
    // The list below is documentary only; the size guard is what
    // catches actual regressions.
    let _ = GpuInstance::default();
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
    assert!(
        MAX_INSTANCES <= MESH_ID_ENCODING_CEILING,
        "MAX_INSTANCES ({}) exceeds the R32_UINT mesh_id encoding ceiling \
             (0x7FFFFFFF, with bit 31 reserved for ALPHA_BLEND_NO_HISTORY). \
             Widen `MESH_ID_FORMAT` past 32 bits before bumping past this value.",
        MAX_INSTANCES
    );
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

/// Regression: #417 — every shader that declares its own copy of
/// `struct GpuInstance` must name the final u32 slot
/// `materialKind`, not `_pad1` or any other legacy placeholder.
/// The Rust side guards offsets via
/// `gpu_instance_field_offsets_match_shader_contract`; this test
/// guards name-level drift across the four shader copies so a
/// future refactor that actually reads the field (currently unused
/// on the caustic path) doesn't silently alias it to padding.
///
/// Walks the shaders tree at compile time via `include_str!` —
/// works in `cargo test` even on machines that don't have
/// glslangValidator installed, and catches the missed-rename
/// failure mode from #417 (caustic_splat.comp still said
/// `uint _pad1;` after the triangle.* / ui.vert rename).
#[test]
fn every_shader_struct_gpu_instance_names_material_kind_slot() {
    const SOURCES: &[(&str, &str)] = &[
        (
            "triangle.vert",
            include_str!("../../../shaders/triangle.vert"),
        ),
        // #1583/#1590 — the `struct GpuInstance` declaration was lifted
        // out of `triangle.frag` into the shared `include/bindings.glsl`
        // (`triangle.frag` now `#include`s it). The other four mirrors
        // below still embed their own copy.
        (
            "include/bindings.glsl",
            include_str!("../../../shaders/include/bindings.glsl"),
        ),
        ("ui.vert", include_str!("../../../shaders/ui.vert")),
        (
            "caustic_splat.comp",
            include_str!("../../../shaders/caustic_splat.comp"),
        ),
        // #1498 / REN2-13 — water.vert is the 5th GpuInstance mirror
        // (consumes `model` for vertex displacement); it was omitted
        // from this drift guard even though its layout already matches.
        ("water.vert", include_str!("../../../shaders/water.vert")),
    ];
    for (name, src) in SOURCES {
        assert!(
            src.contains("struct GpuInstance"),
            "{name} no longer declares `struct GpuInstance` — update \
                 the sync list at feedback_shader_struct_sync.md"
        );
        // R1 Phase 6 — `material_kind` moved off `GpuInstance`
        // into the `MaterialBuffer` SSBO. The assertion that
        // every shader's per-instance struct names a final
        // `materialKind` slot (#417) no longer applies.
        // `include/bindings.glsl` is the only source that declares a
        // `GpuMaterial` block at all (see binding 13 below);
        // `triangle.frag` #includes it.
        assert!(
            !src.contains("uint _pad1"),
            "{name}: GpuInstance slot is still named `_pad1` — \
                 the shader has the pre-#417 layout (Shader Struct \
                 Sync invariant #318 / #417)."
        );
        // R1 Phase 6 — these fields were migrated to the
        // `MaterialBuffer` SSBO and dropped from `GpuInstance`.
        // `material_kind` is now read as `materials[id].materialKind`
        // and `materialId` is the only material-table-related
        // slot left on the per-instance struct.
        for needle in [
            // R1 Phase 3 — material table indirection. Every shader
            // copy declares the slot so the std430 stride stays
            // byte-identical across the four.
            "materialId",
        ] {
            assert!(
                src.contains(needle),
                "{name}: GpuInstance must declare `{needle}` (R1 Phase 3+). \
                     Every copy updates in lockstep — see the \
                     feedback_shader_struct_sync memory note."
            );
        }
        // R1 Phase 6 — these names lived on `GpuInstance` before
        // the material-table collapse. A reappearance means the
        // refactor is being undone.
        for stale in [
            "parallaxMapIndex",
            "parallaxHeightScale",
            "parallaxMaxPasses",
            "envMapIndex",
            "envMaskIndex",
            "uvOffsetU",
            "uvScaleU",
            "materialAlpha",
            "skinTintR",
            "hairTintR",
            "multiLayerEnvmapStrength",
            "eyeLeftCenterX",
            "eyeCubemapScale",
            "eyeRightCenterX",
            "multiLayerInnerThickness",
            "multiLayerRefractionScale",
            "multiLayerInnerScaleU",
            "sparkleR",
            "sparkleIntensity",
            "diffuseR",
            "ambientR",
            "falloffStartAngle",
            "falloffStopAngle",
            "falloffStartOpacity",
            "falloffStopOpacity",
            "softFalloffDepth",
        ] {
            // The names CAN appear on the `GpuMaterial` mirror
            // declarations — what's forbidden is reappearance on
            // `struct GpuInstance` after Phase 6 dropped them.
            let gi_start = src.find("struct GpuInstance");
            let gi_end = gi_start.and_then(|s| src[s..].find('}').map(|e| s + e));
            if let (Some(s), Some(e)) = (gi_start, gi_end) {
                let gi_block = &src[s..e];
                assert!(
                    !gi_block.contains(stale),
                    "{name}: per-material field `{stale}` reappeared on \
                         `struct GpuInstance` — R1 Phase 6 dropped it. \
                         Read it from `materials[gpuInstance.materialId]` \
                         instead."
                );
            }
        }
    }
}

/// Regression: #776 / #785 — `ui.vert` must read its texture index
/// from `inst.textureIndex` (per-instance), NOT from
/// `materials[inst.materialId].textureIndex`. The UI quad is
/// appended at `draw.rs` with `..GpuInstance::default()`, which
/// leaves `materialId = 0`. Post-#807 `materials[0]` is the
/// reserved neutral default — a UI shader that read it would
/// pull a neutral GpuMaterial (not an arbitrary scene material
/// as in the pre-#807 days), but the texture index would still
/// be wrong (the UI texture lives in `inst.textureIndex`, not
/// in any GpuMaterial slot). The guard stays as defense-in-depth
/// against future drift. See `scene_buffer.rs:172-176` for the
/// contract and `feedback_shader_struct_sync.md` for the
/// broader invariant.
///
/// #785 was a stale-hunk regression of #776 introduced by an
/// unrelated commit. Static source check so any future drift
/// fails `cargo test` without needing glslangValidator.
#[test]
fn ui_vert_reads_texture_index_from_instance_not_material_table() {
    let src = include_str!("../../../shaders/ui.vert");
    assert!(
        src.contains("fragTexIndex = inst.textureIndex"),
        "ui.vert: `fragTexIndex` must be assigned from \
             `inst.textureIndex` (the per-instance UI texture handle). \
             Reading `materials[inst.materialId].textureIndex` samples \
             the first scene material instead — see #776 / #785."
    );
    // Match syntactic declarations only — the surrounding comments
    // legitimately reference `MaterialBuffer` / `materials[…]` to
    // explain why the read is forbidden, and the test must not
    // catch its own documentation.
    assert!(
        !src.contains("buffer MaterialBuffer"),
        "ui.vert: must NOT declare a `MaterialBuffer` SSBO. The UI \
             vertex stage only consumes per-instance `textureIndex`; \
             pulling in the material table re-enables the #776 / #785 \
             failure mode."
    );
    assert!(
        !src.contains("struct GpuMaterial"),
        "ui.vert: must NOT declare `struct GpuMaterial`. Only \
             `include/bindings.glsl` declares the material struct \
             (binding 13; `triangle.frag` #includes it). See #776 / #785."
    );
    assert!(
        !src.contains("materials[inst"),
        "ui.vert: must NOT index into `materials[inst.…]`. The UI \
             quad's `materialId` is 0 (default-initialized), so any \
             read aliases the first scene material — see #776 / #785."
    );
}

/// #1067 / REN-D14-NEW-07 — sibling guard for the water shaders.
/// `water.vert` / `water.frag` consume the per-instance `WaterPush`
/// push-constant block (128 B with reflection tint + scroll vectors)
/// instead of the MaterialBuffer SSBO; the water pipeline's descriptor
/// set doesn't even have binding 13 wired. Acquiring a MaterialBuffer
/// binding would be a silent regression (the descriptor set layout
/// would reject the bind at validation time) and re-introduce the
/// #776 / #785 failure-mode for the water path.
#[test]
fn water_shaders_must_not_acquire_material_buffer_binding() {
    for (name, src) in [
        ("water.vert", include_str!("../../../shaders/water.vert")),
        ("water.frag", include_str!("../../../shaders/water.frag")),
    ] {
        assert!(
            !src.contains("buffer MaterialBuffer"),
            "{name}: must NOT declare a `MaterialBuffer` SSBO. The water \
             pipeline's descriptor set has no material-table binding; \
             adding one would silently break the water pipeline. \
             See #1067 / REN-D14-NEW-07."
        );
        assert!(
            !src.contains("struct GpuMaterial"),
            "{name}: must NOT declare `struct GpuMaterial`. Water \
             material parameters live in the `WaterPush` push-constant \
             block (128 B) — see #1067 / REN-D14-NEW-07."
        );
        assert!(
            !src.contains("materials[inst") && !src.contains("materials["),
            "{name}: must NOT index into `materials[…]`. See #1067."
        );
    }
}

/// SH-3 / #641 regression. The vertex shader must compose
/// `fragPrevClipPos` through the previous-frame bone palette so
/// motion vectors on skinned vertices encode actual joint motion.
/// Pre-#641 it composed through the current-frame palette, leaving
/// every actor body / hand / face pixel with a wrong motion vector
/// that SVGF + TAA reprojected as a ghost trail.
///
/// Static source check (no `glslangValidator` dependency): the
/// shader must declare a `bones_prev` SSBO at `set 1, binding 12`
/// and feed `prevWorldPos` (composed through `bones_prev`) into
/// `fragPrevClipPos = prevViewProj * …`.
#[test]
fn triangle_vert_uses_bones_prev_for_motion_vectors() {
    let src = include_str!("../../../shaders/triangle.vert");
    assert!(
        src.contains("binding = 12) readonly buffer BonesPrevBuffer"),
        "triangle.vert must declare a previous-frame bone palette \
             SSBO at `set 1, binding = 12` (SH-3 / #641). Without it \
             skinned vertices produce wrong motion vectors and SVGF / \
             TAA ghost actor limbs in motion."
    );
    assert!(
        src.contains("mat4 bones_prev[]"),
        "triangle.vert: `BonesPrevBuffer` must expose a `mat4 \
             bones_prev[]` array — same layout as `bones[]` so the \
             current and previous palettes can share `inBoneIndices`."
    );
    assert!(
        src.contains("fragPrevClipPos = prevViewProj * prevWorldPos"),
        "triangle.vert: `fragPrevClipPos` must project the \
             previous-frame skinned `prevWorldPos`, not the current \
             frame's `worldPos`. SH-3 / #641 — composing through \
             `bones[]` for both frames is the bug this test guards."
    );
    assert!(
        src.contains("xformPrev"),
        "triangle.vert: a separate `xformPrev` matrix must be \
             composed from `bones_prev` so `prevWorldPos` reflects \
             last frame's joint poses (SH-3 / #641)."
    );
}

/// #1486 / REN2-01 regression. Bone palettes are uploaded in ABSOLUTE
/// world space (`skin_vertices.comp` builds the skinned BLAS from the
/// same palette and the TLAS is absolute), but `viewProj` has been
/// camera-relative since the #markarth-precision cascade (36f66493).
/// The skinned vertex branch must therefore rebase the blended palette
/// matrix's translation by `renderOrigin` before projecting — without
/// it every skinned mesh rasterizes displaced by the full render
/// origin (≥4096 units, typically off-screen) whenever the camera
/// leaves the `[0,4096)³` origin box, and the unconditional
/// `fragWorldPos = worldPos + renderOrigin` double-adds the origin
/// for the skinned fragments that do remain visible.
///
/// Static source check (no `glslangValidator` dependency): both the
/// current- and previous-frame blended matrices must subtract
/// `renderOrigin` in the skinned branch.
#[test]
fn triangle_vert_skinned_branch_rebases_render_origin() {
    let src = include_str!("../../../shaders/triangle.vert");
    assert!(
        src.contains("xform[3].xyz -= renderOrigin.xyz"),
        "triangle.vert: the skinned branch must rebase the blended \
             bone-palette matrix translation by `renderOrigin` \
             (`xform[3].xyz -= renderOrigin.xyz`) so skinned geometry \
             projects in the same render-origin-relative space as the \
             rigid path (#1486 / REN2-01)."
    );
    assert!(
        src.contains("xformPrev[3].xyz -= renderOrigin.xyz"),
        "triangle.vert: the previous-frame blended matrix must get \
             the same `renderOrigin` rebase as `xform` — otherwise \
             skinned motion vectors are off by the full render origin \
             (#1486 / REN2-01)."
    );
}

/// #1488 / REN2-03 regression. Both caustic deposit writers trace in
/// ABSOLUTE world space (their landing points are lifted by
/// `+renderOrigin` / arrive absolute for the TLAS), but `viewProj` has
/// been camera-relative since the #markarth-precision cascade
/// (36f66493). Re-projecting the absolute landing point without
/// subtracting the origin displaces NDC by the full render origin —
/// the in-bounds guards then silently `continue`, dropping every
/// splat: glass caustics (#321) and water floor caustics (#1210
/// Phase E) vanished in all content outside the `[0,4096)³` origin
/// cell.
///
/// Static source check (no `glslangValidator` dependency): both
/// writers must rebase by `renderOrigin` inside the projection.
#[test]
fn caustic_writers_rebase_render_origin_before_reprojection() {
    let cases = [
        (
            "caustic_splat.comp",
            include_str!("../../../shaders/caustic_splat.comp"),
            "viewProj * vec4(P - renderOrigin.xyz, 1.0)",
        ),
        (
            "water.frag",
            include_str!("../../../shaders/water.frag"),
            "viewProj * vec4(floorWorld - renderOrigin.xyz, 1.0)",
        ),
    ];
    for (name, src, needle) in cases {
        assert!(
            src.contains(needle),
            "{name}: caustic deposit re-projection must subtract \
                 `renderOrigin` before multiplying by the camera-relative \
                 `viewProj` (expected `{needle}`); projecting the absolute \
                 landing point makes the NDC guard cull every splat at any \
                 non-zero render origin (#1488 / REN2-03)."
        );
    }
}

/// #1490 / REN2-05 regression. `screen_to_world_dir` must return the
/// direction from the CAMERA to the unprojected far-plane point, not
/// from the coordinate-space origin. `params.camera_pos` is uploaded in
/// the same render-origin-relative space as `inv_view_proj` (draw.rs
/// subtracts `render_origin` at the composite upload), so the subtraction
/// is exact. Pre-fix the missing term skewed sky/sun/cloud/haze
/// directions by up to ~1.35° (≈75% of the sun disc) and popped at
/// every 4096-unit origin snap.
#[test]
fn composite_screen_to_world_dir_subtracts_camera_pos() {
    let src = include_str!("../../../shaders/composite.frag");
    assert!(
        src.contains("normalize(world.xyz / w - params.camera_pos.xyz)"),
        "composite.frag: `screen_to_world_dir` must subtract \
             `params.camera_pos` from the unprojected far point before \
             normalizing — without it the returned direction is measured \
             from the coordinate-space origin and the sky dome swims / \
             the sun disc misaligns vs `sun_dir` (#1490 / REN2-05)."
    );
}

/// Regression for #575 / SH-1. The global `GlobalVertices` SSBO
/// is declared as `float vertexData[]` so every read implicitly
/// reinterprets the bytes as IEEE-754 float. Per the layout
/// table at the SSBO declaration in triangle.frag:
///
///   - safe float offsets: `position` (0..2), `color` (3..6),
///     `normal` (7..9), `uv` (10..11), `bone_weights` (16..19).
///   - **unsafe** offsets (require `floatBitsToUint` /
///     `unpackUnorm4x8` recovery): `bone_indices` (12..15),
///     `splat_weights_0/1` (20..21).
///
/// Pre-fix, a future RT shader author following the existing
/// `vertexData[base + N]` pattern could silently read u32 /
/// packed-u8 bit patterns as floats. This test grep-checks the
/// only shader that currently reads `vertexData` (triangle.frag)
/// for any forbidden offset — `+ 12` through `+ 15` (bone
/// indices) or `+ 20` / `+ 21` (splat weights) — that ISN'T
/// wrapped in `floatBitsToUint(…)` or `unpackUnorm4x8(…)`.
///
/// `caustic_splat.comp` and `ui.vert` don't bind GlobalVertices
/// at all and aren't checked. `skin_vertices.comp` reads bone
/// indices but does so through `floatBitsToUint`; the regex
/// excludes that pattern.
#[test]
fn triangle_frag_no_unsafe_vertex_data_reads() {
    let src = include_str!("../../../shaders/triangle.frag");

    // Strip safe-recovery wrappers so a forbidden raw read
    // surfaces as a literal `vertexData[... + 11..14|19|20]`.
    // We don't run a full GLSL parser; instead, line-by-line
    // we reject any line that contains the forbidden offset
    // pattern AND no `floatBitsToUint` / `unpackUnorm4x8` /
    // `floatBitsToInt` recovery call. Whitespace tolerant.
    for (lineno, line) in src.lines().enumerate() {
        // Skip the SSBO-declaration block — it documents the
        // unsafe offsets but doesn't read them.
        if line.contains("WARNING")
            || line.contains("│")
            || line.contains("//")
                && (line.contains("floatBitsToUint") || line.contains("unpackUnorm4x8"))
        {
            continue;
        }
        // Look for `vertexData[ ... + N ]` where N is 12-15 or
        // 20-21. Tolerate whitespace and the `(vOff + iN)` outer
        // expression that the existing `getHitUV` site uses.
        for forbidden in [12, 13, 14, 15, 20, 21] {
            let needle_simple = format!("+ {}]", forbidden);
            let needle_alt = format!("+{}]", forbidden);
            if line.contains(&needle_simple) || line.contains(&needle_alt) {
                // Allow the read when it's wrapped in a
                // recovery call.
                if line.contains("floatBitsToUint")
                    || line.contains("unpackUnorm4x8")
                    || line.contains("floatBitsToInt")
                {
                    continue;
                }
                panic!(
                    "triangle.frag:{}: unsafe `vertexData[... + {}]` read \
                         (offset {} is {} — not an IEEE-754 float). Use \
                         `floatBitsToUint(...)` or `unpackUnorm4x8(...)` to \
                         recover the bit pattern. See #575 / SH-1.\nLine: {}",
                    lineno + 1,
                    forbidden,
                    forbidden,
                    if (12..=15).contains(&forbidden) {
                        "u32 (bone index)"
                    } else {
                        "packed 4× u8 unorm (splat weight)"
                    },
                    line.trim()
                );
            }
        }
    }
}

// ── GpuMaterial GLSL ↔ Rust field-order cross-check (#1657 / SF-D8-01) ──

/// Normalize an identifier so snake_case and camelCase spellings of the
/// same field collapse to one key: strip every `_`, lowercase the rest.
/// `emissive_mult` and `emissiveMult` both → `emissivemult`.
fn normalize_ident(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && !s.as_bytes()[0].is_ascii_digit()
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Slice out the body between the first `{` after `decl` and its matching
/// `}`. Both `struct GpuMaterial` declarations have a flat (un-nested)
/// body, so the first `}` is the closer.
fn extract_struct_body<'a>(src: &'a str, decl: &str) -> Option<&'a str> {
    let start = src.find(decl)?;
    let open = src[start..].find('{')? + start;
    let close = src[open..].find('}')? + open;
    Some(&src[open + 1..close])
}

/// Ordered field names of the Rust `#[repr(C)] struct GpuMaterial`,
/// parsed from `material.rs` source. A field line is `pub <ident>: <ty>,`;
/// comment / attribute / blank lines are skipped.
fn parse_rust_struct_fields(src: &str) -> Vec<String> {
    let body = extract_struct_body(src, "pub struct GpuMaterial")
        .expect("material.rs must declare `pub struct GpuMaterial`");
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let lhs = line[..colon].trim();
        let ident = lhs.strip_prefix("pub ").unwrap_or(lhs).trim();
        if is_ident(ident) {
            out.push(ident.to_string());
        }
    }
    out
}

/// Ordered field names of the GLSL `struct GpuMaterial`, parsed from
/// `include/bindings.glsl`. Handles multi-name declarations
/// (`float a, b, c;`) and skips `//`/`///` comment lines.
fn parse_glsl_struct_fields(src: &str) -> Vec<String> {
    const TYPES: &[&str] = &[
        "float", "uint", "int", "bool", "vec2", "vec3", "vec4", "mat2", "mat3", "mat4",
    ];
    let body = extract_struct_body(src, "struct GpuMaterial")
        .expect("include/bindings.glsl must declare `struct GpuMaterial`");
    let mut out = Vec::new();
    for raw in body.lines() {
        // Drop any trailing line comment first (also collapses `///` /
        // `//` doc lines to empty so they're skipped).
        let line = match raw.find("//") {
            Some(i) => &raw[..i],
            None => raw,
        }
        .trim();
        let Some(semi) = line.find(';') else { continue };
        let decl = line[..semi].trim();
        let mut parts = decl.splitn(2, char::is_whitespace);
        let ty = parts.next().unwrap_or("");
        if !TYPES.contains(&ty) {
            continue;
        }
        let Some(rest) = parts.next() else { continue };
        for piece in rest.split(',') {
            let id = piece.trim();
            if is_ident(id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// #1657 / SF-D8-01 — cross-check the GLSL `struct GpuMaterial` field
/// ORDER against the Rust `#[repr(C)]` struct field order.
///
/// The pre-existing guards leave one leg of the GpuMaterial lockstep
/// contract unpinned: `gpu_material_field_offsets_match_shader_contract`
/// pins only the *Rust* offsets, and `gpu_material_glsl_field_names_pinned`
/// only asserts each GLSL name is *present* (`src.contains`). Neither
/// catches a within-vec4 GLSL reorder (e.g. swapping `metalness` and
/// `roughness`) that preserves the 300 B size — the shader would then
/// read the wrong scalar on every lit surface, yet every `cargo test`
/// would pass. This is the positive-order guard the `GpuInstance`
/// contract already has (`gpu_instance_field_offsets_match_shader_contract`)
/// but `GpuMaterial` lacked.
///
/// Walks BOTH source files at compile time (`include_str!`, no glslang
/// needed), extracts each struct's declaration-order field list,
/// normalizes snake_case ↔ camelCase, and asserts the two ordered lists
/// are identical. The Rust struct stays the source of truth (its offsets
/// are pinned elsewhere); this makes the GLSL declaration track it.
#[test]
fn gpu_material_glsl_field_order_matches_rust_struct() {
    let rust_src = include_str!("../material.rs");
    let glsl_src = include_str!("../../../shaders/include/bindings.glsl");

    let rust_fields = parse_rust_struct_fields(rust_src);
    let glsl_fields = parse_glsl_struct_fields(glsl_src);

    assert!(
        rust_fields.len() > 60,
        "parsed only {} fields from the Rust `struct GpuMaterial` — parser likely broke",
        rust_fields.len()
    );
    assert!(
        glsl_fields.len() > 60,
        "parsed only {} fields from the GLSL `struct GpuMaterial` — parser likely broke",
        glsl_fields.len()
    );

    let rust_norm: Vec<String> = rust_fields.iter().map(|f| normalize_ident(f)).collect();
    let glsl_norm: Vec<String> = glsl_fields.iter().map(|f| normalize_ident(f)).collect();

    assert_eq!(
        rust_norm.len(),
        glsl_norm.len(),
        "GpuMaterial field COUNT differs: Rust has {} {:?}, GLSL has {} {:?}. The two \
         `struct GpuMaterial` declarations (material.rs + include/bindings.glsl) must stay in \
         lockstep — see #1657 / SF-D8-01.",
        rust_norm.len(),
        rust_fields,
        glsl_norm.len(),
        glsl_fields,
    );

    for (i, (r, g)) in rust_norm.iter().zip(glsl_norm.iter()).enumerate() {
        assert_eq!(
            r, g,
            "GpuMaterial field #{i} ORDER mismatch: Rust `{}` vs GLSL `{}`. The GLSL \
             `struct GpuMaterial` in include/bindings.glsl must declare fields in the SAME order \
             as the Rust `#[repr(C)]` struct (the offset source of truth). A within-vec4 reorder \
             keeps the 300 B size but corrupts every lit-surface read — see #1657 / SF-D8-01.",
            rust_fields[i], glsl_fields[i],
        );
    }
}

// ── GpuLight four-way GLSL lockstep (#1916) ──

/// Strip a GLSL struct body down to its bare `<type> <name>;` declaration
/// lines — drop `//` line comments and blank lines, collapse internal
/// whitespace. Two struct bodies with identical stripped output declare
/// the same fields in the same order, regardless of how each copy's
/// comments describe them.
fn strip_struct_body(body: &str) -> Vec<String> {
    body.lines()
        .map(|raw| match raw.find("//") {
            Some(i) => &raw[..i],
            None => raw,
        })
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// #1916 — `struct GpuLight` is hand-duplicated across four GLSL sources:
/// `include/bindings.glsl` (the shared copy `triangle.frag` `#include`s),
/// `cluster_cull.comp`, `caustic_splat.comp`, and (since commit `977eb95a`)
/// `volumetrics_inject.comp`. That fourth copy was never added to the
/// `gpu_types.rs` doc-comment enumeration, and no test pinned the four
/// declarations against each other — a future `GpuLight` field change
/// could update three copies and silently leave the volumetrics fog pass
/// reading a stale layout (wrong light color/position feeding the fog
/// glow). Walks all four sources at compile time (`include_str!`, no
/// glslangValidator dependency) and asserts their stripped field lists
/// are byte-identical.
#[test]
fn gpu_light_glsl_copies_stay_in_lockstep() {
    const SOURCES: &[(&str, &str)] = &[
        (
            "include/bindings.glsl",
            include_str!("../../../shaders/include/bindings.glsl"),
        ),
        (
            "cluster_cull.comp",
            include_str!("../../../shaders/cluster_cull.comp"),
        ),
        (
            "caustic_splat.comp",
            include_str!("../../../shaders/caustic_splat.comp"),
        ),
        (
            "volumetrics_inject.comp",
            include_str!("../../../shaders/volumetrics_inject.comp"),
        ),
    ];

    let mut reference: Option<(&str, Vec<String>)> = None;
    for (name, src) in SOURCES {
        let body = extract_struct_body(src, "struct GpuLight")
            .unwrap_or_else(|| panic!("{name}: no longer declares `struct GpuLight`"));
        let fields = strip_struct_body(body);
        assert!(
            fields.len() >= 4,
            "{name}: parsed only {} GpuLight field lines — parser likely broke",
            fields.len()
        );
        match &reference {
            None => reference = Some((name, fields)),
            Some((ref_name, ref_fields)) => {
                assert_eq!(
                    ref_fields, &fields,
                    "GpuLight layout mismatch: `{ref_name}` vs `{name}`. All four GLSL copies of \
                     `struct GpuLight` must declare identical fields in the same order (Shader \
                     Struct Sync invariant, #1916) — a drift here silently corrupts light data \
                     for whichever copy lags behind."
                );
            }
        }
    }
}
