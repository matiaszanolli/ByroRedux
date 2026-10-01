//! Cornell-box test harness — a self-contained reference scene for
//! validating ray-traced materials and lighting without on-disk game
//! data. Activated with the `--cornell` CLI flag (handled in
//! [`crate::scene::setup_scene`]).
//!
//! # Two lighting variants
//!
//! `--cornell` is **interior / point-light only**: a closed box lit by a
//! ceiling panel + a camera-side fill, `CellLightingRes.directional_color`
//! zeroed and no `SkyParamsRes`. Every sun-driven path — directional
//! BRDF + RT sun shadows, the volumetric froxel sun injection, the
//! Effect_Lit sun shading, the composite sky — is therefore **inert**, and
//! a "sun looks wrong" regression bisected against it returns a false
//! all-clear (#1942).
//!
//! `--cornell-sun` is the exterior counterpart: same probe set, ceiling
//! removed, *all* local lights dropped, and the canonical procedural
//! exterior environment installed ([`procedural_fallback_cell_lighting`] +
//! [`procedural_fallback_sky`], the same constructors a plugin-less
//! exterior load uses) with a fixed [`SUN_DIR_RAW`]. The sun is then the only
//! light in the scene, so any sign flip / axis swap / dropped term in the
//! directional chain shows up as a moved or missing shadow rather than a
//! plausible-looking image. No `WeatherDataRes` is inserted, so the sun
//! does not drift with TOD.
//!
//! #3561 — that used to read "`weather_system` stays inert", which was
//! never true once `ensure_game_time` started running for every scene kind:
//! the system's `GameTimeRes` guard passes here, and its missing-
//! `WeatherDataRes` branch reached `apply_neutral_exterior_fallback`, which
//! rebuilt `CellLightingRes` from a hardcoded hour-6 sun and returned before
//! the `SkyParamsRes` write — leaving the shading directional and the
//! painted sun disc ~48 degrees apart from frame 1. `weather_system` still
//! runs; what makes the direction stable is that the fallback now preserves
//! the direction it was handed instead of inventing one.
//!
//! The scene is the classic Cornell box (white floor/ceiling/back wall,
//! red left wall, green right wall, a ceiling area light) populated with
//! probe objects chosen to exercise specific RT behaviours:
//!
//!   * a tall matte block + a matte sphere — GI color bleeding, soft
//!     contact shadows;
//!   * a 5-sphere **roughness sweep** (metal) and a 5-sphere
//!     **metalness sweep** — GGX highlight shape, RT reflections, and
//!     the renderer's roughness reflection-gate;
//!   * a glass sphere + glass cube — `MATERIAL_KIND_GLASS` IOR
//!     refraction / transmission;
//!   * an emissive cube — emissive contribution to GI + bloom.
//!
//! Every probe carries a [`Name`] and a live-mutable [`Material`], so the
//! `mat.*` console commands (see [`crate::commands`]) can sweep material
//! parameters at runtime and watch the RT response — no rebuild needed.
//! All geometry uses a flat-white vertex color; surface color is driven
//! entirely through `Material::diffuse_color` so a single
//! `mat.set <id> color r g b` tweak fully recolors a probe.

