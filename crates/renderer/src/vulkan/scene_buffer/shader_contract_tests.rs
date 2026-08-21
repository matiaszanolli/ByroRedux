//! Shader-source contract tests.
//!
//! These read the GLSL sources with `include_str!` and assert *semantics*:
//! that debug views bypass non-transport frame-graph terms, that a zero GI
//! budget is a true no-ray floor, that normal alpha masks specular intensity
//! rather than roughness, and the GLSL↔Rust field-order cross-checks for
//! `GpuMaterial` / `GpuLight`.
//!
//! Split out of `gpu_instance_layout_tests.rs` under #2977 — they are not
//! layout tests, and sit beside the shader-constants suite
//! (`super::super::super::shader_constants`) they actually exercise.

use super::*;
use crate::shader_constants::{RESERVOIR_LIGHT_BITS, RESERVOIR_LIGHT_MASK, RESERVOIR_SURFACE_MASK};

/// The ReSTIR history packs a selected light into ten bits and uses the
/// all-ones value as an invalid sentinel. The upload cap must leave that value
/// unoccupied; otherwise real light 1023 and "no selection" alias in temporal
/// and spatial reuse.
#[test]
fn max_lights_leaves_the_packed_restir_sentinel_unoccupied() {
    assert_eq!(MAX_LIGHTS, RESERVOIR_LIGHT_MASK as usize);
    assert_eq!(RESERVOIR_LIGHT_MASK, (1u32 << RESERVOIR_LIGHT_BITS) - 1);
    assert_eq!(RESERVOIR_SURFACE_MASK, u32::MAX >> RESERVOIR_LIGHT_BITS);

    let header = include_str!("../../../shaders/include/shader_constants.glsl");
    let shader = include_str!("../../../shaders/triangle.frag");
    assert!(header.contains("#define MAX_LIGHTS 1023u"));
    assert!(header.contains("#define RESERVOIR_LIGHT_BITS 10u"));
    assert!(header.contains("#define RESERVOIR_LIGHT_MASK 1023u"));
    assert!(header.contains("#define RESERVOIR_SURFACE_MASK 4194303u"));
    assert!(!shader.contains("const uint RESERVOIR_LIGHT_MASK"));
    assert!(shader.contains(">> RESERVOIR_LIGHT_BITS"));
    assert!(shader.contains("<< RESERVOIR_LIGHT_BITS"));
    assert!(shader.contains("explicit 1023 invalid value"));
}

/// The categorical selected-light oracle distinguishes a legitimate absence
/// (black) from a corrupt non-sentinel index (magenta).
#[test]
fn selected_light_debug_marks_out_of_range_indices_magenta() {
    let shader = include_str!("../../../shaders/triangle.frag");
    assert!(shader.contains("selectedLightDebug >= lightCount"));
    assert!(shader.contains("selectedLightDebug != 0xFFFFFFFFu"));
    assert!(shader.contains("selectedColor = vec3(1.0, 0.0, 1.0);"));
}

/// The selected-ray diagnostic is bounded to one atomically-claimed record
/// and carries the exact geometry/mask/hit/light payload required to
/// correlate the selected-light and visibility views.
#[test]
fn selected_ray_probe_is_bounded_and_captures_the_detailed_shadow_query() {
    let bindings = include_str!("../../../shaders/include/bindings.glsl");
    let shadow = include_str!("../../../shaders/include/shadow_transport.glsl");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let triangle = include_str!("../../../shaders/triangle.frag");
    let draw = include_str!("../context/draw.rs");

    assert!(bindings.contains("binding = 19) coherent buffer SelectedRayProbeBuffer"));
    assert!(shadow.contains("traceShadowTransmittanceDetailed("));
    assert!(lighting.contains("traceLightTransmittanceDetailed("));
    assert!(triangle.contains("atomicCompSwap(selectedRayProbeControl.y, 1u, 2u)"));
    assert!(triangle.contains("selectedRayProbeLightParams = lights[selectedLightDebug].params"));
    assert!(triangle.contains("atomicExchange(selectedRayProbeControl.y, 3u)"));
    let arm_barrier = draw
        .split_once("// Barrier: make the instance SSBO host write")
        .and_then(|(_, tail)| tail.split_once("// #1255").map(|(block, _)| block))
        .expect("selected-ray arm must precede the shared host-to-shader barrier");
    assert!(arm_barrier.contains("vk::PipelineStageFlags::HOST"));
    assert!(arm_barrier.contains("vk::AccessFlags::HOST_WRITE"));
    assert!(arm_barrier.contains("vk::PipelineStageFlags::FRAGMENT_SHADER"));
    assert!(arm_barrier.contains("vk::AccessFlags::SHADER_READ"));
    assert!(arm_barrier.contains("vk::AccessFlags::SHADER_WRITE"));

    let publish_barrier = draw
        .split_once("// Publish the bounded fragment-shader probe record to the host.")
        .and_then(|(_, tail)| {
            tail.split_once("// Soft-particle depth fade:")
                .map(|(block, _)| block)
        })
        .expect("selected-ray publish must have a fragment-to-host barrier");
    assert!(publish_barrier.contains("vk::PipelineStageFlags::FRAGMENT_SHADER"));
    assert!(publish_barrier.contains("vk::AccessFlags::SHADER_WRITE"));
    assert!(publish_barrier.contains("vk::PipelineStageFlags::HOST"));
    assert!(publish_barrier.contains("vk::AccessFlags::HOST_READ"));
}

