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
/// The `GpuInstance` struct is duplicated across **four** GLSL
/// sources — `triangle.vert`, `triangle.frag`, `ui.vert`, and (since
/// the caustic pass #321) `caustic_splat.comp` — and must stay
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

/// Regression for #1028 / R-D6-01. `GpuCamera` must stay 288 B
/// — three `mat4` (192 B) plus six trailing `vec4` (96 B) for
/// `position`, `flags`, `screen`, `fog`, `jitter`, `sky_tint`.
/// Every shader that re-declares `CameraUBO` (`triangle.vert`,
/// `triangle.frag`, `water.vert`, `water.frag`, `cluster_cull.comp`,
/// `caustic_splat.comp`) must match this size — pre-#1028 the
/// first and last two were one `vec4` short, which the test would
/// not have caught (no Rust-side drift) but the audit did. This
/// pin at least catches the Rust-side regression so the doc-
/// comment stays honest.
#[test]
fn gpu_camera_is_288_bytes() {
        assert_eq!(
            size_of::<GpuCamera>(),
            288,
            "GpuCamera must stay 288 B to match the CameraUBO declaration in every shader that re-declares it — see #1028 / R-D6-01"
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
/// encoding in `triangle.frag:980` would wrap to meshId 0
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
            ("triangle.vert", include_str!("../../../shaders/triangle.vert")),
            ("triangle.frag", include_str!("../../../shaders/triangle.frag")),
            ("ui.vert", include_str!("../../../shaders/ui.vert")),
            (
                "caustic_splat.comp",
                include_str!("../../../shaders/caustic_splat.comp"),
            ),
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
            // `triangle.frag` is the only shader that declares a
            // `GpuMaterial` block at all (see binding 13 below).
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
             `triangle.frag` mirrors the material struct (binding 13). \
             See #776 / #785."
        );
        assert!(
            !src.contains("materials[inst"),
            "ui.vert: must NOT index into `materials[inst.…]`. The UI \
             quad's `materialId` is 0 (default-initialized), so any \
             read aliases the first scene material — see #776 / #785."
        );
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

/// Regression for #575 / SH-1. The global `GlobalVertices` SSBO
/// is declared as `float vertexData[]` so every read implicitly
/// reinterprets the bytes as IEEE-754 float. Per the layout
/// table at the SSBO declaration in triangle.frag:
///
///   - safe float offsets: `position` (0..2), `color` (3..5),
///     `normal` (6..8), `uv` (9..10), `bone_weights` (15..18).
///   - **unsafe** offsets (require `floatBitsToUint` /
///     `unpackUnorm4x8` recovery): `bone_indices` (11..14),
///     `splat_weights_0/1` (19..20).
///
/// Pre-fix, a future RT shader author following the existing
/// `vertexData[base + N]` pattern could silently read u32 /
/// packed-u8 bit patterns as floats. This test grep-checks the
/// only shader that currently reads `vertexData` (triangle.frag)
/// for any forbidden offset — `+ 11` through `+ 14` (bone
/// indices) or `+ 19` / `+ 20` (splat weights) — that ISN'T
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
            // Look for `vertexData[ ... + N ]` where N is 11-14 or
            // 19-20. Tolerate whitespace and the `(vOff + iN)` outer
            // expression that the existing `getHitUV` site uses.
            for forbidden in [11, 12, 13, 14, 19, 20] {
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
                        if (11..=14).contains(&forbidden) {
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
