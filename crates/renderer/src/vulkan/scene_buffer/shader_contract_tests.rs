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

/// #2265 / TD7-001 — the 8-layer ray-walk budget shared by
/// `traceReflection` (raytrace.glsl), water's foliage-cutout skip walk
/// (water.frag), and `traceShadowTransmittanceDetailed`
/// (shadow_transport.glsl) must come from the single `MAX_ALPHA_SKIP_LAYERS`
/// GLSL macro, not three independently hand-declared local constants that
/// can silently drift out of sync.
#[test]
fn alpha_skip_layer_budget_is_a_single_shared_constant() {
    let header = include_str!("../../../shaders/include/shader_constants.glsl");
    assert!(header.contains("#define MAX_ALPHA_SKIP_LAYERS 8u"));

    let raytrace = include_str!("../../../shaders/include/raytrace.glsl");
    let water = include_str!("../../../shaders/water.frag");
    let shadow_transport = include_str!("../../../shaders/include/shadow_transport.glsl");
    for (name, src) in [
        ("raytrace.glsl", raytrace),
        ("water.frag", water),
        ("shadow_transport.glsl", shadow_transport),
    ] {
        assert!(
            src.contains("int(MAX_ALPHA_SKIP_LAYERS)"),
            "{name} must bound its ray-walk loop with the shared MAX_ALPHA_SKIP_LAYERS"
        );
        assert!(
            !src.contains("MAX_TRANSPARENT_SKIPS") && !src.contains("MAX_OPAQUE_LAYERS"),
            "{name} must not re-introduce a locally hand-declared alpha-skip cap"
        );
    }
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
    // #3282 / TD1-2026-08-24-01 — the selected-ray ARM + its shared
    // host-to-shader barrier moved from `draw.rs` into
    // `build_and_upload_instances.rs`; the PUBLISH barrier (below) stays in
    // `draw_frame`'s own tail in `draw.rs`, untouched by that split.
    let build_instances = include_str!("../context/build_and_upload_instances.rs");
    let draw = include_str!("../context/draw.rs");

    assert!(bindings.contains("binding = 19) coherent buffer SelectedRayProbeBuffer"));
    assert!(shadow.contains("traceShadowTransmittanceDetailed("));
    assert!(lighting.contains("traceLightTransmittanceDetailed("));
    assert!(triangle.contains("atomicCompSwap(selectedRayProbeControl.y, 1u, 2u)"));
    assert!(triangle.contains("selectedRayProbeLightParams = lights[selectedLightDebug].params"));
    assert!(triangle.contains("atomicExchange(selectedRayProbeControl.y, 3u)"));
    let arm_barrier = build_instances
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
    use crate::shader_constants::{MESH_ID_NO_HISTORY_BIT, MESH_ID_STABLE_MASK};

    let src = include_str!("../../../shaders/triangle.frag");
    assert!(
        src.contains("uint stableSurfaceId = inst.surfaceId & MESH_ID_STABLE_MASK;")
            && src.contains("alphaBlendFrag ? sortedInstanceId : stableSurfaceId"),
        "opaque TAA/SVGF history must use stable identity while alpha caustics keep the current draw index"
    );

    // #3881 — the two halves of the attachment ABI must stay complementary.
    // Previously the mask was the literal `0x7FFFFFFFu` at three sites and
    // was never expressed as the complement of the flag bit, so the two
    // could drift apart while every string assertion still matched.
    assert_eq!(
        MESH_ID_STABLE_MASK, !MESH_ID_NO_HISTORY_BIT,
        "the stable-ID lane must be exactly the complement of the no-history bit"
    );
    assert_eq!(
        MESH_ID_NO_HISTORY_BIT.count_ones(),
        1,
        "the no-history flag must be a single bit — the rest of the word is the ID"
    );
    assert!(
        src.contains("alphaBlendFrag ? MESH_ID_NO_HISTORY_BIT : 0u"),
        "the writer must set the flag by name, not by a hand-typed literal"
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
        "rayHitAlbedo(mat, uv, baseSample.rgb, 0.0)",
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

/// Regression for #3902 — `rayHitAlbedo` pre-fix applied only the constant
/// diffuse tint, dropping the five other texture-driven albedo-modifying
/// roles the raster path (`triangle.frag`) composes into `texColor`/
/// `albedo`: `decals[0..3]`, `tint`, `inner_layer`, `dark`, `detail`. RT
/// reflections/GI/water refraction therefore shaded a different surface
/// colour than the primary pass shows for the same surface. Pins that
/// `rayHitAlbedo` now samples all five, in the raster path's own order,
/// AND that every one of its four call sites was updated to the new
/// `(mat, uv, baseRgb, lod)` signature (the roles need `uv`/`lod` to
/// sample from — the pre-fix `(mat, baseRgb)` signature couldn't have
/// composed them at all).
#[test]
fn ray_hit_albedo_composes_every_raster_albedo_role() {
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");

    assert!(
        hit.contains("vec3 rayHitAlbedo(GpuMaterial mat, vec2 uv, vec3 baseRgb, float lod) {"),
        "rayHitAlbedo must take uv + an explicit lod to sample the \
         additional albedo-modifying roles — the pre-fix (mat, baseRgb) \
         signature had no way to."
    );

    let fn_start = hit
        .find("vec3 rayHitAlbedo(GpuMaterial mat")
        .expect("rayHitAlbedo definition must exist");
    let fn_body = &hit[fn_start..];
    let fn_end = fn_body
        .find("\nvec3 rayHitEmission(")
        .expect("rayHitAlbedo body must be followed by rayHitEmission");
    let fn_body = &fn_body[..fn_end];

    for (role, needle) in [
        ("decals", "mat.decalMap0Index"),
        ("decals", "mat.decalMap1Index"),
        ("decals", "mat.decalMap2Index"),
        ("decals", "mat.decalMap3Index"),
        (
            "diffuse tint",
            "vec3(mat.diffuseR, mat.diffuseG, mat.diffuseB)",
        ),
        ("tint map", "mat.tintMapIndex"),
        ("inner layer", "mat.innerLayerMapIndex"),
        (
            "inner layer kind gate",
            "MATERIAL_KIND_MULTI_LAYER_PARALLAX",
        ),
        ("dark map", "mat.darkMapIndex"),
        ("detail map", "mat.detailMapIndex"),
    ] {
        assert!(
            fn_body.contains(needle),
            "rayHitAlbedo is missing the {role} role (expected `{needle}`) — \
             a secondary ray would shade a different colour than the raster \
             pass on any surface using it (#3902)"
        );
    }
    // Every additional sample must use the caller-supplied explicit LOD,
    // not a hardcoded 0.0 — a blurred base sample (rough reflections,
    // refraction mip floor) must composite correspondingly blurred
    // decal/tint/inner-layer/dark/detail texels.
    assert_eq!(
        fn_body.matches("textureLod(").count(),
        5,
        "rayHitAlbedo must have exactly 5 additional-role sample call sites \
         (decals, tint, inner layer, dark, detail — one source-level \
         textureLod(...) call each; the decal loop's single call site \
         executes up to 4 times at runtime) via explicit-LOD textureLod, \
         matching sampleRayHitBase/rayHitEmission's existing secondary-ray \
         discipline. A different count means a role was dropped or a new \
         one was added without updating this pin."
    );

    // Every call site must thread uv + lod through, not the stale 2-arg form.
    for (file, src) in [
        (
            "raytrace.glsl",
            include_str!("../../../shaders/include/raytrace.glsl"),
        ),
        ("water.frag", include_str!("../../../shaders/water.frag")),
        (
            "triangle.frag",
            include_str!("../../../shaders/triangle.frag"),
        ),
    ] {
        assert!(
            !src.contains("rayHitAlbedo(mat, baseSample.rgb)")
                && !src.contains("rayHitAlbedo(hitMat, hitBaseRgb)")
                && !src.contains("rayHitAlbedo(tMat, tAlbedo)")
                && !src.contains("rayHitAlbedo(hitMat, hitBase.rgb)"),
            "{file} must not call the pre-#3902 2-argument rayHitAlbedo form"
        );
    }
    assert_eq!(
        include_str!("../../../shaders/include/raytrace.glsl")
            .matches("rayHitAlbedo(")
            .count()
            + include_str!("../../../shaders/water.frag")
                .matches("rayHitAlbedo(")
                .count()
            + include_str!("../../../shaders/triangle.frag")
                .matches("rayHitAlbedo(")
                .count(),
        4,
        "expected exactly 4 rayHitAlbedo call sites (raytrace.glsl GI/reflection, \
         water.frag, triangle.frag refraction + GI bounce) — a new secondary-ray \
         terminus that samples albedo without this helper reintroduces #3902's \
         drift for that path"
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
    // #3879 — the clamp ceilings now come from the generated header rather
    // than being retyped as literals, so match that spelling and pin the
    // relationship separately: tier 0 must be a true zero and every active
    // tier must sit at or under the shader ceiling.
    assert!(
        triangle.contains("clamp(rayBudget.maxPathSegments, 1u, uint(MAX_PATH_SEGMENTS))")
            && triangle.contains("clamp(rayBudget.maxShadedHits, 1u, uint(MAX_SHADED_HITS))"),
        "active GI tiers still require bounded non-zero loop limits"
    );

    use crate::vulkan::scene_buffer::ray_budget::AdaptiveRayBudget;
    let floor = AdaptiveRayBudget::settings_for_tier(0);
    assert_eq!(
        (floor.max_path_segments, floor.max_shaded_hits),
        (0, 0),
        "tier zero must upload true zeros — the outer gate is what makes it a \
         no-ray floor, since the inner clamps would turn any non-zero value \
         back into a one-segment workload"
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
        // #3530 — masked before the bail test; sampling
        // `textures[0x8000000N]` would be a wildly out-of-bounds bindless
        // index, so the mask here is load-bearing, not cosmetic.
        "uint parallaxIdx = mat.parallaxMapIndex & ~PARALLAX_ALPHA_HEIGHT_BIT;",
        "parallaxIdx == 0u",
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
        frag.contains("vec3 reflectionMiss = sceneFlags.yzw;")
            && frag.contains("float skyWeight = smoothstep(-0.2, 0.8, R.y);")
            && frag.contains("reflectionMiss = mix(sceneFlags.yzw, skyTint.xyz, skyWeight);"),
        "water reflection misses must use a directional outdoor sky gradient and
         cell ambient indoors."
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

/// #2756 / REN-D10-05 regression. `ssao.comp`'s `cameraPos` UBO field is
/// fed `ssao_cam_rel = camera_pos - render_origin` from the host
/// (post_passes.rs) — camera-RELATIVE, in the same frame as `viewProj` /
/// `invViewProj` — even though the field used to be commented as absolute
/// "camera world position". The shader math was always correct (every use
/// is a same-frame difference), but the stale comment is exactly what a
/// future author reads before wiring in a new absolute-space consumer.
/// Static source check: the SSAOParams declaration must document the
/// field as camera-relative, and the host upload site must still compute
/// it that way.
#[test]
fn ssao_camera_pos_is_documented_as_render_origin_relative() {
    let shader_src = include_str!("../../../shaders/ssao.comp");
    assert!(
        shader_src.contains("CAMERA-RELATIVE camera position"),
        "ssao.comp: SSAOParams.cameraPos must be documented as camera-\
             relative (`ssao_cam_rel = camera_pos - render_origin`), not \
             absolute — see #2756 / REN-D10-05."
    );
    assert!(
        !shader_src.contains("xyz = camera world position"),
        "ssao.comp: the stale \"camera world position\" (absolute) wording \
             for SSAOParams.cameraPos must not come back."
    );

    let host_src = include_str!("../context/post_passes.rs");
    assert!(
        host_src.contains("let ssao_cam_rel = [")
            && host_src.contains("camera_pos[0] - render_origin.x"),
        "post_passes.rs: the SSAO dispatch must still feed a render-origin-\
             relative camera position, matching the shader's documented \
             contract."
    );
}

/// #1642 / #2756 (REN-D10-05) regression. `triangle.frag`'s soft-particle
/// depth-fade path reconstructs `sceneWorld`/`fragSceneWorld` via the
/// render-origin-RELATIVE `invViewProj`, but `cameraPos.xyz` (CameraUBO) is
/// ABSOLUTE — the two must not be differenced directly, or the result is
/// dominated by `|renderOrigin|` in large exterior worldspaces. Static
/// source check: the rebase (`camRel = cameraPos.xyz - renderOrigin.xyz`)
/// must exist and both `length()` gap terms must use `camRel`, never the
/// raw absolute `cameraPos`.
#[test]
fn triangle_frag_soft_particle_rebases_camera_before_depth_gap() {
    let src = include_str!("../../../shaders/triangle.frag");
    assert!(
        src.contains("vec3 camRel = cameraPos.xyz - renderOrigin.xyz;"),
        "triangle.frag: expected the soft-particle depth-fade path to \
             rebase the camera into render-origin-relative space \
             (`camRel = cameraPos.xyz - renderOrigin.xyz`) before \
             differencing against `sceneWorld`/`fragSceneWorld` — see \
             #1642 / #2756."
    );
    assert!(
        src.contains("length(sceneWorld - camRel)")
            && src.contains("length(fragSceneWorld - camRel)"),
        "triangle.frag: the soft-particle depth gap must difference the \
             RELATIVE `camRel`, not the absolute `cameraPos`, against \
             `sceneWorld`/`fragSceneWorld` — see #1642 / #2756."
    );
}

/// #2777 / REN-D2-01 regression. Both ReSTIR reuse depth-compatibility
/// gates compare a `packHalf2x16`-clamped history depth (65504.0 max —
/// the largest finite half-float) against the current frame's UNCLAMPED
/// f32 `worldDist`. Without clamping `worldDist` the same way on read,
/// the comparison provably fails for every pixel past ~65504 BU (both
/// this-frame and history depth exceed the clamp — reuse goes silently
/// inert exactly where distant exteriors need it). Static source check:
/// both `*DepthCompatible` comparisons must clamp `worldDist` before
/// differencing.
#[test]
fn restir_depth_compatibility_gates_clamp_world_dist_to_match_packed_history() {
    let src = include_str!("../../../shaders/triangle.frag");
    let cases = [
        (
            "temporalDepthCompatible",
            "abs(rpHistoryDepth.y - min(worldDist, 65504.0)) <= temporalDepthTolerance",
        ),
        (
            "spatialDepthCompatible",
            "abs(rnWorldDist - min(worldDist, 65504.0)) <= spatialDepthTolerance",
        ),
    ];
    for (name, needle) in cases {
        assert!(
            src.contains(needle),
            "triangle.frag: `{name}` must clamp `worldDist` to \
                 `min(worldDist, 65504.0)` before differencing against the \
                 packHalf2x16-clamped history depth (expected to find: \
                 `{needle}`) — see #2777 / REN-D2-01."
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
/// shared secondary-hit include plus its triangle fragment consumer
/// for any forbidden offset — `+ 12` through `+ 15` (bone
/// indices) or `+ 20` / `+ 21` (splat weights) — that ISN'T
/// wrapped in `floatBitsToUint(…)` or `unpackUnorm4x8(…)`.
///
/// `skin_vertices.comp` reads bone indices but does so through
/// `floatBitsToUint`; the check excludes that pattern.
///
/// #4018 (REN-2026-09-06-D2-03) — the source list is DISCOVERED, not
/// hardcoded. It used to be a literal array, and by the time this was filed
/// it covered three of the eight shaders that index a raw-float vertex SSBO:
/// `caustic_splat.comp` (its own private `GlobalVertices` at set 0 binding 9),
/// `volumetrics_inject.comp` (`boundaryVertexData`), and the three
/// ground-cover stages had all drifted outside it. Nothing failed when they
/// did, because the test had no completeness half — the same shape as #3829,
/// where a hardcoded shader list without one let `volumetrics_inject.comp`
/// sit outside a GPU-layout contract for 13 days and cost a CRITICAL.
///
/// The walk is two passes so a new *buffer name* is picked up as well as a new
/// reader: pass 1 collects every `float <name>[];` array declaration whose
/// name contains `ertex` (`vertexData`, `boundaryVertexData`, and whatever a
/// future skinned or instanced variant is called), pass 2 puts every file that
/// indexes one of those names under the guard.
#[test]
fn rt_hit_shaders_have_no_unsafe_vertex_data_reads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut files = Vec::new();
    collect_shader_files(&root, &mut files);
    files.sort();

    let loaded: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            let name = path
                .strip_prefix(&root)
                .expect("under shaders root")
                .to_string_lossy()
                .replace('\\', "/");
            (
                name,
                std::fs::read_to_string(path).expect("shader readable"),
            )
        })
        .collect();

    // Pass 1 — the raw-float vertex array names actually declared anywhere.
    let mut array_names: Vec<String> = Vec::new();
    for (_, src) in &loaded {
        for raw in src.lines() {
            let code = match raw.find("//") {
                Some(i) => &raw[..i],
                None => raw,
            };
            let Some(rest) = code.trim_start().strip_prefix("float ") else {
                continue;
            };
            let Some(name) = rest.split_once("[]").map(|(n, _)| n.trim()) else {
                continue;
            };
            if name.is_empty() || !name.contains("ertex") {
                continue;
            }
            if !array_names.iter().any(|n| n == name) {
                array_names.push(name.to_owned());
            }
        }
    }
    assert!(
        array_names.len() >= 2,
        "the vertex-SSBO name scan found only {array_names:?} — the extraction \
         broke, not the shaders. At minimum `vertexData` (the global bindless \
         buffer) and `boundaryVertexData` (volumetrics) are declared."
    );

    // Pass 2 — every file that indexes one of them is under the guard.
    let sources: Vec<(&str, &str)> = loaded
        .iter()
        .filter(|(_, src)| {
            array_names
                .iter()
                .any(|name| src.contains(&format!("{name}[")))
        })
        .map(|(name, src)| (name.as_str(), src.as_str()))
        .collect();
    // The floor that separates "the walk found nothing" from "nothing is
    // wrong". Deliberately NOT including `triangle.frag`: it was in the old
    // hardcoded list, but it reaches vertex data through `ray_hit.glsl`'s
    // helpers and indexes no array itself — the walk is self-correcting there,
    // since a direct read added to it later selects it automatically.
    for required in [
        "include/ray_hit.glsl",
        "include/terrain_sample.glsl",
        "caustic_splat.comp",
        "volumetrics_inject.comp",
    ] {
        assert!(
            sources.iter().any(|(name, _)| *name == required),
            "{required} indexes a raw-float vertex SSBO but the discovery walk \
             did not select it — the walk broke, not the shader. Selected: {:?}",
            sources.iter().map(|(n, _)| *n).collect::<Vec<_>>()
        );
    }

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
/// #3983 (REN-2026-09-06-D17-01) — the specular-AA filter takes screen-space
/// derivatives, so it must be evaluated in the widest control flow its inputs
/// allow, never inside the per-light loop.
///
/// `shadowableLightRadiance` used to call `specularAaRoughness(N, roughness)`
/// itself, and all five of its call sites in `triangle.frag` sit behind
/// per-invocation-divergent predicates: the cluster loop's
/// `contribution < 0.001` continue, the ReSTIR temporal and spatial reuse
/// gates, the selected-light block, and the legacy-WRS ray-query test.
/// GLSL/SPIR-V leave `dFdx`/`dFdy` undefined there — the class #3622 fixed in
/// `parallaxDisplaceUV`. Both inputs are branch-invariant, so the call was
/// also pure waste: a 16-light cluster ran the derivative chain sixteen times
/// for sixteen identical values, forcing the quad into lockstep at each one.
///
/// Two legs, because either alone rots: the filter must be gone from the
/// per-light function, AND `triangle.frag` must actually compute it at
/// function scope. A hoist that left the call behind would pass the first.
#[test]
fn the_specular_aa_filter_is_evaluated_outside_the_per_light_loop() {
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let frag = include_str!("../../../shaders/triangle.frag");

    let body = glsl_fn_body(lighting, "vec3 shadowableLightRadiance(");
    // Composed at runtime so this test's own source cannot satisfy the scan.
    let filter = format!("specularAa{}", "Roughness(");
    assert!(
        !body.contains(filter.as_str()),
        "shadowableLightRadiance evaluates the specular-AA filter itself, so \
         its dFdx/dFdy run inside per-invocation-divergent control flow at \
         every call site — hoist it to uniform flow in triangle.frag and pass \
         the result in (#3983)"
    );
    // The signature, not the body: `glsl_fn_body` starts after the brace.
    assert!(
        lighting.contains("float roughness, float aaRoughness,"),
        "shadowableLightRadiance must take the filtered roughness as a \
         parameter alongside the raw one — disneyDiffuseSplit deliberately \
         uses the unfiltered value, so they cannot be collapsed (#3983)"
    );

    // The hoisted call must sit at function scope in main (4-space indent),
    // not nested inside a branch that would reintroduce the divergence.
    let hoisted = frag
        .lines()
        .find(|l| l.contains(filter.as_str()) && !l.trim_start().starts_with("//"))
        .expect("triangle.frag must compute the specular-AA roughness (#3983)");
    let indent = hoisted.len() - hoisted.trim_start().len();
    assert!(
        indent <= 8,
        "the hoisted specularAaRoughness call is indented {indent} spaces, so \
         it sits inside nested control flow — the whole point of #3983 is that \
         it runs where every lane of the quad reaches it"
    );
}

/// #3982 (REN-2026-09-06-D16-01) — the three name-diverging GLSL↔Rust mirrors
/// that had no lockstep coverage at all.
///
/// `shader_sources_declaring` finds mirrors by literal declaration string
/// (`"struct GpuInstance"`, …), so a GLSL struct that mirrors a Rust struct
/// under a *different* name is invisible to it. #3829 was that failure:
/// `GpuBoundaryInstance` ran 13 days at a 128 B stride against a 160 B
/// `GpuInstance`, green suite throughout, because it wore its own name. Its
/// fix guarded that one struct. `fa5c4191`'s commit message even records
/// seeing `FogClusterEntry` and `CombustionLightMoment` as the same shape —
/// verified in sync once, then not guarded — and `ClusterEntry`, which has
/// **three** GLSL copies, was not part of even that check.
///
/// `CombustionLightMoment` needs the field-ORDER leg, not just a stride: the
/// shader writes by name (`atomicAdd(combustionLightMoments[b].weighted_x, …)`)
/// while `decode_combustion_light_moment` decodes positionally by word index,
/// so a GLSL-side reorder compiles clean, stays 32 B, and silently swaps a
/// luma-weighted centroid for a radiant channel.
#[test]
fn name_diverging_glsl_rust_mirrors_stay_in_lockstep() {
    let volumetrics_comp = include_str!("../../../shaders/volumetrics_inject.comp");
    let cluster_cull = include_str!("../../../shaders/cluster_cull.comp");
    let bindings = include_str!("../../../shaders/include/bindings.glsl");
    let volumetrics_rs = include_str!("../volumetrics.rs");
    let compute_rs = include_str!("../compute.rs");

    for (label, glsl_src, glsl_decl, rust_src, rust_decl) in [
        // Three GLSL copies against one Rust struct — comparing each to the
        // same reference also proves the three agree with each other, which is
        // the multi-copy shape `GpuInstance` and `GpuLight` each already have
        // a test for.
        (
            "ClusterEntry (cluster_cull.comp, the writer)",
            cluster_cull,
            "struct ClusterEntry",
            compute_rs,
            "struct ClusterEntry",
        ),
        (
            "ClusterEntry (bindings.glsl, the fragment reader)",
            bindings,
            "struct ClusterEntry",
            compute_rs,
            "struct ClusterEntry",
        ),
        (
            "ClusterEntry (volumetrics_inject.comp, the fog reader)",
            volumetrics_comp,
            "struct ClusterEntry",
            compute_rs,
            "struct ClusterEntry",
        ),
        (
            "FogClusterEntry",
            volumetrics_comp,
            "struct FogClusterEntry",
            volumetrics_rs,
            "struct GpuFogClusterEntry",
        ),
        (
            "CombustionLightMoment",
            volumetrics_comp,
            "struct CombustionLightMoment",
            volumetrics_rs,
            "struct GpuCombustionLightMoment",
        ),
    ] {
        let glsl = parse_glsl_struct_fields_typed(glsl_src, glsl_decl);
        let rust = parse_rust_struct_fields_typed(rust_src, rust_decl);
        assert!(
            !glsl.is_empty() && !rust.is_empty(),
            "{label}: one side parsed to zero fields — the declaration moved \
             or was renamed, so this guard is checking nothing (#3982)"
        );
        assert_eq!(
            glsl.len(),
            rust.len(),
            "{label}: {} GLSL fields vs {} Rust fields (#3982)",
            glsl.len(),
            rust.len()
        );
        // Both parsers return `(type, name)`.
        for (i, ((glsl_ty, glsl_name), (rust_ty, rust_name))) in
            glsl.iter().zip(rust.iter()).enumerate()
        {
            assert_eq!(
                normalize_ident(glsl_name),
                normalize_ident(rust_name),
                "{label}: field {i} is `{glsl_name}` in GLSL but `{rust_name}` in \
                 Rust — order or naming drifted (#3982)"
            );
            assert!(
                rust_glsl_scalar_type_matches(rust_ty, glsl_ty),
                "{label}: field `{glsl_name}` is `{glsl_ty}` in GLSL but \
                 `{rust_ty}` in Rust (#3982)"
            );
        }
    }

    // The stride leg, against the sizes the Rust side asserts for itself.
    assert_eq!(
        std430_struct_size(&parse_glsl_struct_fields_typed(
            volumetrics_comp,
            "struct CombustionLightMoment"
        )),
        32,
        "CombustionLightMoment's std430 stride left 32 B — \
         `combustion_light_moment_abi_is_eight_std430_words` pins the Rust \
         side at 32 and only the Rust side (#3982)"
    );
    assert_eq!(
        std430_struct_size(&parse_glsl_struct_fields_typed(
            volumetrics_comp,
            "struct FogClusterEntry"
        )),
        8,
        "FogClusterEntry's std430 stride left 8 B — a wrong `count` decode is \
         what makes a stale cluster live under the #3834 partial-upload \
         contract (#3982)"
    );
}

/// How each GLSL struct relates to the Rust tree. See
/// [`every_shader_struct_is_classified`].
enum MirrorClass {
    /// A Rust counterpart exists and a named test compares the two.
    Guarded(&'static str),
    /// A Rust counterpart exists, but no field-level comparison yet. Carries
    /// the specific blocker, not a shrug.
    MirroredPendingGuard(&'static str),
    /// No Rust counterpart — a GLSL-local working type.
    ShaderLocal,
}

/// #3982 step 2 — close the *discovery* gap, so a name-diverging mirror
/// cannot appear unnoticed the way five of them already did.
///
/// The hand-written `SOURCES` tables and `shader_sources_declaring` between
/// them only ever cover structs someone thought to name. This walks every
/// `struct` declared under `crates/renderer/shaders/` and requires each one to
/// be classified — which converts "someone remembered" into "someone had to
/// decide", the only version of this guard that survives the next struct.
///
/// The buckets are load-bearing, not labels: a `Guarded` entry must name a
/// test that still exists, and a `ShaderLocal` entry must really have no Rust
/// counterpart. That second check is a heuristic — it looks for a Rust
/// declaration of `<Name>` or `Gpu<Name>`, which is the naming convention this
/// tree actually uses — so it proves the common case and not every case.
#[test]
fn every_shader_struct_is_classified() {
    use MirrorClass::*;
    const CLASSIFIED: &[(&str, MirrorClass)] = &[
        (
            "GpuInstance",
            Guarded("gpu_instance_glsl_copies_stay_in_lockstep"),
        ),
        (
            "GpuLight",
            Guarded("gpu_light_glsl_copies_stay_in_lockstep"),
        ),
        (
            "GpuMaterial",
            Guarded("gpu_material_glsl_field_order_matches_rust_struct"),
        ),
        (
            "GpuTerrainTile",
            Guarded("gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep"),
        ),
        (
            "WaterParams",
            Guarded("gpu_water_params_rust_and_glsl_copies_stay_in_lockstep"),
        ),
        (
            "GpuFogVolume",
            Guarded("gpu_fog_volume_glsl_field_order_matches_rust_struct"),
        ),
        (
            "GpuBoundaryInstance",
            Guarded("gpu_boundary_instance_stride_matches_gpu_instance"),
        ),
        (
            "ClusterEntry",
            Guarded("name_diverging_glsl_rust_mirrors_stay_in_lockstep"),
        ),
        (
            "FogClusterEntry",
            Guarded("name_diverging_glsl_rust_mirrors_stay_in_lockstep"),
        ),
        (
            "CombustionLightMoment",
            Guarded("name_diverging_glsl_rust_mirrors_stay_in_lockstep"),
        ),
        // Found by this very walk, after #3982 was filed — the ground-cover
        // stratum (#4054-#4058) landed five more name-diverging mirrors. They
        // are NOT guarded: each Rust side carries explicit `pad*` fields that
        // the GLSL side leaves to std430's implicit padding, so the
        // field-name-and-order comparator above reports a length mismatch on
        // structs that are actually in sync. Guarding them needs a
        // padding-aware comparison, which is its own piece of work.
        (
            "GroundCoverCell",
            MirroredPendingGuard("GpuGroundCoverCell pads explicitly (pad0, pad1)"),
        ),
        (
            "GroundCoverChunk",
            MirroredPendingGuard("sibling of GroundCoverCell"),
        ),
        (
            "GroundCoverSpecies",
            MirroredPendingGuard("sibling of GroundCoverCell"),
        ),
        (
            "BenchCell",
            MirroredPendingGuard("GpuBenchCell, groundcover_bench.rs"),
        ),
        (
            "BenchChunk",
            MirroredPendingGuard("GpuBenchChunk, groundcover_bench.rs"),
        ),
        // Rust holds a byte stride (`RESERVOIR_BYTES`), not a mirrored struct.
        (
            "Reservoir",
            MirroredPendingGuard("Rust side is a stride constant, not a struct"),
        ),
        ("GroundCoverFactors", ShaderLocal),
        ("GroundCoverBlade", ShaderLocal),
        ("GcDrawIndirect", ShaderLocal),
        ("TerrainSample", ShaderLocal),
        ("LocalMedium", ShaderLocal),
        ("DisneyDiffuseSplit", ShaderLocal),
        ("CombustionDifferential", ShaderLocal),
    ];

    // ---- the walk
    let shaders = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut declared: Vec<String> = Vec::new();
    let mut stack = vec![shaders.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("shader dir readable") {
            let path = entry.expect("shader dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("vert" | "frag" | "comp" | "glsl")
            ) {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in src.lines() {
                if let Some(rest) = line.strip_prefix("struct ") {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() && !declared.contains(&name) {
                        declared.push(name);
                    }
                }
            }
        }
    }
    declared.sort();

    let mut listed: Vec<String> = CLASSIFIED.iter().map(|(n, _)| (*n).to_string()).collect();
    listed.sort();
    assert_eq!(
        declared, listed,
        "the shader tree declares {declared:?} but the classification table \
         lists {listed:?}. Every GLSL struct must be classified as Guarded, \
         MirroredPendingGuard, or ShaderLocal — an unclassified one is exactly \
         how #3829's GpuBoundaryInstance and #3982's five ground-cover mirrors \
         reached HEAD unnoticed (#3982)"
    );

    // ---- the buckets have to mean something
    let renderer_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_sources = String::new();
    let mut stack = vec![renderer_src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src dir readable") {
            let path = entry.expect("src dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(src) = std::fs::read_to_string(&path) {
                    rust_sources.push_str(&src);
                    rust_sources.push('\n');
                }
            }
        }
    }
    let declares_rust_struct = |name: &str| {
        rust_sources.lines().any(|raw| {
            let code = raw.trim_start();
            if code.starts_with("//") {
                return false;
            }
            let code = code.strip_prefix("pub ").unwrap_or(code);
            code.strip_prefix("struct ")
                .and_then(|rest| rest.strip_prefix(name))
                .is_some_and(|rest| rest.starts_with([' ', '{', '(', ';', '<']))
        })
    };

    for (name, class) in CLASSIFIED {
        match class {
            Guarded(test) => assert!(
                rust_sources.contains(&format!("fn {test}(")),
                "`{name}` is classified as guarded by `{test}`, but no such test \
                 exists in the renderer crate — the guard was renamed or deleted \
                 and the classification now vouches for nothing (#3982)"
            ),
            // The bucket that could become a rubber stamp, so it is the one
            // that has to carry a reason someone can act on.
            MirroredPendingGuard(blocker) => assert!(
                !blocker.is_empty(),
                "`{name}` is parked as a mirror without a guard but names no \
                 blocker — an empty reason is how this bucket turns into a \
                 place to put things (#3982)"
            ),
            ShaderLocal => assert!(
                !declares_rust_struct(name) && !declares_rust_struct(&format!("Gpu{name}")),
                "`{name}` is classified as shader-local, but the renderer crate \
                 declares a matching Rust struct — it is a mirror now and needs a \
                 lockstep guard, which is the drift this table exists to catch \
                 (#3982)"
            ),
        }
    }
}

/// #3986 (REN-2026-09-06-D2-01) — the secondary-ray coverage test must build
/// its alpha from the same chain the raster path does, and both must run the
/// same comparison table.
///
/// `getHitVertexAlpha`'s own docstring states the invariant: "secondary rays
/// must reconstruct the same barycentric value or alpha-tested leaves/grates
/// cast a different silhouette from the visible surface." `rayHitHasCoverage`
/// reproduced every step of the raster chain except the four-slot decal
/// alpha-over composite. Because that composite only ever raises alpha, the RT
/// silhouette was a strict SUBSET of the visible one — shadow, reflection,
/// refraction, GI and water rays punching through covered texels.
///
/// The second leg is the comparison table itself. It was hand-written twice
/// and had already drifted: `triangle.frag` used a rounded `0.004`
/// EQUAL/NOTEQUAL epsilon where `alphaComparePass` uses an exact `1.0/255.0`.
/// One definition now, so it cannot drift a second time.
#[test]
fn ray_and_raster_coverage_share_one_alpha_chain() {
    let ray_hit = include_str!("../../../shaders/include/ray_hit.glsl");
    let frag = include_str!("../../../shaders/triangle.frag");

    let coverage = glsl_fn_body(ray_hit, "bool rayHitHasCoverage(");
    let decals = coverage
        .find("mat.decalMap0Index")
        .expect("rayHitHasCoverage must composite the decal slots (#3986)");
    let compare = coverage
        .find("alphaComparePass(")
        .expect("rayHitHasCoverage must still run the shared comparison table");
    assert!(
        decals < compare,
        "the decal composite must run BEFORE the alpha comparison, as the \
         raster path does — after it, the test sees the un-composited alpha \
         and the silhouettes diverge exactly as before (#3986)"
    );
    assert!(
        coverage.contains("textureLod("),
        "the decal samples must use an explicit LOD — a secondary-ray hit has \
         no quad and therefore no implicit derivatives (#3986)"
    );

    // One definition of the seven-arm table. The raster path must call the
    // shared helper, and must not carry its own epsilon.
    assert!(
        frag.contains("alphaComparePass(texColor.a, aThresh, mat.alphaTestFunc)"),
        "triangle.frag must run its per-instance alpha test through \
         alphaComparePass rather than hand-writing the table (#3986)"
    );
    // The hand-written table's signature, not a bare float literal: `0.004`
    // occurs legitimately elsewhere in this shader (glass optical thickness),
    // so scanning for the epsilon would fire on unrelated code — and on this
    // fix's own explanatory comment. `aFunc ==` appears only in the copy.
    let arm = format!("a{} ==", "Func");
    assert!(
        !frag.contains(arm.as_str()),
        "triangle.frag hand-writes the alpha-comparison arms again — \
         alphaComparePass is the one definition, and the epsilon is what \
         drifted last time (#3986)"
    );
}

/// Body of a GLSL function, by brace matching from its declaration.
fn glsl_fn_body<'a>(src: &'a str, decl: &str) -> &'a str {
    let start = src
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` must still exist"));
    let open = start
        + src[start..]
            .find('{')
            .unwrap_or_else(|| panic!("`{decl}` has no body"));
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces in `{decl}`");
}

/// #3984 (REN-2026-09-06-D17-02) — every Gram-Schmidt TBN builder in the
/// shader tree must test the length of the projected tangent, not just the
/// raw one.
///
/// `T = normalize(T - dot(T, N) * N)` is `0/0` when T is parallel to N, and
/// the anisotropic-GGX branch in `shadowableLightRadiance` was the one
/// builder of four without the test — its guard proved only that the raw
/// `fragTangent` was non-zero. `N` there is the normal-mapped shading normal,
/// so a strongly-perturbing normal map can rotate it onto the authored
/// tangent; that is the same reasoning behind `perturbNormal`'s guard
/// (#2815). Because `shadowableLightRadiance` is also the ReSTIR `pHat`
/// scorer and the legacy shadow subtrahend, a NaN there reaches the reservoir
/// (spatial reuse spreads it to neighbours) and the EMA history, where the
/// `mix()` recurrence can never clear it — a spreading blot, not a speck.
///
/// Enumerated rather than spot-checked: `material_sampling.glsl`'s comment
/// keeps the list of builders, and a fifth one added without a guard is
/// exactly how the fourth got missed.
#[test]
fn every_gram_schmidt_tangent_frame_guards_the_projected_length() {
    let material_sampling = include_str!("../../../shaders/include/material_sampling.glsl");
    let ray_hit = include_str!("../../../shaders/include/ray_hit.glsl");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");

    for (label, src, decl) in [
        // #4016 re-point — `perturbNormal`'s body moved to
        // `perturbNormalGrad` when the explicit-gradient core was extracted;
        // `perturbNormal` is now a thin wrapper that supplies the four
        // derivatives and delegates. The guard this enumerates lives with the
        // body, so the pin follows it (the shape `d9a7e75c` used for the
        // #3282 split). `the_perturb_normal_wrapper_only_delegates` below
        // keeps the wrapper from growing a second, unguarded frame.
        (
            "perturbNormalGrad",
            material_sampling,
            "vec3 perturbNormalGrad(",
        ),
        (
            "parallaxDisplaceUV",
            material_sampling,
            "vec2 parallaxDisplaceUV(",
        ),
        (
            "getRayHitTangentFrame",
            ray_hit,
            "bool getRayHitTangentFrame(",
        ),
        (
            "shadowableLightRadiance",
            lighting,
            "vec3 shadowableLightRadiance(",
        ),
    ] {
        let body = glsl_fn_body(src, decl);
        assert!(
            body.contains("1e-8"),
            "{label} builds a tangent frame by Gram-Schmidt but carries no \
             post-projection length test — `normalize()` of the collapsed \
             projection is 0/0 (#3984 / #2815)"
        );
    }

    // The anisotropic branch specifically: the guard must sit between the
    // projection and the lobe, not merely somewhere in the function.
    let aniso = glsl_fn_body(lighting, "vec3 shadowableLightRadiance(");
    let proj = aniso
        .find("vec3 Tproj = T - dot(T, N) * N;")
        .expect("the anisotropic branch must still Gram-Schmidt the tangent");
    let lobe = aniso
        .find("distributionGGXAniso(")
        .expect("the anisotropic lobe must still be reachable");
    let guard = aniso
        .find("dot(Tproj, Tproj) >= 1e-8")
        .expect("the anisotropic branch must test the projected length (#3984)");
    assert!(
        proj < guard && guard < lobe,
        "the projected-length guard must sit between the Gram-Schmidt \
         projection and the anisotropic lobe — outside that span it does not \
         stop the NaN from reaching distributionGGXAniso (#3984)"
    );
}

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
    parse_rust_struct_fields_typed(src, decl)
        .into_iter()
        .map(|(_, name)| name)
        .collect()
}

/// Ordered (type, name) pairs of a Rust `#[repr(C)]` struct — as
/// [`parse_rust_struct_fields`], but keeps the declared type instead of
/// discarding it. #2688 / SAFE-D6-01: the name-only parser can't catch a
/// `uint`<->`float` reinterpretation in the GLSL mirror that preserves
/// field order and struct size.
fn parse_rust_struct_fields_typed(src: &str, decl: &str) -> Vec<(String, String)> {
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
        if !is_ident(ident) {
            continue;
        }
        // RHS is `<Type>,` (field decl) with an optional trailing `// ...`
        // offset comment, e.g. `pub roughness: f32, // offset 0`.
        let rhs = match line[colon + 1..].find("//") {
            Some(i) => &line[colon + 1..][..i],
            None => &line[colon + 1..],
        };
        let ty = rhs.trim().trim_end_matches(',').trim();
        out.push((ty.to_string(), ident.to_string()));
    }
    out
}

/// Ordered field names of a GLSL struct declaration (e.g.
/// `"struct GpuMaterial"` / `"struct GpuInstance"`), parsed from a GLSL
/// source file. Handles multi-name declarations (`float a, b, c;`) and
/// skips `//`/`///` comment lines.
fn parse_glsl_struct_fields(src: &str, decl: &str) -> Vec<String> {
    parse_glsl_struct_fields_typed(src, decl)
        .into_iter()
        .map(|(_, name)| name)
        .collect()
}

/// Ordered (type, name) pairs of a GLSL struct declaration — as
/// [`parse_glsl_struct_fields`], but keeps the type token instead of
/// discarding it. #2688 / SAFE-D6-01: the name-only parser can't catch a
/// `uint`<->`float` reinterpretation that preserves field order and size.
fn parse_glsl_struct_fields_typed(src: &str, decl: &str) -> Vec<(String, String)> {
    const TYPES: &[&str] = &[
        "float", "uint", "int", "bool", "vec2", "vec3", "vec4", "mat2", "mat3", "mat4",
        // GpuInstance-only types (#2219 skinned_vertex_address + padding;
        // #3231 morph-target address/count fields). Deliberately no
        // `uvec3` — this struct's own padding avoids it (16-byte std430
        // alignment footgun, see gpu_types.rs), so a future field using
        // it would be a bug this parser should keep failing to see, not
        // silently accept.
        "uint64_t", "uvec2", "uvec4",
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
                out.push((ty.to_string(), id.to_string()));
            }
        }
    }
    out
}

/// The water record is duplicated in Rust and both shader stages. Keep the
/// field order and the growable std430 binding shape as one checked contract;
/// a size-only assertion cannot detect two same-sized fields being swapped.
#[test]
fn gpu_water_params_rust_and_glsl_copies_stay_in_lockstep() {
    let rust_src = include_str!("../water.rs");
    let vert_src = include_str!("../../../shaders/water.vert");
    let frag_src = include_str!("../../../shaders/water.frag");

    // #3564 — water.vert / water.frag are the only mirrors today; pin the set
    // so a third reader cannot be added outside this field-order comparison.
    assert_mirror_list_is_complete(
        "struct WaterParams",
        &[("water.vert", vert_src), ("water.frag", frag_src)],
        "#3564",
    );

    let rust_fields = parse_rust_struct_fields(rust_src, "pub struct GpuWaterParams");
    let vert_fields = parse_glsl_struct_fields(vert_src, "struct WaterParams");
    let frag_fields = parse_glsl_struct_fields(frag_src, "struct WaterParams");
    assert_eq!(
        rust_fields.len(),
        23,
        "GpuWaterParams must remain 23 vec4 slots"
    );
    assert_eq!(
        vert_fields, rust_fields,
        "water.vert WaterParams field order drifted"
    );
    assert_eq!(
        frag_fields, rust_fields,
        "water.frag WaterParams field order drifted"
    );

    const SSBO_DECL: &str = "layout(std430, set = 2, binding = 1) readonly buffer WaterParamsBlock";
    for (name, src) in [("water.vert", vert_src), ("water.frag", frag_src)] {
        assert!(
            src.contains(SSBO_DECL),
            "{name} must bind water params as std430 SSBO"
        );
        assert!(
            src.contains("WaterParams params[];"),
            "{name} must use an unsized array"
        );
        assert!(
            !src.contains("params[186]"),
            "{name} retained the retired fixed cap"
        );
    }
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
///
/// Also asserts, field-by-field, that the GLSL scalar type agrees with the
/// Rust type (#2688 / SAFE-D6-01). The name/order checks above pass on a
/// `uint`<->`float` reinterpretation as long as field order and count are
/// unchanged — that flip is byte-lethal for any field read through an
/// implicit widening conversion, and every `GpuMaterial` field is a bare
/// scalar (`f32`/`u32`; see the struct definition), so this check doesn't
/// need to handle vector/matrix types.
#[test]
fn gpu_material_glsl_field_order_matches_rust_struct() {
    let rust_src = include_str!("../material.rs");
    let glsl_src = include_str!("../../../shaders/include/bindings.glsl");

    let rust_typed = parse_rust_struct_fields_typed(rust_src, "pub struct GpuMaterial");
    let glsl_typed = parse_glsl_struct_fields_typed(glsl_src, "struct GpuMaterial");
    let rust_fields: Vec<String> = rust_typed.iter().map(|(_, n)| n.clone()).collect();
    let glsl_fields: Vec<String> = glsl_typed.iter().map(|(_, n)| n.clone()).collect();

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

    for (i, ((rust_ty, rust_name), (glsl_ty, glsl_name))) in
        rust_typed.iter().zip(glsl_typed.iter()).enumerate()
    {
        assert!(
            rust_glsl_scalar_type_matches(rust_ty, glsl_ty),
            "GpuMaterial field #{i} TYPE mismatch: Rust `{rust_name}: {rust_ty}` vs GLSL \
             `{glsl_ty} {glsl_name}`. Every GpuMaterial field is a bare scalar; a uint<->float \
             reinterpretation preserves field order and struct size but corrupts the value read \
             through the mismatched type — see #2688 / SAFE-D6-01.",
        );
    }
}

/// True if a Rust scalar field type and its GLSL mirror are the same bit
/// pattern's intended interpretation. #2688 / SAFE-D6-01 — covers the
/// scalar types `GpuMaterial` actually declares; extend if a future field
/// needs a vector/matrix type here.
///
/// #3684 (PERF-D4-2026-08-30-04) — extended with `GpuCamera`'s
/// fixed-size-array shapes: rustfmt always renders these with the exact
/// spacing matched here (`[f32; 4]`, `[[f32; 4]; 4]`), so this is a plain
/// string match, not a general array-type parser.
fn rust_glsl_scalar_type_matches(rust_ty: &str, glsl_ty: &str) -> bool {
    matches!(
        (rust_ty, glsl_ty),
        ("f32", "float")
            | ("u32", "uint")
            | ("i32", "int")
            | ("bool", "bool")
            | ("[f32; 4]", "vec4")
            | ("[u32; 4]", "uvec4")
            | ("[[f32; 4]; 4]", "mat4")
    )
}

/// Slice the body of a top-level function declared by `fn_decl` (e.g.
/// `"fn hash_gpu_material_fields"`) out of `src` — the text between the
/// function's own opening `{` and its MATCHING closing `}`, found by
/// brace-depth counting. Unlike [`extract_struct_body`]'s "first `}`"
/// shortcut (safe only because every struct body here is flat), a
/// function body isn't guaranteed brace-free — `hash_gpu_material_fields`
/// happens to be flat today, but this doesn't assume it stays that way.
fn extract_fn_body<'a>(src: &'a str, fn_decl: &str) -> Option<&'a str> {
    let start = src.find(fn_decl)?;
    let open = src[start..].find('{')? + start;
    let mut depth = 0i32;
    for (i, b) in src[open..].bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every `mat.<ident>` field access in `body`, in occurrence order
/// (duplicates included — the caller collects into a set). Deliberately
/// loose (a substring + identifier scan, not a real Rust parse), matching
/// the same "good enough for a source-scanning guard" posture as
/// [`parse_rust_struct_fields`] — `hash_gpu_material_fields`'s parameter
/// is always named `mat`, both here and in `DrawCommand::material_hash`.
fn extract_mat_field_accesses(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while let Some(rel) = body[i..].find("mat.") {
        let start = i + rel + "mat.".len();
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end > start {
            out.push(body[start..end].to_string());
        }
        i = end.max(start + 1);
    }
    out
}

/// Regression for #3568 (REN-2026-08-30-D7-01). `MaterialTable::intern_by_hash`
/// dedups solely on `hash_gpu_material_fields`'s u64: a `GpuMaterial` field
/// populated by `to_gpu_material` but omitted from the hash walk makes two
/// visually-different materials silently collapse onto one table slot — the
/// first-seen record wins and every later draw renders with the wrong value.
/// Release builds have no guard against this at all (the one live check,
/// `intern_by_hash`'s byte-equality `debug_assert!`, is debug-only and only
/// fires if such content is actually loaded that session).
///
/// The three pre-existing pins are all mutually blind to this exact defect:
/// `gpu_material_size_is_432_bytes` pins `size_of`, which a correctly-added-
/// but-unhashed field still satisfies; `gpu_material_glsl_field_order_matches_
/// rust_struct` compares the Rust struct against the GLSL mirror, and both
/// sides get updated in a normal field addition; `material_hash_matches_
/// gpu_material_field_hash` compares the two hash walks (this one and
/// `DrawCommand::material_hash`) against EACH OTHER, so it passes when a
/// field is missing from both.
///
/// Parses the struct's declared field names and the `mat.<ident>`
/// identifiers `hash_gpu_material_fields`'s body actually hashes, out of the
/// SAME `include_str!`'d source, and asserts the two sets are identical in
/// both directions: a field in the struct but not the hash is the silent-
/// dedup-collapse hazard this issue is about; a stale identifier in the
/// hash but not the struct means the walk drifted from a since-renamed or
/// removed field.
#[test]
fn hash_gpu_material_fields_covers_every_gpu_material_field() {
    let rust_src = include_str!("../material.rs");

    let struct_fields: std::collections::BTreeSet<String> =
        parse_rust_struct_fields(rust_src, "pub struct GpuMaterial")
            .into_iter()
            .collect();
    assert!(
        struct_fields.len() > 60,
        "parsed only {} fields from `struct GpuMaterial` — parser likely broke",
        struct_fields.len()
    );

    let hash_body = extract_fn_body(rust_src, "fn hash_gpu_material_fields")
        .expect("material.rs must declare `hash_gpu_material_fields`");
    let hashed_fields: std::collections::BTreeSet<String> =
        extract_mat_field_accesses(hash_body).into_iter().collect();
    assert!(
        hashed_fields.len() > 60,
        "parsed only {} `mat.<field>` accesses out of hash_gpu_material_fields \
         — parser likely broke",
        hashed_fields.len()
    );

    let missing_from_hash: Vec<&String> = struct_fields.difference(&hashed_fields).collect();
    assert!(
        missing_from_hash.is_empty(),
        "GpuMaterial field(s) {missing_from_hash:?} are declared on the struct but never \
         hashed by hash_gpu_material_fields — MaterialTable::intern_by_hash would silently \
         collapse two materials that differ only in this field onto one table slot in \
         release builds (#3568 / REN-2026-08-30-D7-01). Add `h.write_u32(mat.<field>...)` \
         to hash_gpu_material_fields (and the matching DrawCommand::material_hash walk)."
    );

    let stale_in_hash: Vec<&String> = hashed_fields.difference(&struct_fields).collect();
    assert!(
        stale_in_hash.is_empty(),
        "hash_gpu_material_fields hashes field(s) {stale_in_hash:?}, which {} not declared \
         on GpuMaterial — the hash walk has drifted from a renamed or removed field.",
        if stale_in_hash.len() == 1 {
            "is"
        } else {
            "are"
        }
    );
}

/// #2770 / REN-D1-03 — `MATERIAL_KIND_*` values exist in two independent
/// Rust tables: `scene_buffer::constants` (the authoritative values other
/// Rust code imports and compares against) and `shader_constants_data` (a
/// mirror `build.rs` uses to generate the GLSL `#define`s). Nothing
/// previously pinned the two tables together — the same class of
/// decoupling #2686 found for `GLASS_RAY_BUDGET`. Also asserts the
/// generated GLSL header carries the same values, closing the loop.
#[test]
fn material_kind_constants_stay_in_lockstep_across_rust_and_glsl() {
    use crate::shader_constants as rust_glsl_mirror;
    use crate::vulkan::scene_buffer as rust_authoritative;

    let pairs: &[(&str, u32, u32)] = &[
        (
            "MATERIAL_KIND_MULTI_LAYER_PARALLAX",
            rust_authoritative::MATERIAL_KIND_MULTI_LAYER_PARALLAX,
            rust_glsl_mirror::MATERIAL_KIND_MULTI_LAYER_PARALLAX,
        ),
        (
            "MATERIAL_KIND_GLASS",
            rust_authoritative::MATERIAL_KIND_GLASS,
            rust_glsl_mirror::MATERIAL_KIND_GLASS,
        ),
        (
            "MATERIAL_KIND_EFFECT_SHADER",
            rust_authoritative::MATERIAL_KIND_EFFECT_SHADER,
            rust_glsl_mirror::MATERIAL_KIND_EFFECT_SHADER,
        ),
        (
            "MATERIAL_KIND_NO_LIGHTING",
            rust_authoritative::MATERIAL_KIND_NO_LIGHTING,
            rust_glsl_mirror::MATERIAL_KIND_NO_LIGHTING,
        ),
        (
            "MATERIAL_KIND_FIRE_REFRACTION",
            rust_authoritative::MATERIAL_KIND_FIRE_REFRACTION,
            rust_glsl_mirror::MATERIAL_KIND_FIRE_REFRACTION,
        ),
    ];

    let glsl_header = include_str!("../../../shaders/include/shader_constants.glsl");
    for (name, authoritative, mirror) in pairs {
        assert_eq!(
            authoritative, mirror,
            "{name}: scene_buffer::constants ({authoritative}) and \
             shader_constants_data ({mirror}) disagree — see #2770 / \
             REN-D1-03 and #2686 / SAFE-D7-01 for the same class of \
             decoupling."
        );
        assert!(
            glsl_header.contains(&format!("#define {name} {authoritative}u")),
            "shader_constants.glsl is missing or stale for `{name}` — run \
             `cargo build -p byroredux-renderer` to regenerate it."
        );
    }
}

/// #2770 / REN-D1-03 regression: material kind 11 (MultiLayerParallax)
/// must be referenced through the shared `MATERIAL_KIND_MULTI_LAYER_PARALLAX`
/// constant, not a raw `11u` literal, everywhere it gates RT classification.
/// Pins the production (non-test) sites; the four sites the issue named
/// were `predicates.rs`, `draw.rs`, the acceleration test module, and
/// `triangle.frag`.
#[test]
fn material_kind_multi_layer_parallax_has_no_raw_literal_call_sites() {
    let cases = [
        (
            "triangle.frag",
            include_str!("../../../shaders/triangle.frag"),
            "mat.materialKind == MATERIAL_KIND_MULTI_LAYER_PARALLAX",
        ),
        (
            "acceleration/predicates.rs",
            include_str!("../acceleration/predicates.rs"),
            "material_kind == crate::vulkan::scene_buffer::MATERIAL_KIND_MULTI_LAYER_PARALLAX",
        ),
        (
            "context/draw.rs",
            include_str!("../context/draw.rs"),
            "cmd.material_kind == MATERIAL_KIND_MULTI_LAYER_PARALLAX",
        ),
    ];
    for (name, src, needle) in cases {
        assert!(
            src.contains(needle),
            "{name}: expected the shared `MATERIAL_KIND_MULTI_LAYER_PARALLAX` \
             constant at the material-kind-11 gate (expected to find: \
             `{needle}`) — see #2770 / REN-D1-03."
        );
        assert!(
            !src.contains("const MATERIAL_KIND_MULTI_LAYER_PARALLAX: u32 = 11"),
            "{name}: must not reintroduce a locally hand-declared copy of \
             MATERIAL_KIND_MULTI_LAYER_PARALLAX."
        );
    }
}

/// #2796 / REN-D16-01 regression. Bloom must run AFTER composite (reading
/// composite's assembled scene, not the pre-composite raw HDR that never
/// contained sky/GI/caustics), and composite.frag must no longer fold
/// bloom into `combined` itself — the add now happens downstream, in
/// place, via `bloom.rs::apply_to_scene`. Static source checks (no
/// glslang / Vulkan device needed):
/// - `record_post_passes` calls `record_composite_pass` before
///   `record_bloom_pass`.
/// - `composite.frag` no longer contains the old `combined += bloom *
///   BLOOM_INTENSITY` add.
/// - `bloom_apply.comp` exists and performs the expected
///   read-modify-write against `sceneImage`.
#[test]
fn bloom_dispatches_after_composite_and_applies_itself_downstream() {
    let post_passes = include_str!("../context/post_passes.rs");
    let record_post_passes_start = post_passes
        .find("pub(super) fn record_post_passes")
        .expect("record_post_passes must exist");
    let body = &post_passes[record_post_passes_start..];
    let composite_call = body
        .find("self.record_composite_pass(cmd, frame)")
        .expect("record_post_passes must call record_composite_pass");
    let bloom_call = body
        .find("self.record_bloom_pass(cmd, frame)")
        .expect("record_post_passes must call record_bloom_pass");
    assert!(
        composite_call < bloom_call,
        "record_post_passes must call record_composite_pass BEFORE \
         record_bloom_pass — bloom's pyramid must read composite's own \
         assembled scene (sky + GI + caustics + direct), not the \
         pre-composite raw HDR. See #2796 / REN-D16-01."
    );

    let composite_frag = include_str!("../../../shaders/composite.frag");
    assert!(
        !composite_frag.contains("combined += bloom * BLOOM_INTENSITY"),
        "composite.frag must not fold bloom into `combined` itself any \
         more — that add now happens downstream in \
         `bloom.rs::apply_to_scene`, in place on composite's own output. \
         See #2796 / REN-D16-01."
    );

    let apply_comp = include_str!("../../../shaders/bloom_apply.comp");
    assert!(
        apply_comp.contains("imageLoad(sceneImage, coord)")
            && apply_comp.contains("texture(bloomTex, uv)")
            && apply_comp.contains("imageStore(sceneImage, coord,"),
        "bloom_apply.comp must read the scene, sample bloom, and write \
         the sum back to the same image in place."
    );

    let bloom_rs = include_str!("../bloom.rs");
    assert!(
        bloom_rs.contains("pub unsafe fn apply_to_scene("),
        "BloomPipeline must expose apply_to_scene — the compute pass that \
         adds bloom back onto composite's output."
    );
}

/// #2815 / REN-D19-04 regression. `perturbNormal`'s Path 1 (authored
/// vertex tangent) must guard the POST-Gram-Schmidt-projection tangent
/// length before normalizing it, not just the raw incoming tangent's
/// length — a T ∥ N vertex tangent passes the raw-length check but
/// projects to the zero vector, and `normalize(vec3(0))` is NaN. Static
/// source check: the projected tangent must be bound to a name, checked
/// against a small-magnitude threshold, and only normalized after that
/// check passes — mirroring the guard shape `parallaxDisplaceUV` and
/// `getRayHitTangentFrame` already use for the identical hazard.
#[test]
fn perturb_normal_guards_post_projection_tangent_length() {
    let src = include_str!("../../../shaders/include/material_sampling.glsl");
    // #4016 re-point — see `every_gram_schmidt_tangent_frame_guards_the_
    // projected_length`: the two tangent paths live in `perturbNormalGrad`
    // now, and `perturbNormal` is the implicit-derivative wrapper over it.
    let fn_start = src
        .find("vec3 perturbNormalGrad(")
        .expect("perturbNormalGrad must exist");
    // Path 1 ends where Path 2's comment begins.
    let path2_start = src[fn_start..]
        .find("// Path 2")
        .map(|i| fn_start + i)
        .expect("perturbNormalGrad must have a Path 2");
    let path1_body = &src[fn_start..path2_start];

    assert!(
        path1_body.contains("vec3 Tproj = T - dot(T, N) * N;"),
        "perturbNormalGrad Path 1: the Gram-Schmidt projection must be bound \
         to a name so it can be guarded before normalizing — see \
         #2815 / REN-D19-04."
    );
    assert!(
        path1_body.contains("if (dot(Tproj, Tproj) < 1e-8)") && path1_body.contains("return N;"),
        "perturbNormalGrad Path 1 must bail to the unperturbed geometric \
         normal when the projected tangent is near-zero (T ∥ N), before \
         ever normalizing it — see #2815 / REN-D19-04."
    );
    assert!(
        !path1_body.contains("normalize(T - dot(T, N) * N)"),
        "perturbNormalGrad Path 1 must not reintroduce the unguarded \
         normalize-of-a-possibly-zero-vector expression."
    );
}

// ── GpuLight four-way GLSL lockstep (#1916) ──

/// Every shader source under `crates/renderer/shaders/` that **declares**
/// `decl` (e.g. `"struct GpuInstance"`), discovered by walking the directory
/// rather than read off a hand-maintained list. Paths are returned relative to
/// the shaders directory, sorted, so they can be compared against the
/// `SOURCES` tables below verbatim.
///
/// #3564 / REN-2026-08-30-D3-02 — the GLSL-mirror lockstep guards each
/// hardcode the SET of files they check as `include_str!` literals, so a new
/// shader that declares one of these structs is born completely outside the
/// contract and every existing test stays green. That delegates the guard's
/// own correctness back to the code-review convention the guard exists to stop
/// relying on. Pairing each `SOURCES` table with this walk converts "someone
/// forgot to add the file" from silent into a named test failure.
///
/// A line counts as a declaration only if the text before any `//` comment
/// starts with `decl` followed by whitespace or `{`. That is what separates a
/// real declaration from a source comment *mentioning* the struct — the
/// near-miss already on record is `skin_vertices.comp`, whose comment names
/// `struct GpuInstance` while the file declares none (a bare `str::contains`,
/// and `extract_struct_body`'s `find`, would both count it).
fn shader_sources_declaring(decl: &str) -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read shader dir {}: {e}", dir.display()))
        {
            let path = entry.expect("shader dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_glsl = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("vert" | "frag" | "comp" | "glsl")
            );
            if !is_glsl {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let declares = src.lines().any(|raw| {
                let code = match raw.find("//") {
                    Some(i) => &raw[..i],
                    None => raw,
                };
                let code = code.trim_start();
                code.strip_prefix(decl)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '{']))
            });
            if declares {
                found.push(
                    path.strip_prefix(&root)
                        .expect("under shaders root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    found.sort();
    found
}

/// Assert the hand-written `SOURCES` table for a GLSL-mirror lockstep test
/// covers exactly the files that actually declare `decl` (#3564).
fn assert_mirror_list_is_complete(decl: &str, sources: &[(&str, &str)], issue: &str) {
    let mut listed: Vec<String> = sources.iter().map(|(n, _)| (*n).to_string()).collect();
    listed.sort();
    let discovered = shader_sources_declaring(decl);
    assert_eq!(
        discovered, listed,
        "`{decl}` mirror set drifted: crates/renderer/shaders/ declares it in {discovered:?} \
         but the lockstep test checks {listed:?}. Every declaration must be in the SOURCES \
         table — an unlisted mirror with a dropped or reordered field silently corrupts the \
         data for whichever pass reads it, with a fully green `cargo test` ({issue})."
    );
}

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
///
/// #3763 (SAFE-2026-08-30-D6-02) — this used to be the ONLY leg: mirror-
/// vs-mirror across the four GLSL copies, never against `gpu_types.rs`.
/// `GpuMaterial` and `GpuInstance` both have a second leg
/// (`gpu_material_glsl_field_order_matches_rust_struct`,
/// `gpu_instance_glsl_copies_stay_in_lockstep`'s own second half) that
/// catches a Rust-only field append/reorder; `GpuLight` had none, so that
/// exact class of drift shipped green. Adds it here, reusing the same
/// `parse_rust_struct_fields`/`normalize_ident` machinery
/// `gpu_instance_glsl_copies_stay_in_lockstep` uses.
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

    assert_mirror_list_is_complete("struct GpuLight", SOURCES, "#1916 / #3564");

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

    // Second leg (#3763): the shared GLSL field list must also match the
    // Rust `#[repr(C)]` struct's declaration order. `strip_struct_body`
    // above kept full "type name;" lines for the mirror-vs-mirror
    // comparison; re-parse one representative source into bare field
    // names via `parse_glsl_struct_fields` (leg 1 already proved all four
    // are identical, so any one source stands in for all of them).
    let (_, first_src) = SOURCES[0];
    let glsl_fields = parse_glsl_struct_fields(first_src, "struct GpuLight");
    let rust_src = include_str!("gpu_types.rs");
    let rust_fields = parse_rust_struct_fields(rust_src, "pub struct GpuLight");

    let rust_norm: Vec<String> = rust_fields.iter().map(|f| normalize_ident(f)).collect();
    let glsl_norm: Vec<String> = glsl_fields.iter().map(|f| normalize_ident(f)).collect();

    assert_eq!(
        rust_norm.len(),
        glsl_norm.len(),
        "GpuLight field COUNT differs: Rust has {} {:?}, GLSL mirrors have {} {:?}. The Rust \
         `struct GpuLight` (gpu_types.rs) and its four GLSL mirrors must stay in lockstep — \
         see #3763 / SAFE-2026-08-30-D6-02.",
        rust_norm.len(),
        rust_fields,
        glsl_norm.len(),
        glsl_fields,
    );

    for (i, (r, g)) in rust_norm.iter().zip(glsl_norm.iter()).enumerate() {
        assert_eq!(
            r, g,
            "GpuLight field #{i} ORDER mismatch: Rust `{}` vs GLSL `{}`. Every GLSL \
             `struct GpuLight` mirror must declare fields in the SAME order as the Rust \
             `#[repr(C)]` struct — see #3763 / SAFE-2026-08-30-D6-02.",
            rust_fields[i], glsl_fields[i],
        );
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

    assert_mirror_list_is_complete("struct GpuInstance", SOURCES, "#2748 / #3564");

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

/// #3231 — closes a real gap the two tests above cannot: neither checks
/// GLSL's std430 ALIGNMENT rules, only field NAME/ORDER/COUNT. A 3-
/// component vector type (`vec3`/`ivec3`/`uvec3`) is 16-byte-aligned
/// under std430 — the exact footgun `GpuInstance`'s own top-of-struct
/// doc comment warns about for `vec3` — so slipping one into a `struct
/// GpuInstance` declaration desyncs the array STRIDE the shader
/// compiler computes from the one the CPU-uploaded `#[repr(C)]` struct
/// actually uses, corrupting every instance past the first. Confirmed
/// the hard way: an earlier revision of this exact padding tail used
/// `uvec3 _reserved2`, passed both tests above (name/order/count all
/// matched), and produced a GPU device-lost hang with zero
/// validation-layer diagnostic on the very next `cargo run`. Scoped to
/// `GpuInstance` specifically (not every struct in these files) because
/// `GpuMaterial`/`GpuLight`/`GpuCamera` already forbid `vec3` entirely
/// via their own established "all scalars" convention this test can't
/// see past struct boundaries to verify generically.
#[test]
fn gpu_instance_glsl_declarations_never_use_a_3_component_vector_type() {
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
    for (name, src) in SOURCES {
        let body = extract_struct_body(src, "struct GpuInstance")
            .unwrap_or_else(|| panic!("{name}: source must declare `struct GpuInstance`"));
        // Strip `//` line comments first (same as parse_glsl_struct_fields_typed)
        // so an explanatory comment merely NAMING the forbidden type —
        // e.g. "NOT uvec3" — doesn't trip this as a false positive.
        let code_only: String = body
            .lines()
            .map(|line| match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        for needle in ["vec3", "ivec3", "uvec3"] {
            assert!(
                !code_only.contains(needle),
                "{name}: `struct GpuInstance` declares a `{needle}` field — 3-component \
                 vectors are 16-byte-aligned under std430 and will desync the array stride \
                 from the tightly-packed Rust struct (#3231). Use separate scalar fields \
                 instead, matching every other padding lane in this struct."
            );
        }
    }
}

/// std430 size of a GLSL field list, honouring alignment. Shared by the
/// boundary-instance stride guard below; kept deliberately small and
/// total — an unrecognised type panics rather than silently contributing
/// zero, which would make an over-wide struct look correctly sized.
fn std430_struct_size(fields: &[(String, String)]) -> usize {
    let mut offset = 0usize;
    let mut max_align = 1usize;
    for (ty, name) in fields {
        let (size, align) = match ty.as_str() {
            "float" | "uint" | "int" | "bool" => (4, 4),
            "vec2" | "uvec2" | "uint64_t" => (8, 8),
            "vec4" | "uvec4" => (16, 16),
            "mat4" => (64, 16),
            other => panic!(
                "std430_struct_size: unhandled GLSL type `{other}` on field `{name}` — add it \
                 here rather than letting it contribute zero bytes to a stride assertion."
            ),
        };
        offset = offset.div_ceil(align) * align;
        offset += size;
        max_align = max_align.max(align);
    }
    offset.div_ceil(max_align) * max_align
}

/// #3829 — `volumetrics_inject.comp` declares `struct GpuBoundaryInstance`, a
/// deliberate prefix-mirror of `GpuInstance` bound to the very same per-frame
/// instance SSBO (binding 19, written by
/// `VolumetricsPipeline::write_boundary_geometry`). Because it carries its own
/// struct NAME, it sits outside `gpu_instance_glsl_copies_stay_in_lockstep` —
/// whose SOURCES list is hardcoded, and whose `assert_mirror_list_is_complete`
/// discovery half greps for the literal `struct GpuInstance` and so cannot see
/// it either. It was a sixth, untracked mirror.
///
/// It went stale exactly that way: #3231 grew `GpuInstance` 128 → 160 B and
/// this mirror kept its single 16-byte `_boundaryTail`, leaving a 128-byte
/// stride reading the same buffer. Every boundary-geometry read past instance
/// index 0 was misaligned by 32 bytes per index for ~13 days, with a fully
/// green `cargo test` — silently wrong fire/smoke-vs-geometry collision
/// normals rather than a crash, since the fields feed bounds checks and a
/// normal lookup rather than a raw dereference.
///
/// The tail exists only to make the stride right, so this pins the two things
/// that actually matter: the named prefix agrees with the Rust struct
/// field-for-field, and the total std430 stride equals `size_of::<GpuInstance>()`.
#[test]
fn gpu_boundary_instance_stride_matches_gpu_instance() {
    let glsl_src = include_str!("../../../shaders/volumetrics_inject.comp");
    let glsl = parse_glsl_struct_fields_typed(glsl_src, "struct GpuBoundaryInstance");
    let rust = parse_rust_struct_fields(include_str!("gpu_types.rs"), "pub struct GpuInstance");

    // Leg 1: the NAMED prefix must match the Rust struct field-for-field. The
    // shader recovers `model` / `boneOffset` / `vertexOffset` / `indexOffset` /
    // `vertexCount` by name, so a reorder here is as corrupting as a stride
    // mismatch and a size-only assertion cannot see it.
    let prefix: Vec<String> = glsl
        .iter()
        .map(|(_, name)| name.clone())
        .take_while(|name| !name.starts_with("_boundaryTail"))
        .collect();
    assert!(
        prefix.len() >= 13,
        "parsed only {} named GpuBoundaryInstance prefix field(s) — parser likely broke",
        prefix.len()
    );
    for (i, glsl_name) in prefix.iter().enumerate() {
        assert_eq!(
            normalize_ident(&rust[i]),
            normalize_ident(glsl_name),
            "GpuBoundaryInstance prefix field #{i} diverges from `GpuInstance`: Rust `{}` vs \
             GLSL `{glsl_name}`. Both read the same SSBO (binding 19), so the named prefix must \
             stay field-for-field identical (#3829).",
            rust[i],
        );
    }

    // Leg 2: the total stride must match, or every index past 0 is misaligned.
    let glsl_stride = std430_struct_size(&glsl);
    assert_eq!(
        glsl_stride,
        std::mem::size_of::<GpuInstance>(),
        "`GpuBoundaryInstance` (volumetrics_inject.comp) has a {glsl_stride}-byte std430 stride \
         but `GpuInstance` is {} B. They are bound to the SAME buffer, so a mismatch misaligns \
         every boundary-geometry read past instance index 0 by the difference, per index — \
         silently wrong data, no validation error (#3829). Widen the `_boundaryTail*` words to \
         cover the new fields.",
        std::mem::size_of::<GpuInstance>(),
    );
}

// ── CameraUBO five-way GLSL lockstep (#3684 / PERF-D4-2026-08-30-04) ──

/// #3684 — `struct GpuCamera` (declared in GLSL as `uniform CameraUBO { ... }`)
/// is hand-duplicated across the same five sources `GpuInstance` is
/// (`include/bindings.glsl`, `triangle.vert`, `water.vert`,
/// `cluster_cull.comp`, `caustic_splat.comp`), but — unlike `GpuInstance`,
/// `GpuLight`, and `GpuMaterial` — had no lockstep test at all: only a
/// `size_of::<GpuCamera>() == 368` pin and a SPIR-V block-SIZE reflection
/// check, both blind to a within-size field reorder (`skyTint` ↔
/// `sunDirection` — two adjacent `vec4`s in a struct that is entirely
/// `vec4`s) or a type flip (`uvec4 renderDebug` → `vec4`, whose bits are
/// read back via `floatBitsToUint`/`bitcast`). #2688 established that
/// exact type-flip class as byte-lethal for `GpuMaterial`; the camera had
/// no equivalent guard.
///
/// Reuses the same two-leg pattern `GpuLight`/`GpuInstance` established:
/// mirror-vs-mirror across the five GLSL copies via `strip_struct_body`
/// (CameraUBO has no multi-name declarations, so the simpler raw-line
/// comparison `GpuLight` uses is sufficient — `parse_glsl_struct_fields`'s
/// multi-name handling `GpuInstance` needs isn't required here), then a
/// second, TYPED leg against the Rust `#[repr(C)] struct GpuCamera` that
/// checks name, order, AND scalar/array type via
/// [`rust_glsl_scalar_type_matches`] — extended by this same fix to
/// recognize `GpuCamera`'s `[f32; 4]`/`[u32; 4]`/`[[f32; 4]; 4]` shapes,
/// which no prior struct in this file needed (`GpuMaterial` is bare
/// scalars only).
///
/// Deliberately does NOT call [`assert_mirror_list_is_complete`] /
/// [`shader_sources_declaring`]: those require `decl` to be the START of
/// the trimmed source line (after stripping any `//` comment), which
/// matches a plain `struct X {` declaration but not
/// `layout(set = N, binding = M) uniform CameraUBO {` — the `layout(...)`
/// qualifier always precedes it. Loosening that shared, already-tested
/// helper (used by three other structs' lockstep tests) to a bare
/// substring match would reopen the exact false-positive the doc comment
/// on `shader_sources_declaring` names as the reason it isn't one already
/// (`skin_vertices.comp`'s comment *mentioning* `struct GpuInstance` while
/// declaring none). A sixth shader adding `CameraUBO` without joining this
/// SOURCES list is a real but narrower gap than the field-lockstep defect
/// this test exists to close, and isn't in the issue's own suggested fix.
#[test]
fn camera_ubo_glsl_copies_stay_in_lockstep() {
    const SOURCES: &[(&str, &str)] = &[
        (
            "include/bindings.glsl",
            include_str!("../../../shaders/include/bindings.glsl"),
        ),
        (
            "triangle.vert",
            include_str!("../../../shaders/triangle.vert"),
        ),
        ("water.vert", include_str!("../../../shaders/water.vert")),
        (
            "cluster_cull.comp",
            include_str!("../../../shaders/cluster_cull.comp"),
        ),
        (
            "caustic_splat.comp",
            include_str!("../../../shaders/caustic_splat.comp"),
        ),
    ];

    let mut reference: Option<(&str, Vec<String>)> = None;
    for (name, src) in SOURCES {
        let body = extract_struct_body(src, "uniform CameraUBO {")
            .unwrap_or_else(|| panic!("{name}: no longer declares `uniform CameraUBO {{`"));
        let fields = strip_struct_body(body);
        assert!(
            fields.len() >= 13,
            "{name}: parsed only {} CameraUBO field lines — parser likely broke",
            fields.len()
        );
        match &reference {
            None => reference = Some((name, fields)),
            Some((ref_name, ref_fields)) => {
                assert_eq!(
                    ref_fields, &fields,
                    "CameraUBO layout mismatch: `{ref_name}` vs `{name}`. All five GLSL copies \
                     of `uniform CameraUBO` must declare identical fields in the same order \
                     (Shader Struct Sync invariant, #3684) — a drift here silently corrupts \
                     camera data (view/projection matrices, lighting, motion vectors) for \
                     whichever copy lags behind."
                );
            }
        }
    }

    // Second leg: name, order, AND type against the Rust `#[repr(C)]`
    // struct — the offset source of truth.
    let (_, first_src) = SOURCES[0];
    let glsl_typed = parse_glsl_struct_fields_typed(first_src, "uniform CameraUBO {");
    let rust_src = include_str!("gpu_types.rs");
    let rust_typed = parse_rust_struct_fields_typed(rust_src, "pub struct GpuCamera");

    let rust_fields: Vec<String> = rust_typed.iter().map(|(_, n)| n.clone()).collect();
    let glsl_fields: Vec<String> = glsl_typed.iter().map(|(_, n)| n.clone()).collect();
    let rust_norm: Vec<String> = rust_fields.iter().map(|f| normalize_ident(f)).collect();
    let glsl_norm: Vec<String> = glsl_fields.iter().map(|f| normalize_ident(f)).collect();

    assert_eq!(
        rust_norm.len(),
        glsl_norm.len(),
        "GpuCamera field COUNT differs: Rust has {} {:?}, GLSL mirrors have {} {:?}. The Rust \
         `struct GpuCamera` (gpu_types.rs) and its five GLSL `CameraUBO` mirrors must stay in \
         lockstep — see #3684.",
        rust_norm.len(),
        rust_fields,
        glsl_norm.len(),
        glsl_fields,
    );

    // #3684 — two fields are named differently on purpose between the Rust
    // struct and every GLSL mirror: Rust `position` is GLSL `cameraPos`,
    // and Rust `flags` is GLSL `sceneFlags`. Both are consistent across
    // all five GLSL copies (this test's own first leg already proved
    // that), so they're a deliberate naming choice — the GLSL side names
    // things by what a shader author reads at the call site, the Rust
    // side by what the CPU struct field is. Recorded explicitly here
    // rather than silently loosening the check, so any OTHER field name
    // mismatch (a real drift) still fails loud.
    const KNOWN_NAME_ALIASES: &[(&str, &str)] =
        &[("position", "cameraPos"), ("flags", "sceneFlags")];
    for (i, (r_raw, g_raw)) in rust_fields.iter().zip(glsl_fields.iter()).enumerate() {
        let names_match = normalize_ident(r_raw) == normalize_ident(g_raw);
        let aliased = KNOWN_NAME_ALIASES
            .iter()
            .any(|(rust_name, glsl_name)| rust_name == r_raw && glsl_name == g_raw);
        assert!(
            names_match || aliased,
            "GpuCamera field #{i} ORDER mismatch: Rust `{r_raw}` vs GLSL `{g_raw}`. Every GLSL \
             `uniform CameraUBO` mirror must declare fields in the SAME order as the Rust \
             `#[repr(C)]` struct — see #3684.",
        );
    }

    for (i, ((rust_ty, rust_name), (glsl_ty, glsl_name))) in
        rust_typed.iter().zip(glsl_typed.iter()).enumerate()
    {
        assert!(
            rust_glsl_scalar_type_matches(rust_ty, glsl_ty),
            "GpuCamera field #{i} TYPE mismatch: Rust `{rust_name}: {rust_ty}` vs GLSL \
             `{glsl_ty} {glsl_name}`. A within-size type reinterpretation (e.g. `uvec4` <-> \
             `vec4`) preserves field order and struct size but corrupts the value read through \
             the mismatched type — see #3684 (the #2688 GpuMaterial precedent for this exact \
             class of defect).",
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
        "vec3 primaryDiffuseWeight = (1.0 - fresnelSchlickPower(",
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
        "fresnelSchlickScalarPower(",
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

#[test]
fn bethesda_lighting_response_is_masked_shadowable_and_palette_scaled() {
    let frag = include_str!("../../../shaders/triangle.frag");
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let pbr = include_str!("../../../shaders/include/pbr.glsl");

    for needle in [
        "mat.lightingMaskMapIndex",
        "mat.backLightingMapIndex",
        "clamp(mat.grayscaleToPaletteScale, 0.0, 1.0)",
        "lightingMask, backLightingMap",
    ] {
        assert!(frag.contains(needle), "triangle.frag lost `{needle}`");
    }
    for needle in [
        "MAT_FLAG_SOFT_LIGHTING",
        "MAT_FLAG_RIM_LIGHTING",
        "MAT_FLAG_BACK_LIGHTING",
        "bethesdaDiffuseLightFactor",
        "bethesdaRimFactor",
        "bethesdaBackFactor",
        "fresnelSchlickPower(HdotV, F0, mat.fresnelPower)",
    ] {
        assert!(
            lighting.contains(needle),
            "canonical direct-light path lost `{needle}`"
        );
    }
    assert!(
        pbr.contains("vec3 fresnelSchlickPower(") && pbr.contains("abs(exponent - 5.0) < 1e-4"),
        "authored Fresnel power must retain the optimized neutral x^5 path"
    );
}

/// #3530 — the alpha-channel height path. `PARALLAX_ALPHA_HEIGHT_BIT` rides
/// in bit 31 of `GpuMaterial.parallaxMapIndex`, so **every** reader must mask
/// it before using the value as a bindless index: `textures[0x8000000N]` is a
/// wildly out-of-bounds descriptor read, not a cosmetic mistake. Both POM
/// implementations (the raster one in `material_sampling.glsl` and the
/// secondary-ray one in `ray_hit.glsl`) must also honour the channel, or an
/// Oblivion cave wall parallaxes correctly in the raster pass and samples a
/// normal's red channel as height in reflections.
///
/// The masking half of this contract (every bindless subscript actually
/// masks the bit off) is now [`bindless_index_bits_are_masked_at_every_textures_subscript`]
/// — a per-occurrence, whole-shader-tree scan rather than the "does this one
/// file contain the mask string anywhere" check this test used to run (#3624
/// / REN-2026-08-30-D19-04: that shape couldn't catch a new unmasked
/// subscript added to a file that already had one masked elsewhere, or a
/// new shader file entirely). What's left here is specific to the parallax
/// height *channel* semantics, not the masking mechanism.
#[test]
fn parallax_alpha_height_bit_is_honoured_by_both_marchers() {
    let sampling = include_str!("../../../shaders/include/material_sampling.glsl");
    let hit = include_str!("../../../shaders/include/ray_hit.glsl");

    // Both marchers select the channel rather than hardcoding `.r`.
    assert!(
        sampling.contains("heightInAlpha ? texel.a : texel.r"),
        "the raster POM must sample alpha when the bit is set"
    );
    assert!(
        hit.contains("heightInAlpha ? heightTexel.a : heightTexel.r"),
        "the secondary-ray POM must honour the same channel selection, or \
         reflections disagree with the raster pass"
    );
    // And no un-masked `.r` height fetch survives in either marcher.
    assert!(
        !sampling.contains("textures[nonuniformEXT(parallaxMapIdx)], currentUV).r"),
        "a raw `.r` height fetch survived in the raster POM"
    );
    assert!(
        !hit.contains("textures[nonuniformEXT(mat.parallaxMapIndex)]"),
        "the secondary-ray POM must sample the masked index, never the raw one"
    );
}

/// #3622 (REN-2026-08-30-D19-03) — the raster POM march used to sample its
/// height field with plain `texture()` (implicit derivatives) inside a loop
/// with a data-dependent `break`, which is undefined per the GLSL/Vulkan
/// spec: invocations in the same subgroup can be on different iterations
/// by the time any of them samples, so the hardware-computed derivative is
/// meaningless. Fix: capture one real, well-defined LOD via
/// `textureQueryLod` at the loop's entry UV (before any divergent control
/// flow), then sample every layer at that fixed LOD with `textureLod` —
/// matching `ray_hit.glsl`'s explicit-LOD discipline (which uses a literal
/// `0.0` out of necessity: secondary rays have no screen-space derivatives
/// to query in the first place).
#[test]
fn raster_pom_marcher_samples_height_at_an_explicit_lod() {
    let sampling = include_str!("../../../shaders/include/material_sampling.glsl");

    assert!(
        sampling.contains("textureQueryLod(textures[nonuniformEXT(parallaxMapIdx)], uv).x"),
        "parallaxDisplaceUV must capture the LOD once, at the entry UV, \
         before the divergent march loop"
    );
    assert!(
        sampling.contains(
            "float sampleParallaxHeight(uint idx, vec2 uv, bool heightInAlpha, float lod)"
        ),
        "sampleParallaxHeight must take an explicit lod parameter"
    );
    assert!(
        sampling.contains("textureLod(textures[nonuniformEXT(idx)], uv, lod)"),
        "sampleParallaxHeight must sample with the caller-supplied explicit \
         LOD, not implicit derivatives"
    );
    assert!(
        !sampling.contains("texture(textures[nonuniformEXT(idx)], uv)"),
        "the old implicit-derivative height fetch must not survive"
    );
    // All three march call sites (entry sample, per-layer sample, secant
    // interpolation sample) must thread the same captured `parallaxLod`
    // through — a call site left on an old signature would fail to
    // compile, but pin the exact count so a future refactor that silently
    // drops the LOD argument from one call (while still compiling, e.g. by
    // reintroducing a 3-arg overload) doesn't go unnoticed.
    assert_eq!(
        sampling.matches("heightInAlpha, parallaxLod").count(),
        3,
        "expected exactly 3 call sites (entry / per-layer / secant) to pass \
         parallaxLod through to sampleParallaxHeight"
    );
}

/// #3624 (REN-2026-08-30-D19-04) — replaces the old 3-file whitelist +
/// at-least-once substring check, which could neither catch a 4th shader
/// file reading `parallaxMapIndex`/`glossMapIndex` unmasked, nor a 4th
/// *unmasked* read added to a file that already had one masked occurrence
/// elsewhere (a `src.contains(...)` check goes green the moment ANY line
/// matches, not EVERY hazardous line).
///
/// Walks every `.frag`/`.vert`/`.comp`/`.glsl` file under
/// `crates/renderer/shaders/` and, for each line that builds a bindless
/// `textures[...]` subscript (the actual out-of-bounds-descriptor hazard —
/// `textures[0x8000000N]` — that both bits exist to prevent), requires the
/// field's mask constant to appear on that same line whenever the raw
/// `mat.<field>` struct access also appears there.
///
/// A bare mention of the raw field elsewhere — a comment, an existence
/// test (`!= 0u`), or handing it to a helper that masks internally
/// (`parallaxDisplaceUV(..., mat.parallaxMapIndex, ...)` in
/// `triangle.frag`) — is not itself a hazard and is deliberately not
/// flagged: the callee's OWN `textures[...]` line is where that read gets
/// checked, and it is, because the scan covers every file, not a fixed
/// list. This is what makes the check whole-tree instead of a whitelist
/// that goes stale the moment a new reader is added (the exact failure
/// mode this test replaces).
#[test]
fn bindless_index_bits_are_masked_at_every_textures_subscript() {
    // Generated from `shader_constants_data.rs`, not hand-copied — a value
    // flip changes both sides in lockstep.
    let constants = include_str!("../../../shaders/include/shader_constants.glsl");
    for bit in ["PARALLAX_ALPHA_HEIGHT_BIT", "NORMAL_ALPHA_SPEC_BIT"] {
        assert!(
            constants.contains(&format!("#define {bit}")),
            "{bit} must be `#define`d in shader_constants.glsl"
        );
    }

    // (bindless-index field, its mask constant). Add a pair here — never
    // widen the exemptions below — when a new field gets a channel-selector
    // bit of its own.
    const GUARDED: &[(&str, &str)] = &[
        ("parallaxMapIndex", "PARALLAX_ALPHA_HEIGHT_BIT"),
        ("glossMapIndex", "NORMAL_ALPHA_SPEC_BIT"),
    ];

    let shader_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut files = Vec::new();
    collect_shader_files(&shader_dir, &mut files);
    assert!(
        files.len() >= 10,
        "the shader directory walk under {} found suspiciously few files \
         ({}) — check the extension filter didn't silently break",
        shader_dir.display(),
        files.len()
    );

    let mut violations = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for (line_no, line) in text.lines().enumerate() {
            if !line.contains("textures[") {
                continue; // not a bindless subscript site
            }
            for (field, bit) in GUARDED {
                let field_token = format!("mat.{field}");
                if line.contains(&field_token) && !line.contains(bit) {
                    violations.push(format!(
                        "{}:{}: `{field_token}` used inside a `textures[...]` \
                         subscript without `{bit}` masking — {}",
                        path.display(),
                        line_no + 1,
                        line.trim(),
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "unmasked bindless-index read(s):\n{}",
        violations.join("\n")
    );
}

fn collect_shader_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("dir entry under {}: {e}", dir.display()))
            .path();
        if path.is_dir() {
            collect_shader_files(&path, out);
            continue;
        }
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("frag" | "vert" | "comp" | "glsl")
        ) {
            out.push(path);
        }
    }
}

#[test]
fn material_role_debug_view_is_semantic_and_format_agnostic() {
    let frag = include_str!("../../../shaders/triangle.frag");
    for needle in [
        "debugMode == RENDER_DEBUG_MATERIAL_ROLE",
        "MAT_FLAG_MODEL_SPACE_NORMALS",
        "mat.normalMapIndex != 0u",
        // #3530 — bit 31 of `parallaxMapIndex` is the alpha-height channel
        // selector, so every reader masks it off before testing the index.
        "(mat.parallaxMapIndex & ~PARALLAX_ALPHA_HEIGHT_BIT) != 0u",
        "mat.envMapIndex != 0u || mat.envMaskIndex != 0u",
        "mat.tintMapIndex != 0u",
    ] {
        assert!(frag.contains(needle), "material-role view lost `{needle}`");
    }
    for forbidden in ["skyrim", "fallout", "starfield", "oblivion"] {
        let role_branch = frag
            .split_once("} else if (viewMaterialRole) {")
            .and_then(|(_, tail)| {
                tail.split_once("} else if (viewRtLod) {")
                    .map(|(body, _)| body)
            })
            .expect("material-role debug branch must precede rtLOD");
        assert!(
            !role_branch.to_ascii_lowercase().contains(forbidden),
            "material-role view must classify translated semantics, not `{forbidden}` source formats"
        );
    }
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

#[test]
fn bgem_glass_optics_drive_distinct_reflection_refraction_and_overlay_lanes() {
    let frag = include_str!("../../../shaders/triangle.frag");

    for needle in [
        "mat.glassBlurScale * mat.glassBlurScaleFactor",
        "mat.glassRoughnessScratchMapIndex",
        "mat.glassRefractionScale / DEFAULT_GLASS_REFRACTION_SCALE",
        "reflColor * glassFresnelTint",
        "mat.glassDirtOverlayMapIndex",
        "resolvedAlpha * (1.0 - glassDirtAlpha)",
    ] {
        assert!(
            frag.contains(needle),
            "BGEM glass optical consumer lost `{needle}`"
        );
    }
    assert!(
        frag.contains("glassOpticalRoughness * 8.0")
            && frag.contains("0.4 + glassOpticalRoughness * 5.0"),
        "authored blur/scratch roughness must shape both reflection and refraction"
    );

    // #3459 — both neutral pivots must come from the emitted macros, never
    // from a restated copy of `Material::default()`. Same shape as the water
    // guard in `shader_constants.rs`.
    assert!(
        frag.contains("/ DEFAULT_GLASS_BLUR_SCALE"),
        "blur pivot must divide by the emitted macro"
    );
    assert!(
        !frag.contains("mat.glassBlurScaleFactor, 0.0) / 0.4"),
        "the blur pivot literal must not come back"
    );
    assert!(
        !frag.contains("mat.glassRefractionScale / 0.05"),
        "the refraction pivot literal must not come back"
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

/// #3448 / REN-2026-08-27-D17-01 — `bethesdaRimFactor` resolves its exponent
/// from `rimlightPower` then `lightingEffect2`. Both lanes are `0.0` at every
/// no-value default site (`material_reference_stub`, `parse_fo4`,
/// `parse_fo76_plus`, `MaterialInfo::default`, `ImportedMaterial::default`,
/// `Material::default`), and `parse_skyrim` hard-sets `rimlightPower` to zero
/// by design — so "nothing authored" must not be allowed to fall through the
/// `clamp(exponent, 0.25, 16.0)` onto the FLOOR, which is the broadest
/// possible rim (weight 0.56 head-on at `NdotV = 0.9`) added across the whole
/// surface rather than at its silhouette. This is the same hazard #2589 fixed
/// on the sibling `grayscale_to_palette_scale` / `fresnel_power` fields.
#[test]
fn bethesda_rim_exponent_substitutes_the_format_default_not_the_clamp_floor() {
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let body = extract_struct_body(lighting, "float bethesdaRimFactor")
        .expect("lighting.glsl must declare bethesdaRimFactor");

    assert!(
        body.contains("float exponent = authored > 0.0 ? authored : 2.0;"),
        "bethesdaRimFactor must substitute nif.xml's declared `Lighting Effect 2` \
         default (2.0) when neither exponent lane is authored — see #3448"
    );
    assert!(
        !body.contains("float exponent = mat.rimlightPower > 0.0"),
        "bethesdaRimFactor went back to clamping the raw two-lane pick, so an \
         unauthored (0.0, 0.0) material lands on the clamp FLOOR 0.25 again — the \
         broadest rim, not a neutral one. See #3448."
    );
    // The clamp itself stays — it still bounds authored values — but it must
    // no longer be the thing that decides the no-value case.
    assert!(
        body.contains("clamp(exponent, 0.25, 16.0)"),
        "the authored-value clamp must be kept"
    );
}

/// #3574 / REN-2026-08-30-D17-01 — the per-light early-out in `triangle.frag`
/// must agree with the set of lobes evaluated below it. The #1147 Phase 2b
/// subsurface block is driven by `max(-dot(N, L), 0.0)`, non-zero only where
/// `rawNdotL < 0` — exactly the half-space on which the three Bethesda gate
/// terms are identically zero for a `MAT_FLAG_TRANSLUCENCY`-only material
/// (the flags are disjoint on real content). Without a translucency term in
/// the gate the loop `continue`d on every fragment that block could shade, so
/// the whole feature emitted zero on 100% of loaded content while `mat.dump`
/// and `viewMaterialLobe` both reported it live. Nothing else in the suite can
/// see this: both blocks are independently correct, the defect is their
/// control-flow ordering.
#[test]
fn translucency_drives_the_per_light_contribution_gate() {
    let frag = include_str!("../../../shaders/triangle.frag");

    let start = frag
        .find("float rawNdotL = dot(N, L);")
        .expect("triangle.frag must compute rawNdotL in the cluster light loop");
    let end = frag[start..]
        .find("if (contribution < 0.001) {")
        .expect("triangle.frag must early-out on the contribution gate")
        + start;
    let gate = &frag[start..end];

    assert!(
        gate.contains("MAT_FLAG_TRANSLUCENCY"),
        "the per-light contribution gate must include a translucency term, or the \
         Phase 2b subsurface block below it is unreachable — the gate `continue`s on \
         exactly the back-facing half-space that block exists to shade. See #3574."
    );
    assert!(
        gate.contains("max(-rawNdotL, 0.0)"),
        "the gate's translucency term must be driven by the same back-side `-N·L` the \
         Phase 2b block uses; a term that is zero wherever the block is non-zero \
         re-orphans it. See #3574."
    );
    assert!(
        gate.contains("sssGate) * atten;"),
        "the translucency term must actually reach `contribution` — computing it and \
         dropping it out of the `max(...)` fold is the same bug. See #3574."
    );

    // And the block it feeds must still be there, driven by that same term.
    assert!(
        frag.contains("float backDotL = max(-dot(N, L), 0.0);"),
        "the Phase 2b subsurface block's driver moved — re-derive the gate term to match"
    );
}

/// #3575 / REN-2026-08-30-D17-02 — both shadow-sampling arms must size the
/// soft-shadow emitter disk from `lights[i].params.y`, the canonical source
/// radius `Emitter::from_legacy_world_units` derives once at the translation
/// boundary and clamps to `[1, 32]` units. They used to re-derive it as
/// `position_radius.w * 0.025`, where `.w` is the CULL radius — uploaded as
/// `range * LEGACY_LIGHT_CULL_RANGE_MULTIPLIER` (2.0) and undone again inside
/// `pointSpotAtten`. The two formulas agree only in the unclamped middle
/// (`range * 2.0 * 0.025 == range * 0.05`): the shader had no ceiling, so a
/// 4096-unit worldspace light got a 204.8-unit disk against a canonical 32,
/// and volumetric combustion lights — the one emitter class for which someone
/// computed a real physical `(3V/4pi)^(1/3)` radius — came out up to 7.5x too
/// soft, with the old 1.5-unit floor alone already exceeding their canonical
/// value. It also let a pure cull-window tunable silently own penumbra width.
///
/// `params.y` is the same field `pointSpotAtten` reads as `sourceRadius` and
/// `traceShadowTransmittanceDetailed` receives as `emitterRadius`, so this
/// makes all three agree. The two arms are byte-identical copies, so the pin
/// checks both.
#[test]
fn soft_shadow_disk_reads_the_canonical_source_radius_not_the_cull_radius() {
    let frag = include_str!("../../../shaders/triangle.frag");

    let sites: Vec<&str> = frag
        .match_indices("float lightDiskRadius =")
        .map(|(i, _)| {
            let end = frag[i..].find(';').expect("declaration must terminate") + i;
            &frag[i..end]
        })
        .collect();
    assert_eq!(
        sites.len(),
        2,
        "expected the ReSTIR and legacy-WRS shadow arms to be the only \
         `lightDiskRadius` sites, found {}: {sites:?}",
        sites.len()
    );
    for site in &sites {
        assert!(
            site.contains("lights[i].params.y"),
            "soft-shadow disk site `{site}` must read the canonical source radius \
             `lights[i].params.y` — see #3575"
        );
        assert!(
            !site.contains("radius * 0.025"),
            "soft-shadow disk site `{site}` went back to re-deriving the emitter size \
             from the CULL radius `position_radius.w`. That is 2x the authored range \
             (LEGACY_LIGHT_CULL_RANGE_MULTIPLIER), has no 32-unit ceiling, and makes a \
             culling tunable own shadow softness. See #3575."
        );
    }
}

/// #3620 / REN-2026-08-30-D18-03 — `pathEnvironmentRadiance`'s #3162
/// irradiance-units comment justifies leaving `skyTint` out of the `1/PI`
/// conversion. Its evidence used to be a `skyColor = skyTint.rgb` "background
/// write" in `triangle.frag`, which does not exist: #3323 rewrote that line to
/// `skyColor = exteriorSkyTint.rgb`, and it was never a background write but
/// the glass window-portal escape — a branch whose own comment warns it must
/// not be generalised, so a reader chasing the citation to confirm the units
/// invariant landed in the one place the codebase calls a special case. The
/// units argument itself is sound; only the citation had rotted. Pin the
/// evidence it now points at, so the same drift is caught rather than reviewed.
#[test]
fn path_environment_radiance_cites_live_sky_tint_evidence() {
    let lighting = include_str!("../../../shaders/include/lighting.glsl");
    let frag = include_str!("../../../shaders/triangle.frag");
    let raytrace = include_str!("../../../shaders/include/raytrace.glsl");

    assert!(
        !lighting.contains("skyTint.rgb` background write"),
        "the retired `skyColor = skyTint.rgb` background-write citation came back — \
         no such line exists at HEAD (#3620)"
    );
    assert!(
        !frag.contains("skyColor = skyTint.rgb"),
        "if triangle.frag really regained a `skyColor = skyTint.rgb` write, the #3620 \
         comment fix needs revisiting rather than this assertion relaxing"
    );

    // The radiance-space evidence the comment now cites must actually be there,
    // in both consumers.
    const MISS_BLEND: &str = "skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5";
    assert!(
        frag.contains(MISS_BLEND),
        "triangle.frag must still consume skyTint directly as radiance in the RT-miss \
         blend — that is what the units comment cites (#3620)"
    );
    assert!(
        raytrace.contains(MISS_BLEND),
        "raytrace.glsl's `missCol` twin must still consume skyTint the same way (#3620)"
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
    use crate::shader_constants::{MAX_PATH_SEGMENTS, MAX_SHADED_HITS};
    use crate::vulkan::scene_buffer::ray_budget::AdaptiveRayBudget;

    // #3879 — assert the CPU/GPU *relationship*, not the shader's source
    // text. The old form (`frag.contains("const int MAX_PATH_SEGMENTS = 6;")`)
    // never named `settings_for_tier`, so raising a tier-3 value was a
    // silent no-op: the shader clamped the upload back down to its own
    // literal and this test stayed green because the shader had not changed.
    let top = AdaptiveRayBudget::settings_for_tier(3);
    assert_eq!(
        top.max_path_segments, MAX_PATH_SEGMENTS,
        "the tier-3 segment budget must equal the shader-side ceiling it is \
         clamped against, or raising one silently does nothing"
    );
    assert_eq!(
        top.max_shaded_hits, MAX_SHADED_HITS,
        "the tier-3 shaded-hit budget must equal the shader-side ceiling"
    );

    // The structural shape of the bounded loop still matters: both the hard
    // compile-time ceiling and the adaptive runtime limit must be enforced,
    // and no uploaded value may exceed the ceiling.
    let frag = include_str!("../../../shaders/triangle.frag");
    assert!(
        frag.contains("const int MAX_DIFFUSE_BOUNCES = 2;"),
        "the accepted two-diffuse-event color-bleed path must remain enabled"
    );
    assert!(
        frag.contains("for (int segment = 0; segment < int(MAX_PATH_SEGMENTS); ++segment)")
            && frag.contains("if (segment >= pathSegmentLimit) break;"),
        "bounded path must enforce both the hard and adaptive segment ceilings"
    );
    assert!(
        frag.contains("if (shadedHits < min(int(MAX_SHADED_HITS), shadedHitLimit))"),
        "glossy chains must not expand the accepted local-light shadow-query ceiling"
    );

    // Every tier must fit under the ceiling, not just tier 3.
    for tier in 0..=3 {
        let s = AdaptiveRayBudget::settings_for_tier(tier);
        assert!(
            s.max_path_segments <= MAX_PATH_SEGMENTS && s.max_shaded_hits <= MAX_SHADED_HITS,
            "tier {tier} exceeds a shader-side ceiling and would be clamped"
        );
    }
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
    const FRAC_PI_4: f64 = std::f64::consts::FRAC_PI_4;

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
            for &phi in &[0.0, 0.3, FRAC_PI_4, 1.2, PI / 2.0] {
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

    // #3564 — bindings.glsl is the only declaration today; pin that so a
    // second mirror cannot appear outside this comparison.
    assert_mirror_list_is_complete(
        "struct GpuTerrainTile",
        &[("include/bindings.glsl", BINDINGS_GLSL)],
        "#2463 / #3564",
    );

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
        8,
        "GpuTerrainTile gained or lost a Rust field ({rust_fields:?}) — update the \
         GLSL mirror in include/bindings.glsl, the 144 B size pin, and the offset \
         pin above together (#2463). It was 3 fields / 96 B until #4057 added \
         §12.5's terrain receiver."
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
/// derive its surface colour through `rayHitAlbedo` (`texel × mat.diffuse*`,
/// plus the decal/tint/inner-layer/dark/detail roles composed since #3902),
/// never through `GpuInstance.avgAlbedo*`.
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
        src.contains("vec3 tColor = rayHitAlbedo(tMat, tUV, tAlbedo, refrMip);"),
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

// ── CPU→GPU light-boundary sibling pairs (#3575 / #3232 bug class) ──
//
// `GpuLight` carries three quantities that each have a near-identical
// sibling on the CPU side, and every recurrence of this bug class so far was
// a consumer or producer reaching for the wrong half: #3575 sized the
// soft-shadow disk off the 2x-inflated cull radius instead of the canonical
// source radius, and #3232 rotated a placed NIF light's position by its
// REFR rotation but not its direction. Both presented as visual artifacts
// someone happened to notice, months apart.
//
// `soft_shadow_disk_reads_the_canonical_source_radius_not_the_cull_radius`
// above pins #3575's two sites in `triangle.frag`. The tests below pin the
// class rather than the site: no shader anywhere may fabricate an emitter
// size from `position_radius.w`, and no shader may recover the authored
// influence range from it except through the one shared multiplier the
// upload used. The producer half of the position/direction pair is pinned
// CPU-side by `spawn_nif_lights_rotates_direction_by_reference_rotation`
// (`byroredux/src/cell_loader/nif_light_spawn_gate_tests.rs`).

/// Every GLSL source that reads `GpuLight`. Kept as one list so a new
/// light consumer has a single place to be added to, and so the two pins
/// below cannot silently cover three files while a fourth drifts.
const LIGHT_CONSUMER_SOURCES: &[(&str, &str)] = &[
    (
        "include/lighting.glsl",
        include_str!("../../../shaders/include/lighting.glsl"),
    ),
    (
        "triangle.frag",
        include_str!("../../../shaders/triangle.frag"),
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

/// Executable (non-comment) lines of a GLSL source. The prohibitions below
/// are about code, not about commentary explaining why the code is shaped
/// the way it is — the #3575 comment block quotes the forbidden expression
/// verbatim.
fn executable_glsl_lines(src: &str) -> Vec<&str> {
    src.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//") && !line.starts_with("*"))
        .collect()
}

/// #3575 generalized — `position_radius.w` is the CULL radius
/// (`range x LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`). The emitter's physical
/// size is `params.y`, derived once CPU-side by
/// `Emitter::from_legacy_world_units` as `(range * 0.05).clamp(1.0, 32.0)`
/// and by the procedural emitters as a real physical radius. Scaling `.w`
/// by a small constant reproduces `params.y` only in the unclamped middle;
/// it has no ceiling, disagrees at the floor, and makes a pure culling
/// tunable own shadow softness.
///
/// #3575 fixed the two `triangle.frag` sites and pinned them there. This
/// pins the *class* across every light consumer, so the next shader to want
/// "how big is this lamp" cannot reintroduce the derivation in a file the
/// original pin never looked at.
#[test]
fn no_shader_fabricates_an_emitter_size_from_the_cull_radius() {
    // `.w * k` for a small `k`, in either operand order, however the source
    // spells the read (`lights[i].position_radius.w`, a `radius` local, or
    // an unpacked `Lrad`).
    for (name, src) in LIGHT_CONSUMER_SOURCES {
        for line in executable_glsl_lines(src) {
            for needle in [
                "radius * 0.0",
                "radius * 0.1",
                "0.025 * radius",
                "position_radius.w * 0.",
                "0. * position_radius.w",
            ] {
                assert!(
                    !line.contains(needle),
                    "{name}: `{}` scales the CULL radius down to fabricate an \
                     emitter size. The canonical source radius is \
                     `lights[i].params.y` — see #3575 and the sibling-pair \
                     table on `GpuLight`.",
                    line.trim()
                );
            }
        }
    }
}

/// The authored influence range is the canonical CPU value; the cull radius
/// is what crosses the boundary, because four shaders need it as the
/// cull-window edge. Any shader wanting the authored range back must
/// therefore divide `position_radius.w` by the *same* multiplier
/// `gpu_light_from_emitter` multiplied by.
///
/// Before this pin, four sites spelled that recovery as a bare `0.5`, and
/// one of them (`pointSpotAtten`'s legacy arm) sourced it from
/// `dofParams.z` — a runtime *debug* tunable — so the bench knob for
/// REND-#1451 was the only thing in the tree that knew the upload geometry.
/// Dropping `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER` to 1.5 CPU-side would have
/// left every shader normalizing against the old 2.0 geometry with nothing
/// failing.
#[test]
fn authored_light_range_is_recovered_through_the_shared_cull_multiplier() {
    let glsl_multiplier = crate::shader_constants::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;
    assert_eq!(
        glsl_multiplier,
        byroredux_core::lighting::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER,
        "the renderer's GLSL-emitted multiplier must be the core constant \
         `gpu_light_from_emitter` uploads with"
    );

    // Both attenuation implementations — `pointSpotAtten` (surface) and
    // `froxelLightAtten` (volumetrics) — recover the authored range twice
    // each: once per attenuation model arm.
    for (name, src, expected_sites) in [
        (
            "include/lighting.glsl",
            include_str!("../../../shaders/include/lighting.glsl"),
            2usize,
        ),
        (
            "volumetrics_inject.comp",
            include_str!("../../../shaders/volumetrics_inject.comp"),
            2usize,
        ),
    ] {
        let executable = executable_glsl_lines(src);
        let sites = executable
            .iter()
            .filter(|line| line.contains("R / LEGACY_LIGHT_CULL_RANGE_MULTIPLIER"))
            .count()
            + executable
                .iter()
                .filter(|line| line.contains("1.0 / LEGACY_LIGHT_CULL_RANGE_MULTIPLIER"))
                .count();
        assert_eq!(
            sites, expected_sites,
            "{name}: expected {expected_sites} recoveries of the authored range \
             through LEGACY_LIGHT_CULL_RANGE_MULTIPLIER, found {sites}. A new \
             attenuation arm must derive it the same way, not re-spell `0.5`."
        );

        for line in &executable {
            let recovers_range = line.contains("authoredRange") || line.contains("kneeFrac");
            assert!(
                !(recovers_range && line.contains("* 0.5")),
                "{name}: `{}` recovers the authored influence range with a bare \
                 `0.5` instead of `/ LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`. That \
                 literal is only correct while the multiplier is exactly 2.0.",
                line.trim()
            );
        }
    }
}

/// Both GPU attenuation arms and the CPU reference
/// (`Emitter::distance_attenuation`, documented as "CPU reference for the
/// distance law implemented by surface and froxel shaders") must agree on
/// what the authored range is when a light's luminous surface is larger
/// than its influence range: `range.max(source_radius)`.
///
/// The inverse-square arms always folded `sourceRadius` in; the legacy
/// arms did not, so the one quantity three implementations were supposed
/// to share had two definitions. Procedural emitters — the ones with a
/// real physical `source_radius` rather than a range-derived one — are
/// exactly the class where the two diverge.
#[test]
fn both_attenuation_arms_clamp_the_authored_range_to_the_source_radius() {
    for (name, src, function) in [
        (
            "include/lighting.glsl",
            include_str!("../../../shaders/include/lighting.glsl"),
            "float pointSpotAtten(",
        ),
        (
            "volumetrics_inject.comp",
            include_str!("../../../shaders/volumetrics_inject.comp"),
            "float froxelLightAtten(",
        ),
    ] {
        let start = src
            .find(function)
            .unwrap_or_else(|| panic!("{name}: no longer declares `{function}`"));
        let body = &src[start..];
        let end = body
            .find("\n}\n")
            .unwrap_or_else(|| panic!("{name}: `{function}` body did not terminate"));
        let body = &body[..end];

        let folds = executable_glsl_lines(body)
            .iter()
            .filter(|line| {
                (line.contains("authoredRange") || line.contains("knee ="))
                    && line.contains("sourceRadius")
            })
            .count();
        assert_eq!(
            folds, 2,
            "{name}: `{function}` must fold `sourceRadius` into the authored \
             range in BOTH model arms, matching \
             `Emitter::distance_attenuation`'s `range.max(source_radius)`; \
             found {folds} of 2."
        );
    }
}

/// #3742 (TD2-2026-08-30-02) — `BLUE_NOISE_RANKS` (the shared 8×8
/// void-and-cluster rank table `composite.frag`'s `preResolveDither` and
/// `volumetrics_inject.comp`'s `blueNoiseRank` both sample) must be
/// declared in exactly one place — `include/blue_noise.glsl` — and both
/// consumers must `#include` it rather than re-declaring their own copy.
/// A re-declaration is exactly the failure mode this consolidation
/// closes: two copies that silently diverge produce correlated banding
/// that looks like a denoiser bug, not a constants bug.
#[test]
fn blue_noise_ranks_is_declared_exactly_once() {
    let header = include_str!("../../../shaders/include/blue_noise.glsl");
    let composite = include_str!("../../../shaders/composite.frag");
    let volumetrics = include_str!("../../../shaders/volumetrics_inject.comp");

    assert!(
        header.contains("const uint BLUE_NOISE_RANKS[64]"),
        "include/blue_noise.glsl must declare BLUE_NOISE_RANKS"
    );
    for (name, src) in [
        ("composite.frag", composite),
        ("volumetrics_inject.comp", volumetrics),
    ] {
        assert!(
            !src.contains("const uint BLUE_NOISE_RANKS[64]"),
            "{name} re-declares BLUE_NOISE_RANKS instead of #include-ing \
             include/blue_noise.glsl — the two copies can silently diverge (#3742)"
        );
        assert!(
            src.contains("#include \"include/blue_noise.glsl\""),
            "{name} must #include \"include/blue_noise.glsl\" to reach BLUE_NOISE_RANKS"
        );
    }
}

// ── EXAL ground-cover terrain sampling (#4052) ──────────────────────

/// The literal-offset guard above has a blind spot, and this closes it.
///
/// `rt_hit_shaders_have_no_unsafe_vertex_data_reads` scans for the *literal*
/// offsets `+ 20]` / `+ 21]`. `terrain_sample.glsl` reads those lanes through
/// the generated named constants instead, which is better style and completely
/// invisible to that scan — so a future edit could drop the
/// `unpackUnorm4x8(floatBitsToUint(...))` recovery and the guard would pass.
///
/// Splat lanes are 4× u8 unorm. Read as floats they are NaN or denormal
/// garbage, which reaches the density field as a silently wrong affinity term
/// rather than as a crash.
#[test]
fn named_splat_offset_reads_keep_their_unorm_recovery() {
    let sources = [
        (
            "terrain_sample.glsl",
            include_str!("../../../shaders/include/terrain_sample.glsl"),
        ),
        (
            "triangle.frag",
            include_str!("../../../shaders/triangle.frag"),
        ),
        (
            "groundcover_bench_bake.comp",
            include_str!("../../../shaders/groundcover_bench_bake.comp"),
        ),
    ];
    for (name, src) in sources {
        for (lineno, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            let reads_named_splat = line.contains("VERTEX_SPLAT0_OFFSET_FLOATS")
                || line.contains("VERTEX_SPLAT1_OFFSET_FLOATS");
            if reads_named_splat && line.contains("vertexData[") {
                assert!(
                    line.contains("unpackUnorm4x8") && line.contains("floatBitsToUint"),
                    "{name}:{}: reads a splat lane by named offset without the \
                     unorm recovery. Splat lanes are 4x u8 unorm, not floats — \
                     `unpackUnorm4x8(floatBitsToUint(vertexData[base + N]))`. \
                     See #575 / SH-1 and #4052.\nLine: {}",
                    lineno + 1,
                    line.trim()
                );
            }
        }
    }
}

/// The terrain grid reaches GLSL from `byroredux_core::math::coord` through
/// `build.rs`. This pins that the checked-in generated header actually carries
/// the current Rust values — the file is committed, so a stale copy survives
/// until something rebuilds, and a stale `LAND_GRID_VERTS` would make the
/// sampler index a different row.
#[test]
fn terrain_grid_constants_reach_glsl_unchanged() {
    let header = include_str!("../../../shaders/include/shader_constants.glsl");
    assert!(
        header.contains(&format!(
            "#define LAND_GRID_VERTS {}u",
            byroredux_core::math::coord::LAND_GRID_VERTS
        )),
        "LAND_GRID_VERTS drifted between core and the generated GLSL header"
    );
    assert!(
        header.contains(&format!(
            "#define LAND_VERTEX_SPACING {:?}",
            byroredux_core::math::coord::LAND_VERTEX_SPACING
        )),
        "LAND_VERTEX_SPACING drifted between core and the generated GLSL header"
    );
    assert!(
        header.contains(&format!(
            "#define EXTERIOR_CELL_UNITS {:?}",
            byroredux_core::math::coord::EXTERIOR_CELL_UNITS
        )),
        "EXTERIOR_CELL_UNITS drifted between core and the generated GLSL header"
    );
}

/// `byroSampleTerrain` inverts the mapping `cell_loader/terrain.rs` used to
/// build the vertices. Getting the **row sign** wrong samples a mirrored row
/// and produces terrain that looks plausible and is wrong, so it is worth a
/// real round-trip rather than only a source scan.
///
/// The forward mapping below is a deliberate test-local transcription of
/// `terrain.rs`'s documented formula:
///
/// ```text
/// bx = origin_x + col * SPACING   -> world X
/// by = origin_y + row * SPACING   -> world -Z   (Z-up -> Y-up flip)
/// ```
///
/// It exists only to validate the inverse, and is not a second production
/// definition of anything.
#[test]
fn terrain_sample_grid_mapping_inverts_the_terrain_builder() {
    use byroredux_core::math::coord::{LAND_GRID_VERTS, LAND_VERTEX_SPACING};

    // Forward: (grid cell, row, col) -> Y-up world XZ, per terrain.rs.
    let forward = |gx: i32, gy: i32, row: usize, col: usize| -> (f32, f32) {
        let origin_x = gx as f32 * byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
        let origin_y = gy as f32 * byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
        let bx = origin_x + col as f32 * LAND_VERTEX_SPACING;
        let by = origin_y + row as f32 * LAND_VERTEX_SPACING;
        (bx, -by) // zup_to_yup: x stays x, y becomes -z
    };
    // Inverse: exactly the two expressions in terrain_sample.glsl.
    let inverse = |origin_xz: (f32, f32), world_xz: (f32, f32)| -> (f32, f32) {
        let fc = (world_xz.0 - origin_xz.0) / LAND_VERTEX_SPACING;
        let fr = (origin_xz.1 - world_xz.1) / LAND_VERTEX_SPACING;
        (fr, fc)
    };

    for (gx, gy) in [(0, 0), (3, 7), (-2, 5), (-11, -4)] {
        let origin_xz = forward(gx, gy, 0, 0);
        for row in [0usize, 1, 16, LAND_GRID_VERTS - 1] {
            for col in [0usize, 1, 16, LAND_GRID_VERTS - 1] {
                let world_xz = forward(gx, gy, row, col);
                let (fr, fc) = inverse(origin_xz, world_xz);
                assert!(
                    (fr - row as f32).abs() < 1e-3,
                    "cell ({gx},{gy}) vertex (row {row}, col {col}): inverse \
                     recovered row {fr}, not {row}. A sign error here samples a \
                     mirrored row."
                );
                assert!(
                    (fc - col as f32).abs() < 1e-3,
                    "cell ({gx},{gy}) vertex (row {row}, col {col}): inverse \
                     recovered col {fc}, not {col}"
                );
            }
        }
    }
}

/// The GLSL must keep using the expressions the round-trip above validated.
/// The sign on the row term is the whole point — `worldXZ.y - cellOriginXZ.y`
/// would compile, run, and mirror the terrain.
#[test]
fn terrain_sampler_keeps_the_validated_grid_expressions() {
    let src = include_str!("../../../shaders/include/terrain_sample.glsl");
    assert!(
        src.contains("float fc = (worldXZ.x - cellOriginXZ.x) / LAND_VERTEX_SPACING;"),
        "column expression changed — re-validate against \
         terrain_sample_grid_mapping_inverts_the_terrain_builder"
    );
    assert!(
        src.contains("float fr = (cellOriginXZ.y - worldXZ.y) / LAND_VERTEX_SPACING;"),
        "row expression changed. The subtraction order is load-bearing: the \
         Z-up -> Y-up flip negates the row axis, so `worldXZ.y - cellOriginXZ.y` \
         silently mirrors every sampled row."
    );
    // Out-of-cell queries must be rejected, not clamped onto the boundary row.
    assert!(
        src.contains("return s;") && src.contains("s.valid = false;"),
        "the out-of-cell early-out disappeared; a clamped sample returns a \
         confident wrong answer for a point in the neighbouring cell"
    );
}

/// Path B must reproduce path A's arithmetic, or the bench compares two
/// different answers and calls the difference a cost.
///
/// Two things are pinned. The inverse mapping has to be the same expression
/// (same row sign — see the guard above), and the fetch has to be at texel
/// *centres*: `(idx + 0.5) / LAND_GRID_VERTS`. Dropping the half-texel biases
/// every sample by half a vertex spacing — 64 units — which still looks like
/// terrain and would show up only as path B being mysteriously "different".
#[test]
fn baked_sampler_matches_path_a_arithmetic() {
    let src = include_str!("../../../shaders/include/terrain_sample.glsl");
    let baked = src
        .split_once("TerrainSample byroSampleTerrainBaked(")
        .expect("terrain_sample.glsl must still define the path-B sampler (#4052)")
        .1;
    assert!(
        baked.contains("float fc = (worldXZ.x - cellOriginXZ.x) / LAND_VERTEX_SPACING;")
            && baked.contains("float fr = (cellOriginXZ.y - worldXZ.y) / LAND_VERTEX_SPACING;"),
        "the baked sampler must invert the grid with the same expressions path A \
         uses, or the two paths sample different points and the bench measures \
         nothing comparable"
    );
    assert!(
        baked.contains("vec2 uv = (vec2(fc, fr) + 0.5) / float(LAND_GRID_VERTS);"),
        "the baked fetch must land on texel centres. `(idx + 0.5) / \
         LAND_GRID_VERTS` is what makes a LINEAR fetch reproduce path A's \
         bilinear blend exactly; without the half-texel every sample is \
         biased by half a vertex spacing"
    );
    assert!(
        baked.contains("s.valid = false;"),
        "the baked sampler must reject out-of-cell queries like path A does. A \
         clamping path B would also read as cheaper, since the reject branch \
         is the one place the two paths can diverge in control flow"
    );
}

/// The bake writes vertex (row, col) into texel (col, row), and
/// `byroSampleTerrainBaked` reads it back as `uv = (fc, fr)` — column on x,
/// row on y. Transposing either half samples the terrain's mirror image
/// across its diagonal, which on real heightmaps is plausible terrain.
#[test]
fn bake_texel_indexing_matches_the_baked_fetch() {
    let bake = include_str!("../../../shaders/groundcover_bench_bake.comp");
    assert!(
        bake.contains("ivec3 texel = ivec3(int(col), int(row), int(layer));"),
        "the bake must write vertex (row, col) to texel (col, row); \
         `byroSampleTerrainBaked` reads column on x and row on y (#4052)"
    );
    assert!(
        bake.contains(
            "uint base = (cells[layer].vertexOffset + row * LAND_GRID_VERTS + col) \
             * VERTEX_STRIDE_FLOATS;"
        ),
        "the bake must index the vertex SSBO with terrain.rs's row-major \
         `row * LAND_GRID_VERTS + col`, offset by the covering instance's \
         vertex_offset"
    );
}

/// Both bench consumers share `include/groundcover_bench.glsl`'s candidate
/// generator. If either ever grew its own copy, the scatter and raster halves
/// would sample different points and the two `ns_sample` columns would stop
/// being comparable — which is the one comparison §4's store-vs-resample
/// trade is decided on.
#[test]
fn both_bench_consumers_share_one_candidate_generator() {
    let common = include_str!("../../../shaders/include/groundcover_bench.glsl");
    assert!(
        common.contains("vec2 benchCandidateWorld(BenchChunk chunk, uint index)"),
        "the shared header must still own the candidate-point generator"
    );
    for (name, src) in [
        (
            "groundcover_bench.comp",
            include_str!("../../../shaders/groundcover_bench.comp"),
        ),
        (
            "groundcover_bench.vert",
            include_str!("../../../shaders/groundcover_bench.vert"),
        ),
    ] {
        assert!(
            src.contains("#include \"include/groundcover_bench.glsl\""),
            "{name} must include the shared bench header rather than restating it"
        );
        assert!(
            src.contains("benchCandidateWorld("),
            "{name} must draw its sample points from the shared generator"
        );
        assert!(
            !src.contains("const float A1"),
            "{name} looks like it grew its own copy of the R2 lattice constants"
        );
    }
}

/// The sink is what stops an optimiser deleting the loads the bench exists to
/// time. It works only because `sinkScale` is a push constant — unknown at
/// compile time — rather than a literal the compiler can fold through.
#[test]
fn the_bench_sink_stays_unfoldable() {
    for (name, src) in [
        (
            "groundcover_bench.comp",
            include_str!("../../../shaders/groundcover_bench.comp"),
        ),
        (
            "groundcover_bench.vert",
            include_str!("../../../shaders/groundcover_bench.vert"),
        ),
    ] {
        assert!(
            src.contains("pc.sinkScale"),
            "{name} must route its accumulated sample through the push-constant \
             sink, or the sampling it is timing can be optimised away entirely"
        );
    }
    let vert = include_str!("../../../shaders/groundcover_bench.vert");
    assert!(
        vert.contains("gl_Position = vec4(sink, sink, 0.5, 1.0);"),
        "the raster half's blades must collapse onto one clip position: that is \
         what makes every triangle zero-area, so the fragment stage does no work \
         and the bracket measures the vertex stage"
    );
}

/// #3989 — `shader-pipeline.md` is the authoritative GPU-layout doc, and the
/// audit skill's own instruction is to trust it rather than re-derive. Two of
/// its rows described **live** lanes as free, which is how an author takes a
/// slot that already carries per-frame state and breaks it with no test
/// failure.
///
/// This is the third instance of that class in this struct family (#1928's
/// `VolumetricsParams.render_origin.w`, #2750's `GpuCamera.dof_params.zw`), so
/// the pin is on the doc text rather than on the code the doc describes: the
/// code was always right.
#[test]
fn shader_pipeline_doc_does_not_advertise_live_lanes_as_free() {
    const DOC: &str = include_str!("../../../../../docs/engine/shader-pipeline.md");

    // `GpuCamera.render_debug.w` — written by `pack_weather_surface`
    // (`context/assemble_camera_and_lights.rs`), decoded by `triangle.frag`.
    let render_debug = DOC
        .lines()
        .find(|line| line.contains("| `render_debug` |"))
        .expect("shader-pipeline.md must still document GpuCamera.render_debug");
    assert!(
        !render_debug.contains("w reserved"),
        "render_debug.w is not reserved — it carries the packed weather surface \
         (#3989): {render_debug}"
    );
    assert!(
        render_debug.contains("weather") && render_debug.contains("Not a free slot"),
        "the render_debug row must name its live consumer and carry the same \
         \"not a free slot\" warning the render_origin row above it already \
         does: {render_debug}"
    );

    // `material_flags` bit 10 — `material_flag::BGSM_AUTHORED`, host-side only.
    let bit10 = DOC
        .lines()
        .find(|line| line.starts_with("| 10 |"))
        .expect("shader-pipeline.md must still document material_flags bit 10");
    assert!(
        !bit10.contains("unused") && !bit10.contains("reserved"),
        "material_flags bit 10 is BGSM_AUTHORED, not free (#3989): {bit10}"
    );
    assert!(
        bit10.contains("BGSM_AUTHORED"),
        "the bit-10 row must name the flag that occupies it: {bit10}"
    );

    // The two facts the doc rows assert, checked against the code they describe
    // — so a future flag move breaks this test rather than the doc silently.
    assert_eq!(
        crate::vulkan::material::material_flag::BGSM_AUTHORED,
        1 << 10
    );
    assert!(
        include_str!("../../../src/shader_constants_data.rs")
            .contains("`material_flag::BGSM_AUTHORED` (Rust-side bit 10) is"),
        "the \"intentionally NOT emitted\" note the doc row cites must still exist"
    );
}

/// #4019 (REN-2026-09-06-D2-04) — `shader-pipeline.md`'s Set-0/Set-1
/// descriptor table must not credit the two private-layout passes with global
/// bindings they cannot see.
///
/// `CausticPipeline` and the volumetrics pipelines each build their
/// `VkPipelineLayout` from a single descriptor set layout — their own private
/// `set = 0` — so neither the bindless Set 0 nor the scene Set 1 is reachable
/// from them. The table nonetheless listed `caustic`/`caustic_splat` under
/// Set-0 bindings 0-1 and Set-1 bindings 0, 1, 2 and 4, and `volumetrics`
/// under Set-0 binding 0 and Set-1 bindings 1 and 2 — while the page's own
/// prose three paragraphs later said volumetrics "neither binds any Set-1
/// resource above".
///
/// This is the table every audit is told to prefer over re-deriving descriptor
/// facts from source, and the one that answers "which pipelines must be
/// re-bound when Set 1 changes". Reading it the other way — that
/// `caustic_splat`'s instance reads are covered by the Set-1 lockstep guards —
/// is the mistake that produced #3829; they are a separate private mirror.
#[test]
fn the_descriptor_table_does_not_credit_private_layout_passes_with_global_sets() {
    const DOC: &str = include_str!("../../../../../docs/engine/shader-pipeline.md");
    const CAUSTIC_SRC: &str = include_str!("../caustic.rs");
    // #2256 split the volumetrics construction path into `volumetrics/init.rs`,
    // taking both `set_layouts` sites with it. Scanned as one string with the
    // parent so this pin does not care which side of that seam the pipeline
    // layout is built on — the property is "one set layout per pipeline
    // layout", not "in this file".
    const VOLUMETRICS_SRC: &str = include_str!("../volumetrics.rs");
    const VOLUMETRICS_INIT_SRC: &str = include_str!("../volumetrics/init.rs");
    let volumetrics_all = format!("{VOLUMETRICS_SRC}{VOLUMETRICS_INIT_SRC}");

    // Ground truth: one set layout per pipeline layout. Needle composed at
    // runtime so this test's own text cannot satisfy the scan.
    let single_set = format!("set_layouts(std::slice::from_ref{}", "(");
    for (name, src) in [
        ("caustic.rs", CAUSTIC_SRC),
        ("volumetrics.rs + volumetrics/init.rs", &volumetrics_all[..]),
    ] {
        assert!(
            src.contains(&single_set),
            "{name} no longer builds its pipeline layout from a single set \
             layout — if it now binds a global set, the descriptor table in \
             shader-pipeline.md must gain those rows back rather than this \
             test being deleted (#4019)"
        );
    }

    // The Set-0 / Set-1 block runs from the table header to the first `| 2 |`
    // row (set 2 is water's own, and legitimately private).
    let table_start = DOC
        .find("| Set | Binding | Type | Resource | Used by |")
        .expect("shader-pipeline.md has a descriptor-set table");
    let table_end = table_start
        + DOC[table_start..]
            .find("\n| 2 | 0 |")
            .expect("the table reaches set 2");
    let global_rows = &DOC[table_start..table_end];

    for pass in ["caustic", "volumetrics"] {
        let offenders: Vec<&str> = global_rows
            .lines()
            .filter(|line| line.starts_with("| 0 |") || line.starts_with("| 1 |"))
            .filter(|line| {
                // Only the trailing "Used by" cell names pipelines; the
                // Resource cell legitimately mentions e.g. "caustic
                // accumulator".
                line.rsplit('|')
                    .nth(1)
                    .is_some_and(|used_by| used_by.contains(pass))
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "shader-pipeline.md credits `{pass}` with a Set-0/Set-1 binding, but \
             its pipeline layout has one set — its own private `set = 0`. \
             Offending row(s):\n{}",
            offenders.join("\n")
        );
    }
}

/// #4016 (REN-2026-09-06-D19-01) — every LAND TX01 splat sampler uses explicit
/// gradients, because every one of them sits under a per-fragment `continue`.
///
/// The three loops (diffuse, specular, normal) each skip layers with
/// `if (w <= 0.0) continue`, and `w` is an interpolated per-fragment splat
/// weight — so at a splat boundary one lane of a quad reaches the sampler on
/// iteration k while its neighbour has already continued. Implicit derivatives
/// are spec-undefined there: the class #3622 converted `parallaxDisplaceUV`
/// away from, and that `ray_hit.glsl`'s `resolveRayHitUV` has always avoided.
///
/// Pins the shape rather than the count: a fourth splat loop added later with
/// a plain `texture()` fails this.
#[test]
fn every_terrain_splat_sampler_uses_explicit_gradients() {
    const FRAG: &str = include_str!("../../../shaders/triangle.frag");

    // The gradients must be captured once, OUTSIDE any splat loop.
    for capture in [
        "vec2 splatUVdx = dFdx(sampleUV);",
        "vec3 splatPosdx = dFdx(fragWorldPosRel);",
    ] {
        assert!(
            FRAG.contains(capture),
            "triangle.frag no longer captures `{capture}` — the splat loops \
             need gradients from quad-uniform flow (#4016)"
        );
    }

    // Walk each `for` loop that reads a terrain splat weight and require every
    // sampler inside it to be an explicit-gradient form.
    let lines: Vec<&str> = FRAG.lines().collect();
    let mut checked = 0usize;
    for (start, line) in lines.iter().enumerate() {
        if !line.contains("for (uint i = 0u; i < 8u; ++i)") {
            continue;
        }
        let body: Vec<&str> = lines[start..]
            .iter()
            .take_while(|l| !l.trim_start().starts_with("}\n"))
            .take(20)
            .copied()
            .collect();
        let body_text = body.join("\n");
        if !body_text.contains("terrainSplat[i / 4u][i & 3u]") {
            continue;
        }
        checked += 1;
        for l in &body {
            let code = l.split("//").next().unwrap_or("");
            let implicit = code.contains("= texture(")
                || code.contains("texture(\n")
                || (code.contains("perturbNormal(") && !code.contains("perturbNormalGrad("));
            assert!(
                !implicit,
                "triangle.frag:{}: a terrain-splat loop samples with an \
                 implicit-derivative form under a per-fragment `continue`. Use \
                 `textureGrad` / `perturbNormalGrad` with the gradients captured \
                 before the loop (#4016).\nLine: {}",
                start + 1,
                code.trim()
            );
        }
    }
    assert_eq!(
        checked, 3,
        "expected the three LAND TX01 splat loops (diffuse, specular, normal); \
         found {checked} — the scan broke, or a loop was added/removed without \
         updating this pin"
    );
}

/// #4017 (REN-2026-09-06-D2-02) — `GI_VISIBLE_LIGHT_CAP` has a live consumer.
///
/// The constant was promoted into `shader_constants_data.rs` on the strength
/// of `giHitIrradiance`'s copy, but that function had had no caller since
/// `f8efde63` (2026-07-29). Retuning the constant changed nothing on screen
/// while the number that actually ran sat in `triangle.frag` as a bare `2u`.
/// #3880's gate cannot catch that: it matches *declarations*, and a literal
/// inlined into an expression is invisible to it by construction.
#[test]
fn the_gi_visible_light_cap_is_wired_to_the_live_path_budget() {
    const FRAG: &str = include_str!("../../../shaders/triangle.frag");
    const LIGHTING: &str = include_str!("../../../shaders/include/lighting.glsl");
    const RAYTRACE: &str = include_str!("../../../shaders/include/raytrace.glsl");

    // Needle composed at runtime — this test names the function it forbids.
    //
    // Comments are stripped before the scan. The three sites #4017 corrected
    // explain *why* the function is gone and necessarily name it; that prose
    // is the record and must stay legal. What must not come back is a
    // reference in code — a call, or a prototype for a function with no
    // caller, which is the state this closed.
    let dead = format!("giHit{}", "Irradiance");
    for (name, src) in [
        ("lighting.glsl", LIGHTING),
        ("triangle.frag", FRAG),
        ("include/raytrace.glsl", RAYTRACE),
    ] {
        let offender = src.lines().enumerate().find(|(_, raw)| {
            let code = match raw.find("//") {
                Some(i) => &raw[..i],
                None => raw,
            };
            code.contains(&dead)
        });
        assert!(
            offender.is_none(),
            "{name}:{} references `{dead}` in code, which #4017 deleted as \
             unreachable. If it is being reinstated it needs a caller — and \
             `GI_VISIBLE_LIGHT_CAP` then has two consumers again, which is the \
             state #3880 removed.\nLine: {}",
            offender.expect("checked").0 + 1,
            offender.expect("checked").1.trim()
        );
    }

    assert!(
        FRAG.contains("shadedHits == 0 ? GI_VISIBLE_LIGHT_CAP : 1u"),
        "triangle.frag's `visibleLightLimit` no longer reads \
         `GI_VISIBLE_LIGHT_CAP`. That call site is the only live consumer of the \
         constant; inlining the value back makes retuning it in \
         shader_constants_data.rs a silent no-op (#4017)"
    );
}

/// #4016 — the implicit-derivative wrapper must stay a pure delegation.
///
/// Two pins above (`every_gram_schmidt_tangent_frame_guards_the_projected_length`
/// and `perturb_normal_guards_post_projection_tangent_length`) follow the TBN
/// guards into `perturbNormalGrad`, where the body now lives. That is only
/// sound while `perturbNormal` contains no tangent-frame construction of its
/// own — a second, unguarded frame grown in the wrapper would be invisible to
/// both of them, which is precisely the hole a rename can open.
#[test]
fn the_perturb_normal_wrapper_only_delegates() {
    let src = include_str!("../../../shaders/include/material_sampling.glsl");
    let body = glsl_fn_body(src, "vec3 perturbNormal(vec3 N, vec3 worldPos");

    assert!(
        body.contains("return perturbNormalGrad("),
        "perturbNormal must delegate to the guarded explicit-gradient core \
         rather than carry a body of its own (#4016)"
    );
    for forbidden in [
        "mat3(",
        "dot(T, N)",
        "cross(N,",
        "textureGrad(",
        "= texture(",
    ] {
        assert!(
            !body.contains(forbidden),
            "perturbNormal grew `{forbidden}` — it is the delegation wrapper, \
             and the TBN guards two other tests enumerate live in \
             `perturbNormalGrad`. A frame built here is guarded by nothing \
             (#4016 / #2815 / #3984)"
        );
    }
}

// ---- #3308 depth-convention parity ----

/// The GLSL depth convention must come from the engine's constant, and from
/// nowhere else.
///
/// `ACTIVE_DEPTH_MAPPING` (core) drives the projection, the depth clear and
/// every pipeline's compare state; `build.rs` generates `BYRO_REVERSED_Z` /
/// `BYRO_DEPTH_CLEAR` from it into `include/shader_constants.glsl`, which is
/// what `include/depth_convention.glsl`'s predicates read.
///
/// **What this test does and does not guard.** The header-vs-constant
/// comparison below cannot fail while generation is wired, because the build
/// script regenerates the header before this crate compiles and `include_str!`
/// therefore always reads a fresh one. That is the point: a hand-maintained
/// GLSL copy could drift, a generated one cannot, and drift made impossible
/// beats drift detected. The assertion stays as the executable statement of
/// which value the shaders are entitled to see.
///
/// What it does catch is the two ways that wiring can be cut: a
/// `DEPTH_CLEAR_VALUE` that stops deriving from the mapping, and a
/// `depth_convention.glsl` that grows private `#define`s again instead of
/// including the generated header. Either restores the drift this issue
/// removed, and the consequence is not subtle — under reversed-Z with a stale
/// `BYRO_REVERSED_Z == 0`, `depthIsBackground` answers "background" for every
/// surface and "surface" for the sky, so composite swaps sky and geometry
/// shading across the whole image.
///
/// Stale *SPIR-V* after a flip is the remaining hazard, and it belongs to
/// `scripts/check-shader-artifacts.sh`, which recompiles every shader and
/// compares bytes.
#[test]
fn the_shader_depth_convention_matches_the_engine_constant() {
    use byroredux_core::ecs::components::camera::{DepthMapping, ACTIVE_DEPTH_MAPPING};

    let generated = include_str!("../../../shaders/include/shader_constants.glsl");
    let expected = match ACTIVE_DEPTH_MAPPING {
        DepthMapping::Conventional => "#define BYRO_REVERSED_Z 0",
        DepthMapping::Reversed => "#define BYRO_REVERSED_Z 1",
    };
    assert!(
        generated.contains(expected),
        "ACTIVE_DEPTH_MAPPING is {ACTIVE_DEPTH_MAPPING:?}, so the generated \
         include/shader_constants.glsl must contain `{expected}` — if it does \
         not, build.rs has stopped deriving it from the mapping"
    );

    // The clear value the shaders test against is the one the renderer writes.
    let expected_clear = match ACTIVE_DEPTH_MAPPING {
        DepthMapping::Conventional => "#define BYRO_DEPTH_CLEAR 1.0",
        DepthMapping::Reversed => "#define BYRO_DEPTH_CLEAR 0.0",
    };
    assert!(
        generated.contains(expected_clear),
        "the generated GLSL clear constant must be `{expected_clear}` to match \
         DEPTH_CLEAR_VALUE = {}",
        crate::vulkan::pipeline::DEPTH_CLEAR_VALUE
    );
    assert_eq!(
        crate::vulkan::pipeline::DEPTH_CLEAR_VALUE,
        ACTIVE_DEPTH_MAPPING.clear_value()
    );

    // ...and the predicates are defined against those two, not against
    // literals of their own.
    let convention = include_str!("../../../shaders/include/depth_convention.glsl");
    assert!(convention.contains(r#"#include "include/shader_constants.glsl""#));
    assert!(!convention.contains("#define BYRO_REVERSED_Z"));
    assert!(!convention.contains("#define BYRO_DEPTH_CLEAR"));
    for predicate in [
        "bool depthIsBackground(float z)",
        "bool depthIsSurface(float z)",
        "bool depthIsBackgroundEps(float z, float eps)",
    ] {
        assert!(convention.contains(predicate), "missing {predicate}");
    }
}

/// No shader may test a depth sample against a raw end-of-range literal.
///
/// The #3308 conversion's whole cost was that this convention was restated in
/// six places across three shaders, each of which had to be found by reading.
/// `include/depth_convention.glsl` is now the single place that knows which
/// way the buffer runs; a re-introduced `depth < 1.0` (or `>= 0.999`) would be
/// correct today and silently wrong the moment the mapping flips — exactly the
/// class of latent breakage this issue existed to remove.
///
/// The scan flags a **bare** identifier ending in `…epth` (so `depth`,
/// `sampleDepth`, `candidateDepth`, `referenceDepth` — but not a struct member
/// like `mat.softFalloffDepth`, which is a material distance, not a buffer
/// sample) compared against a **complete** range-endpoint literal (`1.0`,
/// `0.0`, `0.999`, `0.001` — not `1.0e-3`, which is how `opticalDepth`, an
/// extinction integral rather than a depth sample, is compared). Both
/// qualifiers are load-bearing: dropping either turns this into three false
/// positives on shaders that have nothing to do with the depth buffer.
///
/// `the_depth_literal_scanner_recognises_the_shapes_it_exists_to_catch` is the
/// completeness half — it holds the matcher to the forms actually removed
/// here, so a scan narrowed until it passes vacuously fails there instead.
#[test]
fn no_shader_tests_depth_against_a_raw_range_literal() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut files = Vec::new();
    collect_shader_files(&dir, &mut files);
    assert!(
        files.len() > 20,
        "expected the full shader tree, got {}",
        files.len()
    );

    let convention = dir.join("include").join("depth_convention.glsl");
    let mut offenders = Vec::new();

    for path in &files {
        // The convention header is the one file allowed to name the range.
        if *path == convention {
            continue;
        }
        let src = std::fs::read_to_string(path).expect("read shader");
        for (lineno, raw) in src.lines().enumerate() {
            if raw_depth_range_test(raw) {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.file_name().unwrap().to_string_lossy(),
                    lineno + 1,
                    raw.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these compare a depth sample against a raw range literal instead of \
         using include/depth_convention.glsl's depthIsSurface / \
         depthIsBackground / depthIsBackgroundEps — they would silently invert \
         under reversed-Z (#3308):\n  {}",
        offenders.join("\n  ")
    );
}

/// True when a line of GLSL compares a bare `…epth` identifier against a
/// complete depth-range endpoint literal. See
/// `no_shader_tests_depth_against_a_raw_range_literal` for the rule.
fn raw_depth_range_test(raw: &str) -> bool {
    // Comments describe the convention; only code is scanned.
    let line = match raw.find("//") {
        Some(i) => &raw[..i],
        None => raw,
    };

    let bytes = line.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = line[from..].find("epth") {
        let at = from + rel;
        from = at + 4;

        // Bare identifier only: a member access names a material/push field,
        // not a depth-buffer sample.
        let ident_start = line[..at]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(0, |i| i + 1);
        if ident_start > 0 && bytes[ident_start - 1] == b'.' {
            continue;
        }
        // ...and the identifier must *end* at "epth", not merely contain it.
        let rest = &line[at + 4..];
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            continue;
        }

        let rest = rest.trim_start();
        // Longest operator first so "<=" is not read as "<".
        let Some(op) = ["<=", ">=", "==", "!=", "<", ">"]
            .into_iter()
            .find(|o| rest.starts_with(o))
        else {
            continue;
        };
        let value = rest[op.len()..].trim_start();

        // A complete numeric token — `1.0e-3` is an extinction threshold, not
        // an end of the normalised depth range.
        let end = value
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(value.len());
        let (number, tail) = value.split_at(end);
        if tail.starts_with(['e', 'E', 'f', 'F']) {
            continue;
        }
        if matches!(number, "1.0" | "0.0" | "0.999" | "0.001") {
            return true;
        }
    }
    false
}

/// The completeness half of the scan above: every form actually removed by
/// #3308 must still be recognised, and the two shapes deliberately excluded
/// must still be ignored.
///
/// Without this, narrowing `raw_depth_range_test` until the tree is clean
/// would look like a passing suite.
#[test]
fn the_depth_literal_scanner_recognises_the_shapes_it_exists_to_catch() {
    // The six real sites this issue converted, as they read before the fix.
    for caught in [
        "    bool has_surface = depth < 1.0;",
        "    bool referenceIsSurface = referenceDepth < 1.0;",
        "    bool candidateIsSurface = candidateDepth < 1.0;",
        "    if (depth >= 0.999) {",
        "        if (sampleDepth >= 0.999) continue;",
        "    if (depth < 1.0) {",
        // ...and the reversed-Z spellings, so a half-converted flip is caught too.
        "    bool has_surface = depth > 0.0;",
        "    if (depth <= 0.001) {",
    ] {
        assert!(
            raw_depth_range_test(caught),
            "scanner missed a raw depth-range test: {caught}"
        );
    }

    // The three shapes that are not depth-buffer background tests.
    for ignored in [
        "        float sourceIntegral = opticalDepth < 1.0e-3",
        "    float escapeProbability = opticalDepth > 1.0e-4",
        "            && mat.softFalloffDepth > 0.0) {",
        // A comment describing the old convention stays legal — the corrected
        // prose explaining *why* the literals are gone is the record.
        "//   For sky pixels (depth == 1.0, exterior only):",
        // Reading a sample is not testing one.
        "    float depth = texelFetch(depthTex, px, 0).r;",
    ] {
        assert!(
            !raw_depth_range_test(ignored),
            "scanner false-positived on: {ignored}"
        );
    }
}

/// A depth *decode* is the other half of the convention, and the easier half
/// to miss.
///
/// Reconstructing a world position through `inv_view_proj` needs to know
/// nothing about the mapping — the inverse matrix already carries it, which is
/// why `composite.frag`, `ssao.comp` and `caustic_splat.comp` all reconstruct
/// positions without touching `include/depth_convention.glsl`. Recovering a
/// *linear eye distance* from a raw sample is different: it inverts the
/// encoding directly, so it has to flip with it.
///
/// `composite.frag`'s `linearViewDepth` was exactly that — an inlined
/// `n*f / (f - z*(f-n))`, correct today and silently wrong under reversed-Z,
/// feeding the froxel bilateral's depth-compatibility test and the height-fog
/// term. It now delegates to `depthLinearize`, whose two branches are the
/// GLSL twins of `Camera::linear_distance_from_depth` and
/// `linear_distance_from_depth_reversed`.
///
/// The scan below is the general form: no shader may divide by a
/// `farPlane - <depth> * (...)` denominator of its own.
#[test]
fn every_depth_linearisation_goes_through_the_convention_header() {
    let convention = include_str!("../../../shaders/include/depth_convention.glsl");
    assert!(convention.contains("float depthLinearize(float z, float nearPlane, float farPlane)"));
    // Both branches present — a header that lost the reversed arm would still
    // compile and still be wrong the moment the mapping flips.
    assert!(convention.contains("float denom = z * (1.0 - nOverF) + nOverF;"));
    assert!(convention.contains("float denom = 1.0 - z * (1.0 - nOverF);"));

    let composite = include_str!("../../../shaders/composite.frag");
    assert!(
        composite.contains("return depthLinearize(deviceDepth, nearPlane, farPlane);"),
        "composite's linearViewDepth must delegate to the convention header"
    );

    // No shader may re-inline a conventional linearisation. The needle is the
    // shape of that inverse's denominator — a `farPlane`/`far`-minus-depth
    // product — composed at runtime so this test's own source cannot match it.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
    let mut files = Vec::new();
    collect_shader_files(&dir, &mut files);
    let needle = format!("{}{}", "farPlane - ", "deviceDepth");
    let mut offenders = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).expect("read shader");
        if src.contains(&needle) {
            offenders.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        offenders.is_empty(),
        "these inline a conventional depth linearisation instead of calling \
         depthLinearize (#3308): {offenders:?}"
    );
}

/// #3927 — the lit palette branch must sample the LUT's V axis with the
/// authored scalar, not pin it to a constant.
///
/// `grayscaleToPaletteScale` is the palette ROW into a 2D atlas whose U axis
/// is the greyscale ramp — not a lerp weight. Measured on vanilla FO4
/// (`Fallout4 - Materials.ba2`, 287 LUT-bearing BGSMs): `bricks01grad01.dds`
/// is 32x128 with 107 distinct texel rows and is referenced at 9 distinct
/// scales; `hittechmetalpanel_01lgrad.dds` is 32x128 with all 128 rows
/// distinct at 11 scales; `stationwagon01lgrad.dds` takes 25. Along U the
/// colour ramps, down V the palette changes outright. A fixed `v = 0.5`
/// collapsed every material sharing an atlas onto row 64.
///
/// This is a render-visible change that `cargo test` cannot see the output
/// of, so this test pins the *contract* — that the scalar reaches the V
/// coordinate and no `mix()` survives in the branch — while the visual half
/// stays a screenshot/RenderDoc job on FO4 brick geometry.
///
/// The `MATERIAL_KIND_EFFECT_SHADER` branch above deliberately keeps its own
/// `v = 0.5` (#890 Stage 2c, reasoned about Skyrim's semantically-1D 64x64 FX
/// atlases) and is not covered here.
#[test]
fn lit_palette_branch_indexes_the_lut_row_by_the_authored_scale() {
    let frag = include_str!("../../../shaders/triangle.frag");

    // The lit branch is the second `greyscaleLutIndex != 0u` guard; the first
    // belongs to the effect-shader path, which keeps v = 0.5 on purpose.
    let guard = "mat.greyscaleLutIndex != 0u";
    let first = frag.find(guard).expect("effect palette branch");
    let lit_start = frag[first + guard.len()..]
        .find(guard)
        .map(|i| first + guard.len() + i)
        .expect("lit palette branch");
    let lit_end = frag[lit_start..]
        .find("\n    }")
        .map(|i| lit_start + i)
        .expect("end of the lit palette branch");
    let branch = &frag[lit_start..lit_end];

    assert!(
        branch.contains("clamp(mat.grayscaleToPaletteScale, 0.0, 1.0))")
            && branch.contains("vec2(gsIndex,"),
        "the lit palette branch must sample at vec2(gsIndex, <authored scale>) \
         — the scalar is the atlas ROW (#3927). Branch text:\n{branch}"
    );
    assert!(
        !branch.contains("0.5)"),
        "the lit palette branch must not pin V to a constant row; that made \
         every material sharing an atlas render the same colour (#3927)"
    );
    assert!(
        !branch.contains("mix("),
        "the palette remap is a REPLACE, not a lerp — the mix() was what a \
         blend-weight reading of grayscaleToPaletteScale required (#3927 \
         corrects #2443's premise). Branch text:\n{branch}"
    );
}