use byroredux_core::combustion::CombustionRegime;
use byroredux_core::ecs::components::groundcover::WindField;
use byroredux_core::ecs::{
    CombustionState, FogBounds, FogProfile, FogShape, FogSource, FogVolume, GlobalTransform,
    LightSource, Material, MeshHandle, TextureHandle, TotalTime, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::import::ImportedMaterial;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{
    MATERIAL_KIND_FIRE_REFRACTION, MATERIAL_KIND_GLASS, RenderDebugMode, SceneMeshUpload,
    VulkanContext, box_vertices_colored, uv_sphere,
};
use byroredux_sdk::studio::CornellFit;

use crate::components::{CellLightingRes, MaterialTextureHandles};
use crate::env_translate::{procedural_fallback_cell_lighting, procedural_fallback_sky};

/// Classic Cornell wall albedos (linear). Gamebryo colors are raw
/// monitor-space floats and must NOT be sRGB-decoded (see the
/// `feedback_color_space` memory), so these are used verbatim as
/// `Material::diffuse_color`.
const WHITE: [f32; 3] = [0.73, 0.73, 0.73];
const RED: [f32; 3] = [0.65, 0.05, 0.05];
const GREEN: [f32; 3] = [0.12, 0.45, 0.15];

/// Room half-extents (world units). The box spans `x,z ∈ [-HALF_W, HALF_W]`
/// and `y ∈ [0, HEIGHT]`; the front (`+Z`) is left open for the camera.
const HALF_W: f32 = 4.0;
const HEIGHT: f32 = 5.0;
/// Wall slab half-thickness.
const T: f32 = 0.05;

// #5090 — the five harness scenes this file accumulated split into
// `cornell/` submodule files; this root keeps the classic Cornell box,
// the SDK studio room, and the family's shared re-exports.
mod builders;
use builders::*;
use oracle::sun_dir;
mod combustion_lab;
mod glass_dragon;
mod godray_lab;
mod oracle;

pub(crate) use combustion_lab::{combustion_lab_mode, combustion_lab_nuclear_mode, setup_combustion_lab_scene};
pub(crate) use glass_dragon::{glass_dragon_mode, setup_cornell_glass_dragon_scene};
pub(crate) use godray_lab::{godray_lab_mode, setup_godray_lab_scene};
pub(crate) use oracle::{cornell_oracle_rung, cornell_oracle_world_offset, setup_cornell_oracle_scene};

/// Build the SDK Studio's open-front Cornell room around an imported asset.
/// Room sizing is owned by `byroredux-sdk`; this host function only uploads
/// the generated geometry and installs canonical neutral lighting.
pub(crate) fn setup_studio_room(
    world: &mut World,
    ctx: &mut VulkanContext,
    fit: CornellFit,
) -> (Vec3, Vec3) {
    install_cornell_lighting(world, false);
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let center = Vec3::from_array(fit.center);
    let room_center_y = fit.floor_y + fit.height * 0.5;
    let thickness = fit.wall_thickness;
    let horizontal = [fit.half_width, thickness, fit.half_depth];
    let back = [fit.half_width, fit.height * 0.5, thickness];
    let side = [thickness, fit.height * 0.5, fit.half_depth];
    // One mesh per slab, never shared between two entities: the Studio
    // gallery rebuilds this room on every change and reclaims it through
    // `unload_cell`, which drops one mesh reference per holder. A shared
    // upload (refcount 1, two holders) would be released twice and keep its
    // BLAS alive over a freed buffer.
    for (half_extents, position, color, name) in [
        (
            horizontal,
            Vec3::new(center.x, fit.floor_y - thickness, center.z),
            WHITE,
            "studio_floor",
        ),
        (
            horizontal,
            Vec3::new(center.x, fit.floor_y + fit.height + thickness, center.z),
            WHITE,
            "studio_ceiling",
        ),
        (
            back,
            Vec3::new(
                center.x,
                room_center_y,
                center.z - fit.half_depth - thickness,
            ),
            WHITE,
            "studio_back_wall",
        ),
        (
            side,
            Vec3::new(
                center.x - fit.half_width - thickness,
                room_center_y,
                center.z,
            ),
            RED,
            "studio_left_wall",
        ),
        (
            side,
            Vec3::new(
                center.x + fit.half_width + thickness,
                room_center_y,
                center.z,
            ),
            GREEN,
            "studio_right_wall",
        ),
    ] {
        let mesh = builder.box_mesh(half_extents);
        spawn_object(
            world,
            mesh,
            neutral,
            position,
            Quat::IDENTITY,
            matte(color),
            name,
        );
    }
    let panel = builder.box_mesh([
        (fit.half_width * 0.22).max(thickness),
        thickness * 0.4,
        (fit.half_depth * 0.22).max(thickness),
    ]);
    let light_y = fit.floor_y + fit.height - thickness * 1.5;
    spawn_object(
        world,
        panel,
        neutral,
        Vec3::new(center.x, light_y, center.z),
        Quat::IDENTITY,
        emissive([1.0, 0.97, 0.9], 8.0),
        "studio_light_panel",
    );
    spawn_point_light(
        world,
        Vec3::new(
            center.x,
            light_y - fit.height * 0.08,
            center.z + fit.half_depth * 0.1,
        ),
        fit.half_width.max(fit.half_depth).max(fit.height) * 4.0,
        [1.6, 1.55, 1.45],
        "studio_key_light",
    );
    spawn_point_light(
        world,
        Vec3::new(
            center.x - fit.half_width * 0.4,
            fit.floor_y + fit.height * 0.65,
            center.z + fit.half_depth * 0.7,
        ),
        fit.half_width.max(fit.half_depth).max(fit.height) * 3.0,
        [0.55, 0.58, 0.65],
        "studio_camera_fill",
    );
    builder.finish();
    (
        Vec3::from_array(fit.camera_position),
        Vec3::from_array(fit.camera_target),
    )
}


/// Build the Cornell box into `world`, uploading all meshes + BLAS through
/// `ctx`. Returns `(camera_position, camera_target)` so the caller can place
/// the fly-camera looking into the open front of the box.
///
/// `sun` selects the exterior variant (`--cornell-sun`): see the module
/// header for what changes and why (#1942).
pub(crate) fn setup_cornell_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    sun: bool,
) -> (Vec3, Vec3) {
    install_cornell_lighting(world, sun);
    let combustion_probe = std::env::var_os("BYRO_COMBUSTION_PROBE").is_some();

    // Every probe is untextured by design — surface color comes entirely
    // from `Material::diffuse_color`. Bind the registry's white 1×1
    // neutral fallback (handle 1) so the shader's `albedo *= texColor`
    // multiply yields the authored color. Without an explicit
    // `TextureHandle` the draw loop would default to handle 0 — the
    // magenta/checker "missing texture" diagnostic — and every surface
    // would render as a tinted checkerboard. (See `asset_provider`'s F2
    // path: the NIF / cell loaders route textureless materials here too.)
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());

    let mut builder = MeshBuilder::new(ctx);

    // ── Room shell ──────────────────────────────────────────────────
    // Walls are thin slabs; from inside the room the inner face is
    // front-facing (normal points into the room) so back-face culling
    // keeps the outer faces hidden. Color is driven by Material, so the
    // slab geometry is flat-white.
    let cy = HEIGHT * 0.5;
    let h_slab = builder.box_mesh([HALF_W, T, HALF_W]); // floor / ceiling
    let back_slab = builder.box_mesh([HALF_W, cy, T]);
    let side_slab = builder.box_mesh([T, cy, HALF_W]); // left / right

    let walls: &[(MeshHandle, Vec3, [f32; 3], &str)] = &[
        (h_slab, Vec3::new(0.0, -T, 0.0), WHITE, "floor"),
        // Skipped in sun mode — a lid would block every sun ray and
        // reduce the bisection scene to ambient.
        (h_slab, Vec3::new(0.0, HEIGHT + T, 0.0), WHITE, "ceiling"),
        (
            back_slab,
            Vec3::new(0.0, cy, -HALF_W - T),
            WHITE,
            "back_wall",
        ),
        (
            side_slab,
            Vec3::new(-HALF_W - T, cy, 0.0),
            RED,
            "left_wall_red",
        ),
        (
            side_slab,
            Vec3::new(HALF_W + T, cy, 0.0),
            GREEN,
            "right_wall_green",
        ),
    ];
    for &(mesh, pos, color, name) in walls {
        if sun && name == "ceiling" {
            continue;
        }
        spawn_object(
            world,
            mesh,
            neutral,
            pos,
            Quat::IDENTITY,
            matte(color),
            name,
        );
    }

    // ── Local fog volume probe ──────────────────────────────────────
    // #2248 (REN-D21-01) — unlike the global `CellLightingRes::fog_medium`
    // ramp below (deliberately pushed out of range to match a real
    // no-authored-fog interior cell, #1942's sibling trap for the sun
    // path), a local `FogVolume` is an explicitly-placed authored effect
    // — a designer-placed smoke/mist pocket, independent of whether the
    // cell has ambient atmospheric fog at all. Spawned in both variants:
    // local fog isn't sun-driven, so it belongs outside the `!sun` gate.
    // Extinction is authored directly in "per meter" terms and converted
    // to per-world-unit by the same `WORLD_UNITS_PER_METER` divide the
    // real import path uses (`render/fog_volumes.rs`), so a value that
    // reads as "thick smoke" at Bethesda scale also reads as thick smoke
    // here — the box's few-world-unit span is what makes it visible in a
    // handful of units instead of dozens of metres.
    spawn_fog_volume(
        world,
        Vec3::new(-1.6, 1.6, -0.4),
        Vec3::new(1.3, 1.3, 1.3),
        "fog_volume_probe",
    );
    if combustion_probe {
        spawn_combustion_probe(
            world,
            CombustionProbeSpec {
                kind: CombustionProbeKind::Explosion {
                    start_delay_seconds: 2.0,
                    lifetime_seconds: 8.0,
                },
                position: Vec3::new(0.0, 1.35, -0.4),
                half_extents: Vec3::splat(1.55),
                name: "combustion_explosion_probe",
            },
        );
    }

    // ── Local lights (interior variant only) ────────────────────────
    // In sun mode every one of these is skipped: a bisection harness for
    // the directional path must not have a second light source that can
    // mask a dead or misdirected sun. What survives is the emissive cube
    // probe below, which is emissive-GI, not a `LightSource`.
    if !sun {
        // ── Ceiling area light ──────────────────────────────────────
        // An emissive panel (the visible light) plus a point LightSource
        // just below it (the actual direct illumination — emissive-only
        // GI is a known weak spot this harness is meant to expose).
        let light_panel = builder.box_mesh([1.2, 0.02, 1.2]);
        spawn_object(
            world,
            light_panel,
            neutral,
            Vec3::new(0.0, HEIGHT - 0.03, 0.0),
            Quat::IDENTITY,
            emissive([1.0, 0.97, 0.9], 8.0),
            "ceiling_light_panel",
        );
        spawn_point_light(
            world,
            Vec3::new(0.0, HEIGHT - 0.3, 0.0),
            30.0,
            [1.6, 1.55, 1.45],
            "ceiling_light",
        );

        // ── Camera-side key/fill light ──────────────────────────────
        // The ceiling light alone sits *behind* the front probe rows, so
        // their camera-facing hemispheres fall into near-shadow and no
        // material differences are visible. This second light, placed
        // high and off to one side near the camera, rakes the
        // camera-facing sides — giving each probe a GGX highlight whose
        // shape/size reveals roughness, and an albedo-tinted (vs white)
        // specular that reveals metalness. Dimmer than the key so the
        // Cornell colour-bleed look survives. (Whether GI *alone* should
        // fill these faces is a separate question tracked for a later
        // pass.)
        spawn_point_light(
            world,
            Vec3::new(2.0, HEIGHT * 0.8, HALF_W + 1.0),
            40.0,
            [1.1, 1.1, 1.15],
            "camera_fill_light",
        );
    }

    // ── Classic probes: tall matte block + matte sphere ─────────────
    let tall = builder.box_mesh([0.7, 1.5, 0.7]);
    spawn_object(
        world,
        tall,
        neutral,
        Vec3::new(-1.5, 1.5, -1.6),
        Quat::from_rotation_y(-0.3),
        matte(WHITE),
        "tall_block",
    );
    let big_sphere = builder.sphere(0.9);
    spawn_object(
        world,
        big_sphere,
        neutral,
        Vec3::new(1.6, 0.9, -1.2),
        Quat::IDENTITY,
        matte(WHITE),
        "matte_sphere",
    );

    // ── Material sweeps ─────────────────────────────────────────────
    // Two front rows of small spheres. Row near z=+1.5 sweeps roughness
    // at metalness=1.0 (GGX lobe + RT reflection across the gate); row at
    // z=+2.9 sweeps metalness at a fixed *moderate* roughness.
    //
    // The metalness row's roughness is deliberately 0.35, not mirror-
    // smooth: at low roughness both ends of a metalness sweep are
    // dominated by a sharp environment reflection, so dielectric (m=0)
    // and metal (m=1) look near-identical in a dim room — verified live
    // via `mat.set`. At 0.35 the dielectric end shows its diffuse albedo
    // while the metal end shows an albedo-tinted glossy reflection, so
    // the transition actually reads. Sweep either row at runtime with
    // `mat.set <id> roughness <v>` to probe other points.
    let probe = builder.sphere(0.45);
    let xs = [-3.0_f32, -1.5, 0.0, 1.5, 3.0];
    for (i, &x) in xs.iter().enumerate() {
        let r = 0.02 + 0.96 * (i as f32 / (xs.len() - 1) as f32);
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 1.5),
            Quat::IDENTITY,
            pbr([0.95, 0.95, 0.95], 1.0, r),
            &format!("metal_rough_{i}"),
        );
    }
    for (i, &x) in xs.iter().enumerate() {
        let m = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 2.9),
            Quat::IDENTITY,
            pbr([0.9, 0.85, 0.55], m, 0.35),
            &format!("metalness_{i}"),
        );
    }
    // #2477 / REN-D21-2026-08-07-01 — same metalness sweep, one row
    // further back, but with `MAT_FLAG_PBR_BSDF` set so it renders
    // through `disneyDiffuseSplit` instead of legacy Lambert. Side by
    // side with `metalness_*` above, the two rows should read as
    // subtly different (Burley vs. Lambert diffuse falloff) rather
    // than identical — identical would mean the Disney branch is
    // silently not engaging. `mat.set <id> material_flags <bits>`
    // clears/re-sets the bit live for direct comparison.
    for (i, &x) in xs.iter().enumerate() {
        let m = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 4.3),
            Quat::IDENTITY,
            pbr_bsdf([0.9, 0.85, 0.55], m, 0.35),
            &format!("metalness_bsdf_{i}"),
        );
    }
    // #2514 / REN-D21-2026-08-07-02 — one row further back, sweeping
    // subsurface/sheen/sheen_tint/anisotropic together from 0 → 1 at a
    // fixed moderate metalness/roughness (same 0.35 as the row above, for
    // the same "diffuse end doesn't get swamped by a sharp reflection"
    // reason). Before this probe, no entity the harness could produce
    // ever drove these four scalars off `Material::default()`'s zero —
    // `disneyDiffuseSplit` always degenerated back to Burley-only even
    // with `MAT_FLAG_PBR_BSDF` set. Sweep with `mat.set <id> subsurface
    // <v>` / `sheen <v>` / `sheen_tint <v>` / `anisotropic <v>` to isolate
    // one lobe at a time.
    for (i, &x) in xs.iter().enumerate() {
        let t = i as f32 / (xs.len() - 1) as f32;
        spawn_object(
            world,
            probe,
            neutral,
            Vec3::new(x, 0.45, 5.7),
            Quat::IDENTITY,
            pbr_bsdf_lobes([0.9, 0.85, 0.55], 0.5, 0.35, t, t, t, t),
            &format!("bsdf_lobes_{i}"),
        );
    }

    // ── Glass probes ────────────────────────────────────────────────
    // Glass is OPAQUE (no AlphaBlend): the IOR refraction ray IS the
    // transmission — it samples the
    // scene behind and writes it in place of the background, so the bent /
    // refracted world is what you see THROUGH the glass. An alpha-blend
    // window would instead composite the *undistorted* background over the
    // glass and dilute the refraction to invisibility. The old budget /
    // jitter stipple that motivated alpha-blend is fixed (IOR budget,
    // smooth-glass deterministic refraction, deterministic metal refl).
    // Front-centre hero so its wide IOR refraction captures the colourful
    // room behind it (red/green walls, ceiling light, floor) and shows the
    // inverted/magnified scene — the classic glass-ball refraction demo.
    // Against the flat white back wall (where it sat before) the bend is
    // invisible; here the two-surface refraction reads clearly.
    let glass_sphere = builder.sphere(0.95);
    spawn_object(
        world,
        glass_sphere,
        neutral,
        Vec3::new(0.0, 1.05, 2.4),
        Quat::IDENTITY,
        glass([0.9, 0.95, 1.0]),
        "glass_sphere",
    );
    let glass_cube = builder.box_mesh([0.6, 0.6, 0.6]);
    spawn_object(
        world,
        glass_cube,
        neutral,
        Vec3::new(-2.6, 0.6, 0.6),
        Quat::from_rotation_y(0.4),
        glass([1.0, 0.95, 0.9]),
        "glass_cube",
    );

    // ── Fire-refraction probe ────────────────────────────────────────
    // #2249 (REN-D21-03) — `MATERIAL_KIND_FIRE_REFRACTION` had no Cornell
    // coverage: `mat.set` couldn't reach `ior` (the field's distortion-
    // strength overload — now fixed in `commands/scene.rs`) and no probe
    // carried a normal map, so `tangentWarp = N - macroN * dot(N, macroN)`
    // was structurally zero even at max authored strength. This probe
    // exercises both halves together.
    let fire_normal_map = synthesize_wavy_normal_map(builder.ctx);
    let fire_cube = builder.box_mesh([0.6, 0.9, 0.6]);
    let fire_entity = spawn_object(
        world,
        fire_cube,
        neutral,
        Vec3::new(2.6, 0.9, 0.6),
        Quat::from_rotation_y(-0.4),
        fire_refraction(0.6),
        "fire_refraction_probe",
    );
    world.insert(
        fire_entity,
        MaterialTextureHandles {
            textures: byroredux_nif::import::MaterialTextureSet {
                normal: fire_normal_map,
                ..Default::default()
            },
            normal_has_alpha: false,
            // #4423 — synthetic paths bind no tint texture; see the field doc.
            tint_has_alpha: false,
            parallax_height_scale:
                byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE,
            parallax_max_passes:
                byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES,
        },
    );

    // ── Emissive probe ──────────────────────────────────────────────
    let emit_cube = builder.box_mesh([0.35, 0.35, 0.35]);
    spawn_object(
        world,
        emit_cube,
        neutral,
        Vec3::new(0.2, 0.35, 0.4),
        Quat::from_rotation_y(0.6),
        emissive([1.0, 0.4, 0.1], 4.0),
        "emissive_cube",
    );

    builder.finish();

    log::info!(
        "Cornell box ready ({}): {} entities. Tweak materials live via `mat.list` / \
         `mat.set <id> <field> <value>` over byro-dbg.",
        if sun {
            "exterior / sun-only"
        } else {
            "interior / point-light"
        },
        world.next_entity_id()
    );

    // Camera: stand outside the open front, slightly above mid-height,
    // looking at the room center.
    let target = Vec3::new(0.0, HEIGHT * 0.45, 0.0);
    let pos = Vec3::new(0.0, HEIGHT * 0.55, HALF_W + 6.0);
    (pos, target)
}