/// Regression: #417 — every shader that declares its own copy of
/// `struct GpuInstance` must name required fields correctly (originally
/// the final u32 slot as `materialKind`, not `_pad1` or any other legacy
/// placeholder; R1 Phase 6 later moved `materialKind` off this struct
/// entirely — see the field-presence loop below for what's still
/// checked). The Rust side guards offsets via
/// `gpu_instance_field_offsets_match_shader_contract`; this test is
/// presence-only (`src.contains(...)`) — it does NOT check field order
/// or that all five mirrors match each other or the Rust struct. That
/// full lockstep guard is `gpu_instance_glsl_copies_stay_in_lockstep`
/// (#2748 / REN-D3-2026-08-12-01); this test stays as a cheap
/// complementary check for the specific stale-name / missing-field /
/// reintroduced-stale-field regressions it was written against.
///
/// Walks the shaders tree at compile time via `include_str!` —
/// works in `cargo test` even on machines that don't have
/// glslangValidator installed, and catches the missed-rename
/// failure mode from #417 (caustic_splat.comp still said
/// `uint _pad1;` after the triangle.* / ui.vert rename).
#[test]
fn every_shader_struct_gpu_instance_names_expected_fields() {
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
            // Stable temporal-shadow identity. Must occupy the former
            // avg-albedo padding lane in every mirror.
            "surfaceId",
            // #2219 — skinned-instance deformed-vertex buffer address.
            // Every mirror must declare it (even the 4 that never
            // dereference it) so the std430 stride stays byte-identical.
            "skinnedVertexAddress",
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

#[test]
fn restir_history_uses_stable_surface_id_not_instance_order() {
    let src = include_str!("../../../shaders/triangle.frag");
    assert!(
        src.contains("uint surfaceId = inst.surfaceId & RESERVOIR_SURFACE_MASK;"),
        "ReSTIR history must key surfaces by stable GpuInstance.surfaceId"
    );
    assert!(
        !src.contains("uint surfaceId = uint(fragInstanceIndex) + 1u;"),
        "per-frame sorted instance indices invalidate shadow history when actors reorder"
    );
}

#[test]
fn gbuffer_history_uses_stable_surface_id_but_caustics_keep_draw_lookup() {
    let src = include_str!("../../../shaders/triangle.frag");
    assert!(
        src.contains("uint stableSurfaceId = inst.surfaceId & 0x7FFFFFFFu;")
            && src.contains("alphaBlendFrag ? sortedInstanceId : stableSurfaceId"),
        "opaque TAA/SVGF history must use stable identity while alpha caustics keep the current draw index"
    );
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
/// against future drift. See `scene_buffer/gpu_types.rs:191-197` for
/// the contract and `feedback_shader_struct_sync.md` for the
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

/// Water's fragment ray path must resolve the real material at a committed
/// hit. The scene descriptor set already contains the material table and
/// global geometry buffers, and startup SPIR-V reflection validates their
/// union across triangle + water shaders. Keep the vertex shader on the
/// compact per-water-plane path while pinning the fragment shader to the
/// shared hit reconstruction contract.
#[test]
fn water_fragment_uses_shared_material_aware_ray_hits() {
    let vert = include_str!("../../../shaders/water.vert");
    let frag = include_str!("../../../shaders/water.frag");
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");

    assert!(
        !vert.contains("include/bindings.glsl")
            && !vert.contains("buffer MaterialBuffer")
            && !vert.contains("materials["),
        "water.vert must remain on its compact per-plane instance path; \
         only committed fragment-ray hits need scene material lookup."
    );
    assert!(
        frag.contains("#include \"include/bindings.glsl\"")
            && frag.contains("#include \"include/ray_hit.glsl\""),
        "water.frag must consume the shared scene descriptors and hit helpers."
    );
    for needle in [
        "materials[inst.materialId]",
        "instIdx, primIdx, bary, direction, mat",
        "instIdx, primIdx, bary, inst, mat, uv, baseSample",
        "rayHitAlbedo(mat, baseSample.rgb)",
        "rayHitEmission(mat, uv, baseSample.rgb, 0.0)",
    ] {
        assert!(
            frag.contains(needle),
            "water.frag must reconstruct committed hit material data via `{needle}`."
        );
    }
    for helper in [
        "vec2 getHitUV(",
        "vec2 resolveRayHitUV(",
        "float getHitVertexAlpha(",
        "bool rayHitHasCoverage(",
        "vec3 rayHitAlbedo(",
        "vec3 rayHitEmission(",
    ] {
        assert!(
            hit.contains(helper),
            "shared ray_hit.glsl is missing `{helper}`."
        );
    }
    assert!(
        !frag.contains("avgAlbedoR")
            && !frag.contains("avgAlbedoG")
            && !frag.contains("avgAlbedoB"),
        "water rays must not regress to the flat instance-average shortcut."
    );
}

/// Raster and every material-aware ray must agree on the complete authored
/// alpha expression. In particular, NiAlphaProperty makes vertex alpha
/// authoritative even when the shader's vertex-colour flags are clear.
#[test]
fn secondary_ray_coverage_includes_barycentric_vertex_alpha() {
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");
    let shadow = include_str!("../../../shaders/include/shadow_transport.glsl");

    for needle in [
        "float getHitVertexAlpha(",
        "VERTEX_COLOR_OFFSET_FLOATS + 3u",
        "getHitVertexAlpha(instanceIdx, primitiveIdx, barycentrics)",
    ] {
        assert!(
            hit.contains(needle),
            "secondary-ray coverage is missing `{needle}`"
        );
    }
    assert!(
        shadow.contains("uint(hitIdx), uint(hitPrim), hitBary,"),
        "shadow traversal must pass committed-hit barycentrics into coverage"
    );
}

/// #2747 (REN-D10-02) regression. `getHitTriWorldPositions`'s header
/// promises ABSOLUTE world-space positions from both branches. Pre-fix
/// the rigid branch multiplied bind-pose vertices by `hi.model`, which
/// has been render-origin-RELATIVE since the markarth cascade
/// (`rebase_model_matrix` subtracts `render_origin` unconditionally),
/// while the skinned branch read `skin_vertices.comp`'s output, which is
/// ABSOLUTE (same convention `tlas_instance_transform` relies on for
/// `IDENTITY_VK_TRANSFORM`). No wrong pixels resulted (every consumer
/// only differences the three positions, and a uniform offset cancels),
/// but the mixed convention was a latent trap for a future absolute
/// consumer. Static source check: the rigid branch must lift with
/// `+ renderOrigin.xyz`, matching the skinned branch's frame.
#[test]
fn get_hit_tri_world_positions_returns_absolute_space_on_both_branches() {
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");
    let fn_start = hit
        .find("void getHitTriWorldPositions(")
        .expect("getHitTriWorldPositions definition must exist");
    let fn_body = &hit[fn_start..];
    let fn_end = fn_body
        .find("\n}\n")
        .map(|i| i + 3)
        .expect("getHitTriWorldPositions body must close");
    let fn_body = &fn_body[..fn_end];

    // Skinned branch (already absolute — pin it stays that way).
    assert!(
        fn_body.contains("SkinnedVertexRef ref = SkinnedVertexRef(hi.skinnedVertexAddress);"),
        "skinned branch must still read skin_vertices.comp's absolute output"
    );

    // Rigid branch must lift `hi.model`'s render-origin-relative result
    // to absolute — the actual #2747 fix.
    for (w, v) in [("w0", "v0"), ("w1", "v1"), ("w2", "v2")] {
        let needle = format!("{w} = (hi.model * vec4({v}, 1.0)).xyz + renderOrigin.xyz;");
        assert!(
            fn_body.contains(&needle),
            "getHitTriWorldPositions: rigid branch must lift `{w}` to \
             absolute space (expected `{needle}`) — `hi.model` alone is \
             render-origin-RELATIVE, so returning it unlifted mismatches \
             the skinned branch's absolute convention (#2747)"
        );
    }
    assert!(
        !fn_body.contains("(hi.model * vec4(v0, 1.0)).xyz;"),
        "getHitTriWorldPositions: rigid branch must not reintroduce the \
         pre-#2747 unlifted (relative) assignment"
    );
}

/// Pin the scale-aware offset and its zero-tMin shadow traversal together.
/// Reintroducing a fixed world epsilon at either layer recreates acne at
/// large coordinates and light leaks at small/detail scale.
#[test]
fn shadow_transport_uses_scale_aware_ray_origin_offset() {
    let origin = include_str!("../../../shaders/include/ray_origin.glsl");
    let shadow = include_str!("../../../shaders/include/shadow_transport.glsl");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    for needle in [
        "vec3 offsetRayOrigin(vec3 p, vec3 n)",
        "vec3 relativePoint = p - renderOrigin.xyz",
        "intBitsToFloat(floatBitsToInt(relativePoint.x)",
        "return relativeOffset + renderOrigin.xyz",
        "const float INT_SCALE = 256.0",
    ] {
        assert!(
            origin.contains(needle),
            "robust offset is missing `{needle}`"
        );
    }
    assert!(
        shadow.contains("opaqueOrigin, 0.0, direction, opaqueRemaining")
            && shadow.contains("rayOrigin, 0.0, direction, remaining")
            && shadow.contains("advanceShadowRayPastHit("),
        "shadow transport must start at robustly offset origins and advance \
         past transparent hits without a fixed epsilon"
    );
    assert!(
        lighting.contains("offsetRayOrigin(p, n)"),
        "secondary-hit direct-light shadows must share the robust offset"
    );
}

/// Every main secondary-ray consumer must use the same scale-aware origin
/// contract. A fixed engine-unit epsilon cannot be correct in both a small
/// interior detail and Starfield's large-coordinate cells.
#[test]
fn all_secondary_ray_consumers_use_scale_aware_origins() {
    let origin = include_str!("../../../shaders/include/ray_origin.glsl");
    let reflection = include_str!("../../../shaders/include/raytrace.glsl");
    let triangle = include_str!("../../../shaders/triangle.frag");
    let water = include_str!("../../../shaders/water.frag");
    let caustic = include_str!("../../../shaders/caustic_splat.comp");

    assert!(origin.contains("vec3 offsetRayOriginForDirection("));
    for (source, name) in [
        (reflection, "reflection"),
        (triangle, "triangle"),
        (water, "water"),
        (caustic, "caustic"),
    ] {
        assert!(
            source.contains("offsetRayOriginForDirection("),
            "{name} rays must select the robust offset side from their outgoing direction"
        );
    }

    for (source, forbidden) in [
        (reflection, "rayOrigin, 0.05, direction"),
        (triangle, "pathOrigin, 0.05, pathDir"),
        (triangle, "fragWorldPos - N * 0.15"),
        (triangle, "fragWorldPos + N_bias * 0.1"),
        (water, "rayOrigin, 0.05, direction"),
        (water, "worldPos, 0.05, -surfaceNormal"),
        (water, "vWorldPos - Nsurface * 0.05, 0.05"),
        (caustic, "G + ns * 0.1"),
        (caustic, "G - ns * 0.1"),
        (caustic, "exitPoint + receiverDir * 0.1"),
        (caustic, "receiverOrigin, 0.05"),
    ] {
        assert!(
            !source.contains(forbidden),
            "secondary-ray transport reintroduced fixed offset/tMin `{forbidden}`"
        );
    }
}

/// A transport image is an oracle only if later frame-graph terms preserve
/// it. Pin the bypass through composite, temporal upscale, and presentation
/// so fog/bloom/ACES cannot make black, white, or isolated energy ambiguous.
///
/// #2978 — this used to assert that four hardcoded clause strings appeared in
/// each shader. That is a subset check against an expected set derived from
/// nothing: a fifth raw-output view added to the Rust predicate left both
/// shaders tone-mapping a correctness oracle with the suite green. Both
/// shaders now consume the generated `DBG_VIZ_REQUIRES_RAW_OUTPUT(flags)`
/// macro, so the policy has one definition; what this test guards is that
/// neither shader has re-grown a hand-written copy of it.
#[test]
fn correctness_debug_views_bypass_non_transport_frame_graph_terms() {
    use crate::shader_constants::{DBG_VIZ_RAW_OUTPUT_ALL, DBG_VIZ_RAW_OUTPUT_ANY};

    let composite = include_str!("../../../shaders/composite.frag");
    let presentation = include_str!("../../../shaders/presentation.frag");
    for (source, name) in [(composite, "composite"), (presentation, "presentation")] {
        assert!(
            source.contains("DBG_VIZ_REQUIRES_RAW_OUTPUT(dbgFlags)")
                && source.contains("RENDER_DEBUG_LEGACY_FLAGS")
                && source.contains("if (rawDebug)"),
            "{name} must preserve renderer correctness-oracle output via the \
             generated DBG_VIZ_REQUIRES_RAW_OUTPUT policy macro"
        );
        // Every catalog entry is covered by construction once the macro is
        // used. Iterating the catalogs here instead of listing literals
        // makes the reverse true too: a shader that re-spells any single
        // clause by hand has forked the policy and fails, however many
        // views the catalogs grow to.
        for (view, _) in DBG_VIZ_RAW_OUTPUT_ANY.iter().chain(DBG_VIZ_RAW_OUTPUT_ALL) {
            assert!(
                !source.contains(&format!("dbgFlags & {view}")),
                "{name} re-spells the raw-output policy for {view} by hand — \
                 it must consume DBG_VIZ_REQUIRES_RAW_OUTPUT instead (#2978)"
            );
        }
    }
    let post = include_str!("../context/post_passes.rs");
    assert!(post.contains("render_debug_requires_raw_output"));
    assert!(post.contains("force_native_debug"));
}

/// Tier zero is the pre-timing watchdog-safe floor. Zero path limits must
/// suppress the GI branch before its internal clamps turn them back into a
/// one-segment/one-hit workload.
#[test]
fn gi_zero_budget_is_a_true_no_ray_floor() {
    let triangle = include_str!("../../../shaders/triangle.frag");
    assert!(
        triangle.contains("&& rayBudget.maxPathSegments > 0u")
            && triangle.contains("&& rayBudget.maxShadedHits > 0u"),
        "the outer GI gate must consume the tier-zero sentinels before the \
         path loop clamps active tiers"
    );
    assert!(
        triangle.contains("clamp(rayBudget.maxPathSegments, 1u, 6u)")
            && triangle.contains("clamp(rayBudget.maxShadedHits, 1u, 2u)"),
        "active GI tiers still require bounded non-zero loop limits"
    );
}

/// The legacy normal-map alpha lane is authored as specular intensity. It
/// must gate local-light specular and serve as the environment-reflection
/// fallback only when no dedicated environment mask is present. It must not
/// change roughness; dedicated gloss textures retain the roughness path.
#[test]
fn normal_alpha_masks_specular_intensity_not_roughness() {
    let triangle = include_str!("../../../shaders/triangle.frag");
    let branch_start = triangle
        .find("if (normalAlphaSpec) {")
        .expect("normal-alpha specular branch");
    let branch_end = triangle[branch_start..]
        .find("} else {")
        .map(|offset| branch_start + offset)
        .expect("dedicated-gloss sibling branch");
    let normal_alpha_branch = &triangle[branch_start..branch_end];

    assert!(normal_alpha_branch.contains("specStrength *= normalAlphaSpecMask"));
    assert!(
        !normal_alpha_branch.contains("roughness"),
        "normal alpha must not be reinterpreted as gloss/roughness"
    );
    assert!(triangle.contains(
        "mat.envMaskIndex != 0u\n            ? environmentMask\n            : normalAlphaSpecMask"
    ));
    assert!(triangle.contains("* environmentReflectionMask * environmentStrength"));
}

/// Every secondary path that samples a committed material hit must resolve
/// the same parallax-displaced UV as the primary raster surface. The BVH
/// remains the authored triangle mesh; this pins material-space consistency,
/// not displaced silhouettes.
#[test]
fn secondary_ray_material_hits_resolve_parallax_uvs() {
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");
    let raytrace = include_str!("../../../shaders/include/raytrace.glsl");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let shadow_transport = include_str!("../../../shaders/include/shadow_transport.glsl");
    let triangle = include_str!("../../../shaders/triangle.frag");
    let water = include_str!("../../../shaders/water.frag");

    for needle in [
        "bool getRayHitTangentFrame(",
        "vec2 resolveRayHitUV(",
        "VERTEX_NORMAL_OFFSET_FLOATS",
        "VERTEX_TANGENT_OFFSET_FLOATS",
        "mat.parallaxMapIndex == 0u",
        "(dbgFlags & DBG_BYPASS_POM) != 0u",
        "textureLod(",
    ] {
        assert!(
            hit.contains(needle),
            "shared secondary-hit POM is missing `{needle}`."
        );
    }

    for (source_name, source, expected_calls) in [
        ("raytrace.glsl", raytrace, 1),
        ("lighting.glsl", lighting, 0),
        ("shadow_transport.glsl", shadow_transport, 2),
        ("triangle.frag", triangle, 2),
        ("water.frag", water, 1),
    ] {
        assert_eq!(
            source.matches("resolveRayHitUV(").count(),
            expected_calls,
            "{source_name} must route every material-sampling ray hit \
             through shared parallax UV resolution."
        );
        assert!(
            !source.contains("transformRayHitUV("),
            "{source_name} must not bypass shared parallax UV resolution."
        );
    }
}

/// Reflection tint belongs only to reflected radiance, and transmitted rays
/// must start on the opposite side of the view-facing water surface. These
/// source pins catch the exact coupling and self-intersection bugs that made
/// water render as a pale, flat slab.
#[test]
fn water_reflection_and_refraction_keep_distinct_two_sided_semantics() {
    let frag = include_str!("../../../shaders/water.frag");
    let trace_start = frag.find("vec3 traceWaterRay(").expect("traceWaterRay");
    let trace_end = frag[trace_start..]
        .find("// ── Beer-Lambert")
        .map(|offset| trace_start + offset)
        .expect("traceWaterRay section terminator");
    let trace = &frag[trace_start..trace_end];

    assert!(
        !trace.contains("tint_reflect"),
        "the shared water-ray terminus must not apply a reflection-only tint."
    );
    assert!(
        frag.contains("reflColor *= push.tint_reflect.rgb;"),
        "WATR reflection colour must filter reflected radiance explicitly."
    );
    assert!(
        frag.contains("jitter.w > 0.5 ? skyTint.xyz : sceneFlags.yzw"),
        "water reflection misses must use sky only outdoors and cell ambient indoors."
    );
    assert!(
        frag.contains("offsetRayOriginForDirection(vWorldPos, N, Tdir)")
            && frag.contains("? (1.0 / max(ior, 1.0))")
            && frag.contains(": max(ior, 1.0);"),
        "refraction must select the robust transmission side and reverse eta underwater."
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

#[test]
fn triangle_vert_uses_previous_rigid_model_buffer() {
    let src = include_str!("../../../shaders/triangle.vert");
    assert!(
        src.contains("binding = 18) readonly buffer PreviousModelBuffer"),
        "rigid motion requires the vertex-only previous-model SSBO"
    );
    assert!(
        src.contains("xformPrev = previousModels[gl_InstanceIndex]"),
        "rigid vertices must use the previous transform aligned to the current instance index"
    );
    assert!(
        !src.contains("xformPrev = inst.model"),
        "reusing the current model erases rigid object motion"
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

/// REN-LOW L-9 / #2164 regression guard for the **#1496** varying
/// convention — the sibling of #1486's check above, which #1496 never got.
///
/// `triangle.vert` emits `fragWorldPosRel` render-origin-*relative*;
/// `triangle.frag` reconstructs the absolute `fragWorldPos` once at the
/// top of `main()`. The point of the split is that the four derivative
/// consumers keep differentiating the *relative* position, so `dFdx`/
/// `dFdy` form from small magnitudes and f32 quantization lands after
/// the derivative stage (pre-#1496 the absolute varying fed those
/// derivatives up to ~0.0156 u of ULP noise at `|world| ≥ 131k`).
///
/// Until this test, that convention was enforced only by shader
/// comments: `grep -rl fragWorldPosRel --include=*.rs` returned nothing
/// repo-wide, so renaming the varying — or quietly switching one
/// derivative consumer to the absolute local, which is the *easy*
/// mistake since both are in scope — compiled clean and passed every
/// renderer test while silently restoring the precision bug.
#[test]
fn triangle_shaders_keep_the_render_origin_relative_varying_convention() {
    let vert = include_str!("../../../shaders/triangle.vert");
    let frag = include_str!("../../../shaders/triangle.frag");

    assert!(
        vert.contains("fragWorldPosRel = worldPos.xyz"),
        "triangle.vert: the `location = 3` varying must be emitted \
         render-origin-RELATIVE (`fragWorldPosRel = worldPos.xyz`) — \
         emitting the absolute position re-introduces the #1496 \
         derivative-precision bug."
    );
    assert!(
        frag.contains("fragWorldPosRel + renderOrigin.xyz"),
        "triangle.frag: the absolute position must be reconstructed once \
         at the top of main() as `fragWorldPosRel + renderOrigin.xyz` \
         (#1496)."
    );

    // The four derivative consumers. Each must differentiate the
    // RELATIVE position, never the reconstructed absolute one.
    for (needle, what) in [
        (
            "cross(dFdx(fragWorldPosRel), dFdy(fragWorldPosRel))",
            "flat-shading normal",
        ),
        (
            "fragWorldPosRel,  // #1496 — derivative fallback stays origin-relative",
            "POM / parallaxDisplaceUV",
        ),
        (
            "perturbNormal(N, fragWorldPosRel,",
            "derivative TBN (perturbNormal)",
        ),
        (
            "max(length(dFdx(fragWorldPosRel)), length(dFdy(fragWorldPosRel)))",
            "rtLOD footprint",
        ),
    ] {
        assert!(
            frag.contains(needle),
            "triangle.frag: the {what} consumer must take `fragWorldPosRel`, \
             not the reconstructed absolute `fragWorldPos` — differentiating \
             the absolute position is exactly the #1496 precision bug \
             (REN-LOW L-9 / #2164). Expected to find: `{needle}`"
        );
    }
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

/// #2744 (REN-D10-01) regression. `cluster_cull.comp` differences a
/// near-plane corner against the camera position to get a view-ray
/// direction. That difference is a ~0.1-world-unit vector (the near
/// plane distance) — it must be formed from two RELATIVE (small-
/// magnitude) operands so f32 keeps full precision. Lifting either
/// operand to ABSOLUTE world space before the subtraction throws that
/// precision away: at exterior render-origin magnitudes (Markarth-scale,
/// `|world| ~ 176000`) adjacent cluster-tile boundaries collapse onto
/// the same f32, degenerating into zero-width frustum voxels that
/// silently drop point/spot lights from affected tiles.
///
/// Static source check (no `glslangValidator` dependency): the near
/// corners must stay unlifted (`ndcToWorldRel`, no `+ renderOrigin`) and
/// the ray direction must difference two RELATIVE quantities
/// (`nearCornersRel[i] - camRel`) — never `nearCorners[i] - camPos`,
/// which is what pre-fix code did once `ndcToWorld` lifted its result to
/// absolute before this subtraction ran.
#[test]
fn cluster_cull_differences_relative_positions_for_ray_direction() {
    let src = include_str!("../../../shaders/cluster_cull.comp");
    assert!(
        src.contains("vec3 ndcToWorldRel(vec2 ndcXY, float ndcZ, mat4 invViewProj) {"),
        "cluster_cull.comp: expected a RELATIVE-space unprojection helper \
             (`ndcToWorldRel`) — near-plane corners must not be lifted to \
             absolute before the ray-direction subtraction (#2744)."
    );
    // The relative helper's own body must NOT lift to absolute — i.e.
    // must not add `renderOrigin` before returning.
    let helper_start = src
        .find("vec3 ndcToWorldRel(")
        .expect("ndcToWorldRel definition must exist");
    let helper_body = &src[helper_start..helper_start + 400.min(src.len() - helper_start)];
    let helper_end = helper_body
        .find('}')
        .map(|i| i + 1)
        .unwrap_or(helper_body.len());
    let helper_body = &helper_body[..helper_end];
    assert!(
        !helper_body.contains("renderOrigin"),
        "cluster_cull.comp: `ndcToWorldRel` must return a RENDER-ORIGIN-\
             RELATIVE position — it must not reference `renderOrigin` at \
             all (#2744). Found in body: {helper_body:?}"
    );
    assert!(
        src.contains("vec3 camRel = camPos - renderOrigin.xyz;"),
        "cluster_cull.comp: expected an exact relative camera position \
             (`camRel = camPos - renderOrigin.xyz`) computed once before \
             the per-corner ray-direction loop (#2744)."
    );
    assert!(
        src.contains("normalize(nearCornersRel[i] - camRel)"),
        "cluster_cull.comp: the ray direction must difference two \
             RELATIVE quantities (`nearCornersRel[i] - camRel`) — \
             differencing already-absolute positions is the #2744 \
             precision bug."
    );
    assert!(
        !src.contains("nearCorners[i] - camPos"),
        "cluster_cull.comp: must not reintroduce the pre-#2744 \
             absolute-minus-absolute ray-direction difference."
    );
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
/// shared secondary-hit include plus its triangle fragment consumer
/// for any forbidden offset — `+ 12` through `+ 15` (bone
/// indices) or `+ 20` / `+ 21` (splat weights) — that ISN'T
/// wrapped in `floatBitsToUint(…)` or `unpackUnorm4x8(…)`.
///
/// `caustic_splat.comp` and `ui.vert` don't bind GlobalVertices
/// at all and aren't checked. `skin_vertices.comp` reads bone
/// indices but does so through `floatBitsToUint`; the regex
/// excludes that pattern.
#[test]
fn rt_hit_shaders_have_no_unsafe_vertex_data_reads() {
    let sources = [
        (
            "triangle.frag",
            include_str!("../../../shaders/triangle.frag"),
        ),
        (
            "ray_hit.glsl",
            include_str!("../../../shaders/include/ray_hit.glsl"),
        ),
    ];

    // Strip safe-recovery wrappers so a forbidden raw read
    // surfaces as a literal `vertexData[... + 11..14|19|20]`.
    // We don't run a full GLSL parser; instead, line-by-line
    // we reject any line that contains the forbidden offset
    // pattern AND no `floatBitsToUint` / `unpackUnorm4x8` /
    // `floatBitsToInt` recovery call. Whitespace tolerant.
    for (source_name, src) in sources {
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
                        "{source_name}:{}: unsafe `vertexData[... + {}]` read \
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

/// Ordered field names of a Rust `#[repr(C)]` struct (e.g.
/// `"pub struct GpuMaterial"` / `"pub struct GpuInstance"`), parsed from
/// its source file. A field line is `pub <ident>: <ty>,`; comment /
/// attribute / blank lines are skipped.
pub(super) fn parse_rust_struct_fields(src: &str, decl: &str) -> Vec<String> {
    let body =
        extract_struct_body(src, decl).unwrap_or_else(|| panic!("source must declare `{decl}`"));
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

/// Ordered field names of a GLSL struct declaration (e.g.
/// `"struct GpuMaterial"` / `"struct GpuInstance"`), parsed from a GLSL
/// source file. Handles multi-name declarations (`float a, b, c;`) and
/// skips `//`/`///` comment lines.
fn parse_glsl_struct_fields(src: &str, decl: &str) -> Vec<String> {
    const TYPES: &[&str] = &[
        "float", "uint", "int", "bool", "vec2", "vec3", "vec4", "mat2", "mat3", "mat4",
        // GpuInstance-only types (#2219 skinned_vertex_address + padding).
        "uint64_t", "uvec2",
    ];
    let body =
        extract_struct_body(src, decl).unwrap_or_else(|| panic!("source must declare `{decl}`"));
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
/// `roughness`) that preserves the struct's pinned size — the shader would
/// then
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

    let rust_fields = parse_rust_struct_fields(rust_src, "pub struct GpuMaterial");
    let glsl_fields = parse_glsl_struct_fields(glsl_src, "struct GpuMaterial");

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
             keeps the struct size unchanged but corrupts every lit-surface read — see \
             #1657 / SF-D8-01.",
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

// ── GpuInstance five-way GLSL lockstep (#2748 / REN-D3-2026-08-12-01) ──

/// #2748 — `struct GpuInstance` is hand-duplicated across **five** GLSL
/// sources (`include/bindings.glsl`, `triangle.vert`, `ui.vert`,
/// `water.vert`, `caustic_splat.comp`), the largest mirror fan-out of any
/// GPU struct in the codebase. Its only prior guard,
/// `every_shader_struct_gpu_instance_names_expected_fields` below, is
/// presence-only (`src.contains(...)`) — it never compared the five
/// declarations to each other or to the Rust struct, and never checked
/// field order or completeness. `caustic_splat.comp` uses a multi-name
/// declaration (`float avgAlbedoR, avgAlbedoG, avgAlbedoB;`) where every
/// other mirror uses three separate lines, so this walks each source
/// through `parse_glsl_struct_fields` (which already understands
/// multi-name declarations, unlike the raw-line `strip_struct_body` used
/// for `GpuLight`) rather than comparing stripped source text directly.
///
/// First asserts all five mirrors declare an identical field list, name
/// and order alike; then reuses `parse_rust_struct_fields` /
/// `normalize_ident` (the same #1657 machinery
/// `gpu_material_glsl_field_order_matches_rust_struct` uses) to assert
/// that shared list matches the Rust `#[repr(C)] struct GpuInstance`
/// field order in `gpu_types.rs` — the same two-leg coverage
/// `GpuMaterial` and `GpuLight` already have.
#[test]
fn gpu_instance_glsl_copies_stay_in_lockstep() {
    const SOURCES: &[(&str, &str)] = &[
        (
            "include/bindings.glsl",
            include_str!("../../../shaders/include/bindings.glsl"),
        ),
        (
            "triangle.vert",
            include_str!("../../../shaders/triangle.vert"),
        ),
        ("ui.vert", include_str!("../../../shaders/ui.vert")),
        ("water.vert", include_str!("../../../shaders/water.vert")),
        (
            "caustic_splat.comp",
            include_str!("../../../shaders/caustic_splat.comp"),
        ),
    ];

    let mut reference: Option<(&str, Vec<String>)> = None;
    for (name, src) in SOURCES {
        let fields = parse_glsl_struct_fields(src, "struct GpuInstance");
        assert!(
            fields.len() >= 14,
            "{name}: parsed only {} GpuInstance field(s) — parser likely broke",
            fields.len()
        );
        match &reference {
            None => reference = Some((name, fields)),
            Some((ref_name, ref_fields)) => {
                assert_eq!(
                    ref_fields, &fields,
                    "GpuInstance layout mismatch: `{ref_name}` vs `{name}`. All five GLSL copies \
                     of `struct GpuInstance` must declare identical fields in the same order \
                     (Shader Struct Sync invariant, #2748 / REN-D3-2026-08-12-01) — a drift here \
                     silently corrupts per-instance data for whichever copy lags behind."
                );
            }
        }
    }

    // Second leg: the shared GLSL field list must also match the Rust
    // `#[repr(C)]` struct's declaration order (the offset source of
    // truth, pinned separately by
    // `gpu_instance_field_offsets_match_shader_contract`).
    let (_, glsl_fields) = reference.expect("SOURCES is non-empty");
    let rust_src = include_str!("gpu_types.rs");
    let rust_fields = parse_rust_struct_fields(rust_src, "pub struct GpuInstance");

    let rust_norm: Vec<String> = rust_fields.iter().map(|f| normalize_ident(f)).collect();
    let glsl_norm: Vec<String> = glsl_fields.iter().map(|f| normalize_ident(f)).collect();

    assert_eq!(
        rust_norm.len(),
        glsl_norm.len(),
        "GpuInstance field COUNT differs: Rust has {} {:?}, GLSL mirrors have {} {:?}. The Rust \
         `struct GpuInstance` (gpu_types.rs) and its five GLSL mirrors must stay in lockstep — \
         see #2748 / REN-D3-2026-08-12-01.",
        rust_norm.len(),
        rust_fields,
        glsl_norm.len(),
        glsl_fields,
    );

    for (i, (r, g)) in rust_norm.iter().zip(glsl_norm.iter()).enumerate() {
        assert_eq!(
            r, g,
            "GpuInstance field #{i} ORDER mismatch: Rust `{}` vs GLSL `{}`. Every GLSL \
             `struct GpuInstance` mirror must declare fields in the SAME order as the Rust \
             `#[repr(C)]` struct — see #2748 / REN-D3-2026-08-12-01.",
            rust_fields[i], glsl_fields[i],
        );
    }
}

/// The bounded GI path must remain material-aware. The pre-fix implementation
/// multiplied every secondary hit by `hitAlbedo / PI` and sampled another
/// cosine hemisphere, turning polished conductors into diffuse color emitters.
/// These source-level pins complement the SPIR-V compile check: they catch a
/// semantically valid shader rollback that would otherwise compile cleanly.
#[test]
fn bounded_path_uses_ggx_bsdf_transport_and_directional_environment() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let pbr = include_str!("../../../shaders/include/pbr.glsl");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    for needle in [
        "vec3 sampleVisibleGgxNormal(",
        "vec3 evaluatePathBsdf(",
        "bool samplePathBsdf(",
        "specularPdf = D * G1V / max(4.0 * NdotV, 1e-6);",
    ] {
        assert!(
            pbr.contains(needle),
            "bounded path PBR helper `{needle}` is missing"
        );
    }
    for needle in [
        "vec3 pathEnvironmentRadiance(",
        "vec3 pathHitRadiance(",
        "pathLuminance(lights[i].color_type.rgb * candidateBsdf)",
    ] {
        assert!(
            lighting.contains(needle),
            "bounded path lighting helper `{needle}` is missing"
        );
    }
    for needle in [
        "vec3 primaryDiffuseWeight = (1.0 - fresnelSchlick(NdotV, F0))",
        "pathEnvironmentRadiance(pathDir)",
        "pathHitRadiance(",
        "samplePathBsdf(",
        "throughput *= bsdfWeight;",
    ] {
        assert!(
            frag.contains(needle),
            "triangle.frag bounded path no longer uses `{needle}`"
        );
    }
    assert!(
        !frag.contains("hitAlbedo * hitIrradiance * (1.0 / PI)"),
        "secondary path hits must not regress to unconditional Lambert shading"
    );
}

/// Fixed-exponent Schlick weights belong on the ordinary ALU multiply path,
/// not behind GLSL.std.450 `Pow`. Glass additionally uses the scalar helper so
/// its hot path does not broadcast a dielectric F0 only to read `.r` back.
#[test]
fn schlick_fresnel_uses_multiply_chain_and_scalar_glass_path() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let pbr = include_str!("../../../shaders/include/pbr.glsl");
    let shadow = include_str!("../../../shaders/include/shadow_transport.glsl");

    for needle in [
        "float schlickWeight(float cosTheta)",
        "float x2 = x * x;",
        "return x2 * x2 * x;",
        "float fresnelSchlickScalar(float cosTheta, float F0)",
    ] {
        assert!(
            pbr.contains(needle),
            "optimized Fresnel helper `{needle}` is missing"
        );
    }
    assert!(
        !pbr.contains("pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0)"),
        "Schlick must not regress to GLSL pow(x, 5)"
    );
    for needle in [
        "fresnelSchlickScalar(glassNdotV, f0Dielectric)",
        "fresnelSchlickScalar(NdotV_v, f0Dielectric)",
        "fresnelSchlickScalar(cosTheta, f0)",
    ] {
        assert!(
            frag.contains(needle),
            "glass path no longer uses `{needle}`"
        );
    }
    assert!(
        shadow.contains("shadowFresnelSchlickScalar(cosTheta, f0)"),
        "glass shadow transport must use its self-contained scalar Schlick helper"
    );
    assert!(
        shadow.contains("float weight = x2 * x2 * x;"),
        "glass shadow transport must not regress to pow(x, 5)"
    );
}

/// #2819 (REN-D17-05) — `disneyDiffuseSplit`'s sheen colour must mix toward a
/// luminance-normalised base-colour tint, not raw `albedo`. Both cited
/// references (Disney 2012 `disney.brdf`'s `Ctint = baseColor / Cdlum`, and
/// knightcrawler25/GLSL-PathTracer's `GetSpecColor`) normalise by luminance
/// first so `sheenTint` transfers hue without changing sheen intensity —
/// mixing in raw albedo instead makes a dark base colour (e.g. black velvet)
/// scale the whole sheen lobe down at `sheenTint = 1.0`. No automated pixel
/// test exists for this lobe (Cornell-harness capture is the verification
/// path per the issue), so this pins the GLSL source shape the same way the
/// other PBR-lobe regressions in this file do.
#[test]
fn disney_sheen_color_uses_luminance_normalised_tint_not_raw_albedo() {
    let pbr = include_str!("../../../shaders/include/pbr.glsl");

    for needle in [
        "float sheenLuminance = dot(albedo, vec3(0.3, 0.6, 0.1));",
        "vec3 sheenTintColor = sheenLuminance > 0.0 ? albedo / sheenLuminance : vec3(1.0);",
        "vec3 sheenColor = mix(vec3(1.0), sheenTintColor, sheenTint);",
    ] {
        assert!(
            pbr.contains(needle),
            "sheen luminance-normalisation `{needle}` is missing"
        );
    }
    assert!(
        !pbr.contains("vec3 sheenColor = mix(vec3(1.0), albedo, sheenTint);"),
        "sheen colour must not regress to mixing raw (non-luminance-normalised) albedo"
    );
}

#[test]
fn unresolved_glass_keeps_tint_and_low_angle_reflections() {
    let frag = include_str!("../../../shaders/triangle.frag");

    for needle in [
        "if (fresnelScalar > 0.05)",
        "float absorptionCoverage = min(",
        "vec3 tintedTransmission = refrColor * glassTint;",
        "reflColor * reflectionCoverage",
        "+ tintedTransmission * absorptionCoverage",
    ] {
        assert!(
            frag.contains(needle),
            "unresolved glass lost `{needle}` and will read as neutral passthrough"
        );
    }
    assert!(
        !frag.contains("glassSurface = reflColor;\n            resolvedAlpha"),
        "unresolved glass must not discard authored absorption tint"
    );
}

/// #2243 — Disney diffuse is /PI while sheen is not. The canonical clustered
/// path deliberately uses the legacy non-/PI Lambert convention, so it must
/// rescale the complete Disney lobe. Scaling diffuse alone makes sheen PI
/// times weaker relative to diffuse. Directional sources use this same path;
/// the former synthetic no-light sun and its duplicate lobe are gone.
#[test]
fn disney_sheen_keeps_its_relative_weight_in_canonical_direct_path() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    assert!(
        lighting.contains("diffuseBrdf = (dd.diffuse + dd.sheen) * PI * (1.0 - metalness);"),
        "clustered lighting must rescale diffuse and sheen together"
    );
    assert!(
        !lighting.contains("dd.diffuse * PI + dd.sheen"),
        "scaling only Disney diffuse changes sheen's relative weight by PI"
    );
    assert!(
        !frag.contains("diffuseBrdf = (dd.diffuse + dd.sheen)")
            && !frag.contains("Fallback: single directional light"),
        "triangle.frag must not reintroduce a duplicate synthetic-sun BRDF path"
    );
}

/// #2244 — `sampleDalcCube` returns authored directional irradiance. A
/// bounded path that escapes the TLAS consumes environment radiance, so the
/// DALC branch needs the Lambertian irradiance-to-radiance conversion.
#[test]
fn bounded_path_converts_dalc_irradiance_to_environment_radiance() {
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    assert!(
        lighting.contains("return sampleDalcCube(rayDir) * (1.0 / PI);"),
        "bounded path DALC escape must convert irradiance to radiance"
    );
    assert!(
        !lighting.contains("return sampleDalcCube(rayDir);"),
        "raw DALC irradiance must not feed the path radiance term"
    );
}

/// #2472 — `sceneFlags.yzw` (XCLL cell ambient) is the DALC arm's sibling
/// irradiance source in `pathEnvironmentRadiance` and must take the same
/// `1.0 / PI` conversion in every arm — the sky-mix (exterior) arm, the
/// interior non-DALC fallback arm here, and the reflection-miss sibling in
/// `triangle.frag`. #2244 fixed only the DALC arm, leaving a Skyrim-vs-
/// FO3/FNV ~π× ambient gap on the indirect path.
#[test]
fn bounded_path_converts_scene_flags_ambient_to_environment_radiance_in_every_arm() {
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    assert!(
        lighting.contains("mix(sceneFlags.yzw * (1.0 / PI), skyTint.xyz, skyWeight)"),
        "exterior sky-mix arm must convert sceneFlags.yzw to radiance before mixing with skyTint"
    );
    assert!(
        lighting.contains("return sceneFlags.yzw * (1.0 / PI) * 0.5;"),
        "interior non-DALC fallback arm must convert sceneFlags.yzw to radiance"
    );

    let frag = include_str!("../../../shaders/triangle.frag");
    assert!(
        frag.contains("sceneFlags.yzw * (1.0 / PI);"),
        "triangle.frag's reflection-miss ambientFallback must convert sceneFlags.yzw to radiance"
    );
}

/// Ambient-cube interpolation must conserve irradiance for unit normals.
///
/// Linear `abs(N)` weights sum to as much as sqrt(3), which made diagonal
/// normal-map texels visibly brighter than axis-aligned texels after interior
/// XCLL cubes were connected. Squared components form a partition of unity.
#[test]
fn directional_ambient_cube_uses_energy_conserving_squared_normal_weights() {
    let math = include_str!("../../../shaders/include/math_common.glsl");

    assert!(
        math.contains("vec3 n2 = N * N;"),
        "directional ambient cube must weight faces by squared normal components"
    );
    assert!(
        !math.contains("vec3 pw = max(N, vec3(0.0));"),
        "linear abs-normal cube weights inflate diagonal irradiance"
    );
}

/// Authored DALC/XCLL ambient replaces the legacy synthetic point-light fill.
///
/// Running both made the unshadowed fallback wash across the directional,
/// AO-modulated room lighting and visibly erase shadow contrast.
#[test]
fn triangle_frag_has_no_unshadowed_point_light_ambient_fill() {
    let frag = include_str!("../../../shaders/triangle.frag");

    assert!(
        !frag.contains("LIGHT_AMBIENT_FILL_FACTOR") && !frag.contains("lightAmbientFill"),
        "point/spot ambient fill bypasses N·L, visibility, and AO and must not return"
    );
}

/// XCLL directional colour is a physical key, distinct from XCLL/DALC
/// ambient irradiance. It must not regain a type-specific shader bypass.
#[test]
fn triangle_frag_has_no_unshadowed_xcll_directional_fill() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    assert!(!frag.contains("INTERIOR_FILL_AMBIENT_FACTOR"));
    assert!(!frag.contains("lightType > 2.5"));
    assert!(!lighting.contains("lightType > 2.5"));
}

/// Quality work may change the estimator, but the accepted #2161 cost point is
/// still a six-segment path with two diffuse events. Specular/glass transport
/// fits inside the same segment ceiling and must not silently expand the
/// worst-case ray-query budget.
#[test]
fn bounded_path_preserves_the_accepted_segment_and_diffuse_budgets() {
    let frag = include_str!("../../../shaders/triangle.frag");
    assert!(
        frag.contains("const int MAX_PATH_SEGMENTS = 6;"),
        "bounded path segment ceiling drifted from the accepted #2161 cost point"
    );
    assert!(
        frag.contains("const int MAX_DIFFUSE_BOUNCES = 2;"),
        "the accepted two-diffuse-event color-bleed path must remain enabled"
    );
    assert!(
        frag.contains("const int MAX_SHADED_HITS = 2;")
            && frag.contains("int shadedHitLimit = int(clamp(rayBudget.maxShadedHits, 1u, 2u));")
            && frag.contains("if (shadedHits < min(MAX_SHADED_HITS, shadedHitLimit))"),
        "glossy chains must not expand the accepted local-light shadow-query ceiling"
    );
    assert!(
        frag.contains("for (int segment = 0; segment < MAX_PATH_SEGMENTS; ++segment)")
            && frag.contains("if (segment >= pathSegmentLimit) break;"),
        "bounded path must enforce both the hard and adaptive segment ceilings"
    );
}

/// #2810 (REN-D17-08) — the #1250 isotropic-degeneracy contract, checked
/// numerically rather than by string mirror.
///
/// `distributionGGXAniso` documents that it "reduces exactly to
/// `distributionGGX` when ax == ay". That is the property which lets the
/// anisotropic lobe be the single NDF for both cases, so every legacy
/// isotropic material keeps its exact pre-#1250 appearance. The reduction
/// is not free algebra — it holds only because H is a unit vector in
/// tangent space (`HdotX² + HdotY² + NdotH² = 1`), which lets
///
///   `HdotX²/a² + HdotY²/a² + NdotH²`  collapse to  `(1 + NdotH²(a²-1))/a²`
///
/// i.e. the isotropic `denom` scaled by `1/a²`, whose square then cancels
/// against the `ax·ay = a²` prefactor. A future edit that drops the `ax*ay`
/// prefactor, or squares `ax`/`ay` one time too many/few, breaks the
/// identity while still compiling and still looking plausible — and this
/// lobe has no CPU producer (`anisotropic` is reachable only through
/// `mat.set`, see #2514), so nothing would catch it by eyeball either.
///
/// Ports both GLSL bodies verbatim so a divergence in either is caught.
#[test]
fn anisotropic_ggx_reduces_to_the_isotropic_ndf_at_zero_anisotropy() {
    const PI: f64 = std::f64::consts::PI;

    // Verbatim ports of `include/pbr.glsl`.
    fn distribution_ggx(n_dot_h: f64, roughness: f64) -> f64 {
        let a = roughness * roughness;
        let a2 = a * a;
        let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
        a2 / (PI * denom * denom)
    }
    fn distribution_ggx_aniso(n_dot_h: f64, h_dot_x: f64, h_dot_y: f64, ax: f64, ay: f64) -> f64 {
        let ax2 = ax * ax;
        let ay2 = ay * ay;
        let denom = h_dot_x * h_dot_x / ax2 + h_dot_y * h_dot_y / ay2 + n_dot_h * n_dot_h;
        1.0 / (PI * ax * ay * denom * denom)
    }
    fn derive_ax_ay(roughness: f64, anisotropic: f64) -> (f64, f64) {
        let alpha = roughness * roughness;
        let aniso = anisotropic.clamp(0.0, 1.0);
        let aspect = (1.0 - aniso * 0.9).sqrt();
        (
            (0.025f64 * 0.025).max(alpha / aspect),
            (0.025f64 * 0.025).max(alpha * aspect),
        )
    }

    // Sweep roughness above the `0.025² in α-units` floor so `deriveAxAy`
    // returns the unclamped `alpha` and the identity is exercised rather
    // than the floor. roughness ≥ 0.025 ⇒ alpha = roughness² ≥ 0.025².
    for &roughness in &[0.025, 0.05, 0.1, 0.25, 0.5, 0.75, 1.0] {
        let (ax, ay) = derive_ax_ay(roughness, 0.0);
        assert!(
            (ax - ay).abs() < 1e-15,
            "anisotropic=0 must give ax == ay (roughness {roughness}): {ax} vs {ay}"
        );

        // Sample unit half-vectors in tangent space: the identity depends
        // on `HdotX² + HdotY² + NdotH² == 1`.
        for &n_dot_h in &[0.05, 0.2, 0.5, 0.8, 0.95, 1.0] {
            let tangential = (1.0f64 - n_dot_h * n_dot_h).max(0.0).sqrt();
            for &phi in &[0.0, 0.3, 0.7854, 1.2, PI / 2.0] {
                let h_dot_x = tangential * phi.cos();
                let h_dot_y = tangential * phi.sin();

                let iso = distribution_ggx(n_dot_h, roughness);
                let aniso = distribution_ggx_aniso(n_dot_h, h_dot_x, h_dot_y, ax, ay);
                let rel = (aniso - iso).abs() / iso.max(1e-300);
                assert!(
                    rel < 1e-9,
                    "anisotropic NDF must reduce to the isotropic NDF at anisotropic=0 \
                     (roughness {roughness}, NdotH {n_dot_h}, phi {phi}): \
                     iso {iso}, aniso {aniso}, rel err {rel}"
                );
            }
        }
    }

    // The ported bodies must stay in lockstep with the GLSL they mirror.
    let pbr = include_str!("../../../shaders/include/pbr.glsl");
    assert!(
        pbr.contains("return 1.0 / (PI * ax * ay * denom * denom);"),
        "distributionGGXAniso's `ax*ay` prefactor is what cancels the `1/a²` in the \
         collapsed denominator — dropping it silently breaks the #1250 reduction"
    );
    assert!(
        pbr.contains("float denom = HdotX * HdotX / ax2 + HdotY * HdotY / ay2 + NdotH * NdotH;"),
        "distributionGGXAniso's denominator drifted from the form the reduction assumes"
    );
    assert!(
        pbr.contains("return a2 / (PI * denom * denom);"),
        "distributionGGX (isotropic) drifted; the degeneracy target changed"
    );
}

/// #2810 (REN-D17-08) — the #1254 anisotropic clamp.
///
/// `deriveAxAy` clamps `anisotropic` to [0, 1] before `sqrt(1 - 0.9·a)`.
/// Without it an authored value > 1.0 (defense-in-depth against a future
/// BGSM v9+ / Starfield `.mat` importer that forwards unclamped data)
/// makes the radicand negative → `ax`/`ay` NaN → `distributionGGXAniso`
/// NaN → black/undefined fragment. Negative inputs shrink `ax` below the
/// intended floor. Pinned both numerically and at the source, since the
/// guard is one line that a cleanup pass could plausibly delete.
#[test]
fn derive_ax_ay_clamps_anisotropic_against_nan_and_floor_escape() {
    let pbr = include_str!("../../../shaders/include/pbr.glsl");

    assert!(
        pbr.contains("float aniso = clamp(anisotropic, 0.0, 1.0);")
            && pbr.contains("float aspect = sqrt(1.0 - aniso * 0.9);"),
        "deriveAxAy must clamp `anisotropic` to [0,1] BEFORE the sqrt (#1254) — an \
         authored value > 1 otherwise makes the radicand negative and NaNs the lobe"
    );
    assert!(
        !pbr.contains("sqrt(1.0 - anisotropic * 0.9)"),
        "the sqrt must consume the clamped `aniso`, never the raw `anisotropic` (#1254)"
    );
    // The α-units floor is the other half of the contract — it mirrors
    // `specularAaRoughness`'s effective roughness ≥ 0.025 (see #2471 and
    // the #1250 closeout, which deferred the "drop to 0.001" suggestion).
    assert!(
        pbr.contains("ax = max(0.025 * 0.025, alpha / aspect);")
            && pbr.contains("ay = max(0.025 * 0.025, alpha * aspect);"),
        "deriveAxAy's 0.025² α-units floor must survive — it preserves the \
         BSLightingShader gloss-cap behaviour (#1250 closeout / #2471)"
    );

    // #2806 — the specular-AA filter must cap the ADDED kernel term, not
    // only the sum. Filament's `normalFiltering()` computes
    // `kernelRoughness = min(2.0 * variance, threshold)` and saturates
    // afterwards; this shader clamped the sum alone, so a single
    // high-frequency-normal fragment (foliage cutout, chain-link, fine
    // grating) could drive an authored-polished surface to roughness = 1.0
    // in one step and carry that into the anisotropic lobe via `deriveAxAy`.
    assert!(
        pbr.contains("min(2.0 * variance, SPECULAR_AA_THRESHOLD)"),
        "specularAaRoughness must cap the kernel term before adding it \
         (Filament `normalFiltering()`); clamping only the sum lets one \
         aliasing fragment saturate roughness to 1.0 (#2806)"
    );
    assert!(
        pbr.contains("#define SPECULAR_AA_VARIANCE")
            && pbr.contains("#define SPECULAR_AA_THRESHOLD"),
        "both specular-AA coefficients must be named and citable — every \
         neighbouring constant in this file is, and the bare literals were \
         indistinguishable from arbitrary ones (#2806)"
    );
    assert!(
        !pbr.contains("0.25 * (dot(dNdx, dNdx)"),
        "the variance coefficient must go through SPECULAR_AA_VARIANCE, not \
         a re-inlined literal (#2806)"
    );

    // Numeric: the capped filter must be monotonic in variance, must leave
    // a smooth surface smooth at zero variance, and must NOT reach the GGX
    // ceiling from the kernel alone however badly the normal aliases.
    fn filtered_roughness(roughness: f64, variance_sum: f64) -> f64 {
        const VARIANCE: f64 = 0.25;
        const THRESHOLD: f64 = 0.2;
        let kernel = (2.0 * VARIANCE * variance_sum).min(THRESHOLD);
        let alpha2 = (roughness * roughness).powi(2);
        (alpha2 + kernel).clamp(0.025f64.powi(4), 1.0).sqrt().sqrt()
    }
    let polished = 0.05;
    assert!(
        (filtered_roughness(polished, 0.0) - polished).abs() < 1e-6,
        "zero normal variance must return the authored roughness untouched"
    );
    let mut previous = filtered_roughness(polished, 0.0);
    for &variance_sum in &[0.01, 0.1, 1.0, 10.0, 1.0e6] {
        let filtered = filtered_roughness(polished, variance_sum);
        assert!(
            filtered >= previous,
            "filtered roughness must be monotonic in normal variance"
        );
        assert!(
            filtered < 0.9,
            "the kernel cap must keep an authored-polished surface off the \
             GGX ceiling however badly its normal aliases; variance_sum \
             {variance_sum} gave {filtered} (#2806)"
        );
        previous = filtered;
    }

    // Numeric: out-of-range inputs must stay finite and ordered.
    fn derive_ax_ay(roughness: f64, anisotropic: f64) -> (f64, f64) {
        let alpha = roughness * roughness;
        let aniso = anisotropic.clamp(0.0, 1.0);
        let aspect = (1.0 - aniso * 0.9).sqrt();
        (
            (0.025f64 * 0.025).max(alpha / aspect),
            (0.025f64 * 0.025).max(alpha * aspect),
        )
    }
    for &anisotropic in &[-5.0, -1.0, -0.001, 0.0, 0.5, 1.0, 1.001, 2.0, 1e6] {
        let (ax, ay) = derive_ax_ay(0.5, anisotropic);
        assert!(
            ax.is_finite() && ay.is_finite(),
            "anisotropic {anisotropic} produced a non-finite lobe: ax {ax}, ay {ay}"
        );
        assert!(
            ax >= 0.025 * 0.025 && ay >= 0.025 * 0.025,
            "anisotropic {anisotropic} escaped the α-units floor: ax {ax}, ay {ay}"
        );
        // `aspect ≤ 1` for every clamped input, so the tangent axis is
        // never the narrower of the two.
        assert!(
            ax >= ay - 1e-15,
            "ax must remain the stretched axis for anisotropic {anisotropic}: {ax} < {ay}"
        );
    }
}

/// The Rust field list and the GLSL field list must declare the same
/// members in the same order. Catches the case the size pin cannot: a
/// same-width reorder (swapping `layer_normal_index` and
/// `layer_specular_index`) keeps the struct 96 B while every tile samples
/// its normal maps as specular and vice versa.
#[test]
fn gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep() {
    const BINDINGS_GLSL: &str = include_str!("../../../shaders/include/bindings.glsl");
    const GPU_TYPES_RS: &str = include_str!("gpu_types.rs");

    let glsl = strip_struct_body(
        extract_struct_body(BINDINGS_GLSL, "struct GpuTerrainTile")
            .expect("include/bindings.glsl must declare `struct GpuTerrainTile`"),
    );
    let glsl_fields: Vec<String> = glsl
        .iter()
        .filter_map(|line| {
            // `uint layerDiffuseIndex[8];` → `layerdiffuseindex`
            let name = line.split_whitespace().nth(1)?;
            let name = name.split('[').next()?.trim_end_matches(';');
            Some(name.to_ascii_lowercase().replace('_', ""))
        })
        .collect();

    let rust_fields: Vec<String> =
        parse_rust_struct_fields(GPU_TYPES_RS, "pub struct GpuTerrainTile")
            .iter()
            .map(|f| f.to_ascii_lowercase().replace('_', ""))
            .collect();

    assert_eq!(
        rust_fields.len(),
        3,
        "GpuTerrainTile gained or lost a Rust field ({rust_fields:?}) — update the \
         GLSL mirror in include/bindings.glsl, the 96 B size pin, and the offset \
         pin above together (#2463)"
    );
    assert_eq!(
        glsl_fields, rust_fields,
        "GpuTerrainTile GLSL/Rust field lists diverged (GLSL {glsl_fields:?} vs Rust \
         {rust_fields:?}). The SSBO is sized and memcpy'd with the RUST stride while \
         the shader indexes with the GLSL one, so drift here corrupts terrain splat \
         texture indices on every exterior cell with nothing failing (#2463)."
    );
}

/// Regression for #2916 (REN-D2-01) — every secondary-ray terminus must
/// derive its surface colour through `rayHitAlbedo(mat, baseRgb)`
/// (`texel × mat.diffuse*`), never through `GpuInstance.avgAlbedo*`.
///
/// `avg_albedo_*` stopped being the material tint at #1628 (`93add433`):
/// `draw.rs` now uploads `draw_cmd.avg_albedo * handle_avg_rgb(texture)`,
/// i.e. `diffuse_color × the diffuse texture's MEAN texel`. The IOR
/// refraction terminus in `triangle.frag` still multiplied its own
/// `textureLod` sample by that value, so the texture entered the product
/// twice and everything seen through refractive glass rendered ~2–5× too
/// dark relative to the same surface seen directly or in a mirror.
///
/// This is pinned as a source assertion rather than a numeric one because
/// the failure is invisible to every runtime harness available: the
/// `--cornell` reference scene is untextured, and `handle_avg_rgb` returns
/// `None` for untextured handles, which collapses the double-multiply to
/// identity on exactly that content.
#[test]
fn refraction_terminus_tints_through_ray_hit_albedo_not_instance_avg_albedo() {
    let src = include_str!("../../../shaders/triangle.frag");

    assert!(
        src.contains("vec3 tColor = rayHitAlbedo(tMat, tAlbedo);"),
        "the IOR refraction terminus must tint its texel sample with the hit \
         material's own diffuse colour via rayHitAlbedo, the same helper \
         traceReflection / the GI bounce / traceWaterRay / \
         traceShadowTransmittance all use (#2916)"
    );

    // The prohibition is the load-bearing half: avgAlbedo may still be
    // *mentioned* in commentary, but no executable read of the field may
    // remain in this shader. `caustic_splat.comp` is the field's remaining
    // legitimate consumer (set 0, not migrated) and is out of scope here.
    let executable: String = src
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !executable.contains("avgAlbedo"),
        "triangle.frag must not read GpuInstance.avgAlbedo* — since #1628 the \
         field is `diffuse_color × texel-mean`, so multiplying a sampled texel \
         by it counts the texture twice (#2916)"
    );
}