/// Install the environment resources for the selected variant.
///
/// Split out of [`setup_cornell_scene`] because it is the whole point of
/// #1942 and the only part testable without a Vulkan device: it decides
/// whether the renderer's sun paths are driven at all.
///
/// Interior (`sun == false`) keeps the classic look — near-black ambient
/// so the ceiling panel dominates, directional zeroed, fog pushed out of
/// range, and *no* `SkyParamsRes` (so `build_sky_params` returns the
/// all-default `SkyParams` and the composite pass skips the sky).
///
/// Exterior (`sun == true`) reuses the canonical plugin-less exterior
/// constructors rather than hand-rolling a second set of literals, so the
/// harness drifts with the real fallback path instead of away from it.
pub(crate) fn install_cornell_lighting(world: &mut World, sun: bool) {
    if sun {
        world.insert_resource(procedural_fallback_cell_lighting(sun_dir()));
        world.insert_resource(procedural_fallback_sky(sun_dir()));
        return;
    }
    world.insert_resource(CellLightingRes {
        ambient: [0.03, 0.03, 0.03],
        directional_color: [0.0, 0.0, 0.0],
        directional_dir: [0.0, -1.0, 0.0],
        is_interior: true,
        fog_color: [0.0, 0.0, 0.0],
        fog_near: 100_000.0,
        fog_far: 1_000_000.0,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None),
        directional_fade: None,
        fog_clip: None,
        fog_power: None,
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
        inheritance_flags: None,
    });
}

#[cfg(test)]
mod tests {
    use super::combustion_lab::{LAB_UNITS_PER_METER, combustion_lab_manifest};
    use super::glass_dragon::{
        DRAGON_FLOOR_LIFT, DRAGON_GLASS_ROUGHNESS, DRAGON_PRESENTATION_YAW,
        force_glass_dragon_material, place_glass_dragon,
    };
    use super::oracle::{CornellOracleRung, cornell_oracle_manifest};
    use super::*;
    use crate::scene::cornell_sun_mode;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn godray_lab_open_and_sealed_modes_are_explicit() {
        assert_eq!(godray_lab_mode(&args(&[])), None);
        assert_eq!(godray_lab_mode(&args(&["--godray-lab-extra"])), None);
        assert_eq!(godray_lab_mode(&args(&["--godray-lab"])), Some(false));
        assert_eq!(godray_lab_mode(&args(&["--godray-lab-sealed"])), Some(true));
    }

    #[test]
    fn combustion_lab_flag_is_distinct_and_exact() {
        assert!(!combustion_lab_mode(&args(&[])));
        assert!(!combustion_lab_mode(&args(&["--combustion-lab-extra"])));
        assert!(combustion_lab_mode(&args(&["--combustion-lab"])));
        assert!(combustion_lab_mode(&args(&["--combustion-lab-nuclear"])));
        assert!(combustion_lab_nuclear_mode(&args(&[
            "--combustion-lab-nuclear"
        ])));
        assert!(!combustion_lab_nuclear_mode(&args(&["--combustion-lab"])));
    }

    #[test]
    fn combustion_lab_manifest_is_meter_scaled_and_hits_the_baffle() {
        let manifest = combustion_lab_manifest();
        assert_eq!(manifest.room_half_width / LAB_UNITS_PER_METER, 3.0);
        assert_eq!(manifest.room_half_depth / LAB_UNITS_PER_METER, 4.0);
        assert_eq!(manifest.room_height / LAB_UNITS_PER_METER, 4.0);
        assert_eq!(manifest.explosion_radius / LAB_UNITS_PER_METER, 0.45);
        assert!(manifest.explosion_radius > 20.0);
        assert!(manifest.explosion_start_delay_seconds >= 3.0);

        let baffle_bottom = manifest.baffle_center.y - manifest.baffle_half_extents.y;
        let flame_top = manifest.flame_position.y + manifest.flame_half_extents.y;
        let explosion_top = manifest.explosion_position.y + manifest.explosion_radius;
        assert!(baffle_bottom > flame_top);
        assert!(baffle_bottom > explosion_top);
        assert!(baffle_bottom < manifest.room_height);
        assert!(manifest.baffle_half_extents.x > manifest.explosion_position.x.abs());
        assert!(manifest.baffle_half_extents.x > manifest.flame_position.x.abs());
        assert!(manifest.baffle_half_extents.z > manifest.explosion_radius);
        assert!(manifest.camera_position.z > manifest.room_half_depth);
    }

    #[test]
    fn cornell_oracle_cli_names_only_complete_rungs() {
        assert_eq!(cornell_oracle_rung(&args(&[])).unwrap(), None);
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "l0"])).unwrap(),
            Some(CornellOracleRung::L0)
        );
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "L2"])).unwrap(),
            Some(CornellOracleRung::L2)
        );
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "l4"])).unwrap(),
            Some(CornellOracleRung::L4)
        );
        assert_eq!(
            cornell_oracle_rung(&args(&["--cornell-oracle", "L5"])).unwrap(),
            Some(CornellOracleRung::L5)
        );
        for (name, rung) in [
            ("l1-skinned", CornellOracleRung::L1Skinned),
            ("l1-skinned-shared", CornellOracleRung::L1SkinnedShared),
            ("l2-skinned", CornellOracleRung::L2Skinned),
            ("l1-point", CornellOracleRung::L1Point),
            ("l1-skinned-point", CornellOracleRung::L1SkinnedPoint),
            ("l1-spot", CornellOracleRung::L1Spot),
            ("l1-skinned-spot", CornellOracleRung::L1SkinnedSpot),
            ("l2-skinned-spot", CornellOracleRung::L2SkinnedSpot),
            ("l2-cache-pressure", CornellOracleRung::L2CachePressure),
            (
                "l2-cache-pressure-large",
                CornellOracleRung::L2MixedCachePressure,
            ),
            ("l3-mirrored", CornellOracleRung::L3Mirrored),
            ("l4-mirrored", CornellOracleRung::L4Mirrored),
        ] {
            assert_eq!(
                cornell_oracle_rung(&args(&["--cornell-oracle", name])).unwrap(),
                Some(rung)
            );
        }
        assert!(cornell_oracle_rung(&args(&["--cornell-oracle"])).is_err());
        assert!(cornell_oracle_rung(&args(&["--cornell-oracle", "l6"])).is_err());
    }

    #[test]
    fn cache_pressure_oracle_keeps_the_l2_visible_scene_unchanged() {
        let reference = cornell_oracle_manifest(CornellOracleRung::L2);
        let pressure = cornell_oracle_manifest(CornellOracleRung::L2CachePressure);
        assert_eq!(pressure.blocker, reference.blocker);
        assert_eq!(
            pressure.directional_radiance,
            reference.directional_radiance
        );
        assert_eq!(
            pressure.direction_toward_source,
            reference.direction_toward_source
        );
        assert_eq!(pressure.camera_position, reference.camera_position);
        assert_eq!(pressure.camera_target, reference.camera_target);
        assert_eq!(pressure.primary_debug_view, reference.primary_debug_view);
        assert!(!pressure.volumetric_probe && !pressure.material_probes);
    }

    #[test]
    fn mirrored_fog_oracle_keeps_the_medium_partition_and_camera() {
        for (reference, mirrored) in [
            (CornellOracleRung::L3, CornellOracleRung::L3Mirrored),
            (CornellOracleRung::L4, CornellOracleRung::L4Mirrored),
        ] {
            let reference = cornell_oracle_manifest(reference);
            let mirrored = cornell_oracle_manifest(mirrored);
            assert_eq!(mirrored.blocker, reference.blocker);
            assert_eq!(
                mirrored.directional_radiance,
                reference.directional_radiance
            );
            assert_eq!(mirrored.camera_position, reference.camera_position);
            assert_eq!(mirrored.camera_target, reference.camera_target);
            assert_eq!(mirrored.primary_debug_view, reference.primary_debug_view);
            assert!(mirrored.volumetric_probe && !mirrored.material_probes);
        }
    }

    #[test]
    fn cornell_oracle_l5_adds_only_the_canonical_material_probe_row() {
        let l2 = cornell_oracle_manifest(CornellOracleRung::L2);
        let l5 = cornell_oracle_manifest(CornellOracleRung::L5);

        assert!(l5.material_probes);
        assert!(!l5.blocker);
        assert!(!l5.volumetric_probe);
        assert_eq!(l5.directional_radiance, [1.0; 3]);
        assert_eq!(l5.primary_debug_view, "material_lobe");
        assert_eq!(l5.camera_position, l2.camera_position);
        assert_eq!(l5.camera_target, l2.camera_target);
        assert_eq!(l5.direction_toward_source, l2.direction_toward_source);
    }

    #[test]
    fn cornell_oracle_world_offset_is_explicit_finite_and_three_dimensional() {
        assert_eq!(cornell_oracle_world_offset(&args(&[])).unwrap(), Vec3::ZERO);
        assert_eq!(
            cornell_oracle_world_offset(&args(&[
                "--cornell-oracle-world-offset",
                "1000000,0,-1000000",
            ]))
            .unwrap(),
            Vec3::new(1_000_000.0, 0.0, -1_000_000.0)
        );
        for invalid in ["1,2", "1,2,3,4", "1,NaN,3", "far,0,0"] {
            assert!(
                cornell_oracle_world_offset(&args(&["--cornell-oracle-world-offset", invalid,]))
                    .is_err()
            );
        }
        assert!(cornell_oracle_world_offset(&args(&["--cornell-oracle-world-offset"])).is_err());
    }

    #[test]
    fn glass_dragon_flag_is_a_distinct_exact_scene_mode() {
        assert!(!glass_dragon_mode(&args(&[])));
        assert!(!glass_dragon_mode(&args(&["--cornell"])));
        assert!(!glass_dragon_mode(&args(&["--cornell-sun"])));
        assert!(!glass_dragon_mode(&args(&["--cornell-glass-dragon-extra"])));
        assert!(glass_dragon_mode(&args(&[
            "--game",
            "skyrim_se",
            "--cornell-glass-dragon",
        ])));
    }

    #[test]
    fn glass_dragon_placement_lifts_and_presents_the_authored_static_pose() {
        let authored_rotation = Quat::from_rotation_x(0.2);
        let mut transform = Transform::new(Vec3::new(5.0, -9.0, 7.0), authored_rotation, 1.0);

        place_glass_dragon(&mut transform);

        assert_eq!(
            transform.translation,
            Vec3::new(5.0, -9.0 + DRAGON_FLOOR_LIFT, 7.0)
        );
        let expected = Quat::from_rotation_y(DRAGON_PRESENTATION_YAW) * authored_rotation;
        assert!(transform.rotation.dot(expected).abs() > 0.999_999);
    }

    #[test]
    fn glass_dragon_override_reaches_canonical_refractive_glass() {
        let mut pool = StringPool::new();
        let base = pool.intern("textures/actors/dragon/dragon.dds");
        let normal = pool.intern("textures/actors/dragon/dragon_n.dds");
        let mut imported = ImportedMaterial {
            has_alpha: true,
            alpha_test: true,
            is_decal: true,
            is_pbr: true,
            has_translucency: true,
            model_space_normals: true,
            material_kind: byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER,
            emissive_color: [1.0, 0.2, 0.1],
            emissive_mult: 8.0,
            ..Default::default()
        };
        imported.textures.base_color = Some(base);
        imported.textures.normal = Some(normal);

        force_glass_dragon_material(&mut imported);

        assert_eq!(imported.material_kind, MATERIAL_KIND_GLASS);
        assert!(
            imported.has_alpha,
            "refractive glass needs the blend pipeline for fallback coverage and caustic source identity"
        );
        assert_eq!(imported.src_blend_mode, 6);
        assert_eq!(imported.dst_blend_mode, 7);
        assert!(!imported.alpha_test);
        assert!(!imported.is_decal);
        assert_eq!(imported.textures.base_color, None);
        assert_eq!(imported.textures.normal, Some(normal));
        assert!(imported.model_space_normals);
        assert_eq!(imported.emissive_mult, 0.0);
        assert_eq!(imported.metalness_override, Some(0.0));
        assert_eq!(imported.roughness_override, Some(DRAGON_GLASS_ROUGHNESS));
        assert!(imported.bgsm_pbr_scalars_authored);

        let translated = crate::material_translate::translate_material(
            &imported,
            Some("Dragon:0"),
            crate::material_translate::ResolvedPaths {
                textures: Default::default(),
                material_path: None,
                source_base_color: None,
            },
            0,
        );
        assert_eq!(translated.material_kind, MATERIAL_KIND_GLASS);
        assert_eq!(translated.metalness, 0.0);
        assert_eq!(translated.roughness, DRAGON_GLASS_ROUGHNESS);
        assert!(
            translated.ior > 1.0,
            "glass must reach the refractive IOR path"
        );
    }

    #[test]
    fn cornell_oracle_l0_l2_add_exactly_light_then_blocker() {
        let l0 = cornell_oracle_manifest(CornellOracleRung::L0);
        let l1 = cornell_oracle_manifest(CornellOracleRung::L1);
        let l2 = cornell_oracle_manifest(CornellOracleRung::L2);

        assert_eq!(l0.directional_radiance, [0.0; 3]);
        assert!(!l0.blocker);
        assert_eq!(l1.directional_radiance, [1.0; 3]);
        assert!(!l1.blocker);
        assert_eq!(l2.directional_radiance, l1.directional_radiance);
        assert_eq!(l2.direction_toward_source, l1.direction_toward_source);
        assert!(l2.blocker);
        assert_eq!(l2.primary_debug_view, "shadow_visibility");
    }

    #[test]
    fn cornell_oracle_l3_l4_add_exactly_the_opaque_partition() {
        let l3 = cornell_oracle_manifest(CornellOracleRung::L3);
        let l4 = cornell_oracle_manifest(CornellOracleRung::L4);

        assert!(l3.volumetric_probe);
        assert!(l4.volumetric_probe);
        assert_eq!(l3.directional_radiance, [0.0; 3]);
        assert_eq!(l4.directional_radiance, l3.directional_radiance);
        assert!(!l3.blocker);
        assert!(l4.blocker);
        assert_eq!(l3.camera_position, l4.camera_position);
        assert_eq!(l3.camera_target, l4.camera_target);
        assert_eq!(l3.primary_debug_view, "composite_term");
        assert_eq!(l4.primary_debug_view, "composite_term");
    }

    #[test]
    fn cornell_oracle_lambert_expectation_is_analytic() {
        let l0 = cornell_oracle_manifest(CornellOracleRung::L0);
        let l1 = cornell_oracle_manifest(CornellOracleRung::L1);
        assert_eq!(l0.expected_unshadowed_direct([1.0; 3]), [0.0; 3]);

        let direction = Vec3::from_array(l1.direction_toward_source);
        assert!((direction.length() - 1.0).abs() < 1e-6);
        let expected = l1.expected_unshadowed_direct([1.0; 3]);
        for channel in expected {
            assert!((channel - direction.z).abs() < 1e-6);
        }
    }

    #[test]
    fn cornell_oracle_l2_probe_ray_crosses_the_declared_blocker() {
        let l2 = cornell_oracle_manifest(CornellOracleRung::L2);
        let direction = Vec3::from_array(l2.direction_toward_source);
        let at_mid_depth = 0.75 / direction.z;

        // The blocker is [-0.75, 0.75] in X, [3.25, 4.75] in Y, and
        // [0, 1.5] in Z. This receiver point reaches its centre at mid-depth
        // when traced toward the source, while the control remains outside.
        let shadow_probe = Vec3::new(-0.375, 3.625, 0.0);
        let inside = shadow_probe + direction * at_mid_depth;
        assert!(inside.x.abs() < 0.75 && (inside.y - 4.0).abs() < 0.75);

        let unshadowed_probe = Vec3::new(2.5, 6.5, 0.0);
        let outside = unshadowed_probe + direction * at_mid_depth;
        assert!(outside.x.abs() > 0.75 && (outside.y - 4.0).abs() > 0.75);
    }

    /// #1942 — `--cornell-sun` selects the exterior variant, plain
    /// `--cornell` the interior one, and neither flag falls through to
    /// the ESM / NIF / demo paths. Both flags together resolve to sun
    /// mode rather than to whichever the parser happened to test first.
    #[test]
    fn cornell_flag_selects_variant() {
        assert_eq!(cornell_sun_mode(&args(&[])), None);
        assert_eq!(cornell_sun_mode(&args(&["--esm", "Skyrim.esm"])), None);
        assert_eq!(cornell_sun_mode(&args(&["--cornell"])), Some(false));
        assert_eq!(cornell_sun_mode(&args(&["--cornell-sun"])), Some(true));
        assert_eq!(
            cornell_sun_mode(&args(&["--cornell", "--cornell-sun"])),
            Some(true),
            "asking for the sun variant at all means the sun paths are what's being bisected"
        );
        assert_eq!(
            cornell_sun_mode(&args(&["--cornellsun"])),
            None,
            "no prefix matching — an unknown flag must not silently enable the harness"
        );
    }

    /// The interior variant is what #1942 reported: the sun paths are
    /// inert because the directional term is zeroed and no
    /// `SkyParamsRes` exists, so `build_sky_params` hands the renderer
    /// the all-default `SkyParams` (`is_exterior = false`). Pinned so a
    /// future "just give Cornell a sun" edit can't quietly change the
    /// interior reference scene instead of using the new variant.
    #[test]
    fn interior_variant_leaves_the_sun_paths_inert() {
        let mut world = World::new();
        install_cornell_lighting(&mut world, false);

        let lit = world.resource::<CellLightingRes>();
        assert!(lit.is_interior);
        assert_eq!(lit.directional_color, [0.0, 0.0, 0.0]);
        drop(lit);
        assert!(
            world
                .try_resource::<crate::components::SkyParamsRes>()
                .is_none(),
            "no SkyParamsRes → no sky, no volumetric sun injection, no Effect_Lit sun"
        );
    }

    /// The exterior variant drives every sun path: a non-zero
    /// directional colour on an `is_interior = false` cell (so
    /// `compute_directional_upload` scales by the full
    /// `sun_intensity / SUN_INTENSITY_PEAK` instead of the 0.6 interior
    /// constant) plus an `is_exterior` `SkyParamsRes` carrying the same
    /// direction. Both resources must agree — `directional_dir` and
    /// `sun_direction` are separately consumed (`render::lights` vs
    /// `render::sky`), and a harness where they disagree would itself
    /// be a sun-direction bug. #1942.
    #[test]
    fn sun_variant_drives_directional_and_sky_paths() {
        use crate::components::SkyParamsRes;

        let mut world = World::new();
        install_cornell_lighting(&mut world, true);

        let expected = sun_dir();
        let len =
            (expected[0] * expected[0] + expected[1] * expected[1] + expected[2] * expected[2])
                .sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "sun direction must be unit-length, got {len}"
        );
        assert!(
            expected[1] > 0.0,
            "engine convention: the vector points TOWARD the sun, so +Y while the sun is up"
        );

        let lit = world.resource::<CellLightingRes>();
        assert!(!lit.is_interior, "exterior → full sun-intensity scaling");
        assert_ne!(lit.directional_color, [0.0, 0.0, 0.0]);
        assert_eq!(lit.directional_dir, expected);
        drop(lit);

        let sky = world.resource::<SkyParamsRes>();
        assert!(
            sky.is_exterior,
            "gates the composite sky + froxel sun inject"
        );
        assert_eq!(
            sky.sun_direction, expected,
            "SkyParamsRes and CellLightingRes must carry the same direction"
        );
        assert!(sky.sun_intensity > 0.0);
        drop(sky);

        // #3561 — and the invariant has to survive the live path, not just
        // install time. `weather_system` is registered unconditionally
        // (`Stage::Early`, exclusive), and `setup_scene` calls
        // `ensure_game_time` for EVERY scene kind before any `--cornell`
        // branch, so its `GameTimeRes` guard passes here. With no
        // `WeatherDataRes` installed it reached
        // `apply_neutral_exterior_fallback`, which rebuilt the whole
        // `CellLightingRes` from a hardcoded hour-6 sun and then `return`ed
        // BEFORE the `SkyParamsRes` write block — leaving the shading
        // directional and the painted sun disc ~48 degrees apart from frame 1
        // onward. This assertion, not the ones above, is what that bug
        // failed; the install-time pin never ran the system.
        // `setup_scene` calls `world_setup::ensure_game_time` for every scene
        // kind, so the clock is present on the real `--cornell-sun` path;
        // mirror that here (13:00, mid-sky, unlike the hardcoded hour 6 the
        // fallback used to install).
        world.insert_resource(crate::components::GameTimeRes::new(13.0, 0.0));
        crate::systems::weather::weather_system(&world, 0.016);

        let lit = world.resource::<CellLightingRes>();
        assert_eq!(
            lit.directional_dir, expected,
            "the first scheduler tick must not overwrite the harness's authored sun"
        );
        drop(lit);
        let sky = world.resource::<SkyParamsRes>();
        assert_eq!(
            sky.sun_direction, expected,
            "SkyParamsRes and CellLightingRes must still agree after weather_system"
        );
    }

    /// Regression for #2248 (REN-D21-01): `--cornell` must carry a local
    /// `FogVolume` probe that produces genuinely measurable optical depth
    /// at the box's own world-unit scale, not the near-zero the global
    /// fog ramp rounds to over the box's few-unit span (`fog.rs`'s
    /// `fit_legacy_fog_extinction(100_000.0, 1_000_000.0, ...)` is fit for
    /// Bethesda-cell distances, not a ~4-8-unit room).
    #[test]
    fn fog_volume_probe_is_renderable_and_visible_at_cornell_scale() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        spawn_fog_volume(
            &mut world,
            Vec3::new(-1.6, 1.6, -0.4),
            Vec3::new(1.3, 1.3, 1.3),
            "fog_volume_probe",
        );

        let volumes = world.query::<FogVolume>().expect("FogVolume storage");
        let transforms = world
            .query::<GlobalTransform>()
            .expect("GlobalTransform storage");
        let (entity, volume) = volumes
            .iter()
            .next()
            .expect("spawn_fog_volume must insert exactly one FogVolume");
        assert!(
            transforms.get(entity).is_some(),
            "a FogVolume needs a GlobalTransform for the collection query in render/fog_volumes.rs"
        );
        assert!(
            volume.is_renderable(),
            "probe must satisfy FogVolume::is_renderable (bounds set, finite positive extinction)"
        );

        // Mirror the CPU→GPU conversion in `render/fog_volumes.rs`
        // (`extinction_per_meter / WORLD_UNITS_PER_METER`) to check the
        // resulting per-world-unit density actually produces visible
        // attenuation across the volume's own extent, instead of the ~0
        // the Bethesda-cell-scale global ramp rounds to here.
        let bounds = volume.bounds.expect("checked by is_renderable above");
        let sigma_t_per_world_unit =
            volume.extinction_per_meter / crate::fog::WORLD_UNITS_PER_METER;
        let path_length = 2.0 * bounds.half_extents.min_element();
        let optical_depth = sigma_t_per_world_unit * path_length;
        assert!(
            optical_depth > 0.5,
            "optical depth {optical_depth} across the probe must be clearly visible \
             (>0.5), not rounding to ~0 like the global fog ramp does at this scale"
        );
    }

    /// Regression for #2249 (REN-D21-03): the Cornell fire-refraction
    /// probe's normal map must actually vary spatially. A flat/neutral
    /// normal map would still compile and shade, but `N == macroN` at
    /// every fragment makes `tangentWarp = N - macroN * dot(N, macroN)`
    /// structurally zero regardless of authored distortion strength — the
    /// same silent no-op the missing `mat.set ior` case left uncaught.
    #[test]
    fn wavy_normal_map_pixels_vary_and_stay_opaque() {
        let pixels = wavy_normal_map_pixels(16);
        assert_eq!(pixels.len(), 16 * 16 * 4);

        let first_rgb = [pixels[0], pixels[1], pixels[2]];
        let varies = pixels
            .chunks_exact(4)
            .any(|p| [p[0], p[1], p[2]] != first_rgb);
        assert!(
            varies,
            "normal map must vary spatially, not be uniformly flat/neutral"
        );
        assert!(
            pixels.chunks_exact(4).all(|p| p[3] == 255),
            "normal map must be fully opaque"
        );
    }

    /// The fire-refraction probe's `Material` must carry the material
    /// kind + a non-zero authored distortion strength (`ior`) — the
    /// half of #2249 that doesn't need a normal map to check.
    #[test]
    fn fire_refraction_material_carries_kind_and_distortion_strength() {
        let material = fire_refraction(0.6);
        assert_eq!(material.material_kind, MATERIAL_KIND_FIRE_REFRACTION);
        assert_eq!(material.ior, 0.6);
    }

    /// Regression for #2477 (REN-D21-2026-08-07-01): every OTHER Cornell
    /// material constructor leaves `effect_shader_flags` at
    /// `Material::default()`'s `0`, so `MAT_FLAG_PBR_BSDF` stays clear and
    /// the shared direct-lighting BRDF takes the legacy Lambert branch —
    /// never the Disney (`disneyDiffuseSplit`) branch every real
    /// BGSM/BGEM-sourced surface takes. `pbr_bsdf` must set the bit so at
    /// least one probe row exercises that branch.
    #[test]
    fn pbr_bsdf_material_sets_the_disney_bsdf_flag() {
        use byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;

        // Every other constructor: flag clear (the pre-fix, still-correct
        // state for the legacy-Lambert probes).
        assert_eq!(matte(WHITE).effect_shader_flags & PBR_BSDF, 0);
        assert_eq!(
            pbr([0.9, 0.85, 0.55], 0.5, 0.35).effect_shader_flags & PBR_BSDF,
            0
        );

        // The Disney sibling must set it, and must otherwise match `pbr`'s
        // metalness/roughness/color plumbing exactly.
        let plain = pbr([0.9, 0.85, 0.55], 0.5, 0.35);
        let bsdf = pbr_bsdf([0.9, 0.85, 0.55], 0.5, 0.35);
        assert_ne!(
            bsdf.effect_shader_flags & PBR_BSDF,
            0,
            "pbr_bsdf must set MAT_FLAG_PBR_BSDF so the Cornell harness can \
             reach the Disney diffuse branch at all (#2477)"
        );
        assert_eq!(bsdf.metalness, plain.metalness);
        assert_eq!(bsdf.roughness, plain.roughness);
        assert_eq!(bsdf.diffuse_color, plain.diffuse_color);
    }

    /// Regression for #2514 (REN-D21-2026-08-07-02): every OTHER Cornell
    /// material constructor — including `pbr_bsdf` itself — leaves
    /// `subsurface`/`sheen`/`sheen_tint`/`anisotropic` at
    /// `Material::default()`'s zero, so `disneyDiffuseSplit` runs with all
    /// three distinguishing parameters pinned off even when
    /// `MAT_FLAG_PBR_BSDF` is set. `pbr_bsdf_lobes` must drive all four
    /// non-zero while still setting the flag and preserving `pbr_bsdf`'s
    /// metalness/roughness/color plumbing.
    #[test]
    fn pbr_bsdf_lobes_material_drives_all_four_disney_scalars() {
        use byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;

        // Every other constructor, `pbr_bsdf` included: all four lobes
        // stay at zero (the pre-#2514 state).
        assert_eq!(matte(WHITE).subsurface, 0.0);
        let plain_bsdf = pbr_bsdf([0.9, 0.85, 0.55], 0.5, 0.35);
        assert_eq!(plain_bsdf.subsurface, 0.0);
        assert_eq!(plain_bsdf.sheen, 0.0);
        assert_eq!(plain_bsdf.sheen_tint, 0.0);
        assert_eq!(plain_bsdf.anisotropic, 0.0);

        let lobes = pbr_bsdf_lobes([0.9, 0.85, 0.55], 0.5, 0.35, 0.4, 0.3, 0.2, 0.1);
        assert_eq!(lobes.subsurface, 0.4);
        assert_eq!(lobes.sheen, 0.3);
        assert_eq!(lobes.sheen_tint, 0.2);
        assert_eq!(lobes.anisotropic, 0.1);
        assert_ne!(
            lobes.effect_shader_flags & PBR_BSDF,
            0,
            "pbr_bsdf_lobes must still set MAT_FLAG_PBR_BSDF — driving the \
             lobe scalars without it would silently no-op (#2477)"
        );
        assert_eq!(lobes.metalness, plain_bsdf.metalness);
        assert_eq!(lobes.roughness, plain_bsdf.roughness);
        assert_eq!(lobes.diffuse_color, plain_bsdf.diffuse_color);
    }
}
