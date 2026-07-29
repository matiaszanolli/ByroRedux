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
//! plausible-looking image. No `WeatherDataRes` is inserted, so
//! `weather_system` stays inert and the direction does not drift with TOD.
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

use byroredux_core::ecs::{
    GlobalTransform, LightSource, Material, MeshHandle, TextureHandle, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{box_vertices_colored, uv_sphere, VulkanContext, MATERIAL_KIND_GLASS};

use crate::components::CellLightingRes;
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

/// Un-normalised sun direction for `--cornell-sun`, in the engine's
/// canonical convention: the vector points **from the scene toward the
/// sun**, in Y-up world space. That is what `systems::weather`'s
/// `compute_sun_arc` produces (`y = sin(arc angle)`, positive while the
/// sun is up) and what `env_translate` copies verbatim into *both*
/// `CellLightingRes::directional_dir` and `SkyParamsRes::sun_direction`.
///
/// Deliberately asymmetric with three distinct component magnitudes and
/// all-positive signs: the tall block's floor shadow then falls toward
/// `-X/-Z` at an angle no axis swap or sign flip reproduces, so a
/// convention regression relocates it visibly instead of yielding a
/// different-but-plausible image. Normalised at use — `SkyParams`
/// consumers (`draw.rs`, `water.frag`'s caustic synthesis) assume a unit
/// vector.
const SUN_DIR_RAW: Vec3 = Vec3::new(0.6, 0.84, 0.4);

/// Unit-length [`SUN_DIR_RAW`].
fn sun_dir() -> [f32; 3] {
    SUN_DIR_RAW.normalize().to_array()
}

/// Build the Cornell box into `world`, uploading all meshes + BLAS through
/// `ctx`. Returns `(camera_position, camera_target)` so the caller can
/// place the fly-camera looking into the open front of the box.
///
/// `sun` selects the exterior variant (`--cornell-sun`): see the module
/// header for what changes and why (#1942).
pub(crate) fn setup_cornell_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    sun: bool,
) -> (Vec3, Vec3) {
    install_cornell_lighting(world, sun);

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
    });
}

/// Matte dielectric — the diffuse Cornell surface.
fn matte(color: [f32; 3]) -> Material {
    Material {
        diffuse_color: color,
        roughness: 0.95,
        metalness: 0.0,
        ..Default::default()
    }
}

/// Explicit PBR probe with caller-chosen metalness/roughness.
fn pbr(color: [f32; 3], metalness: f32, roughness: f32) -> Material {
    Material {
        diffuse_color: color,
        metalness,
        roughness,
        ..Default::default()
    }
}

/// `MATERIAL_KIND_GLASS` probe — forces the glass-smooth roughness so the
/// IOR refraction path engages (the gate keys on
/// `materialKind == MATERIAL_KIND_GLASS && roughness < 0.35`, not `alpha`),
/// matching the spawn-time `classify_glass_into_material` contract.
/// `alpha: 0.25` below sets `finalAlpha` for these probes to ~0.25 (not
/// 1.0) — currently unconsumed downstream (`taa.comp`/`composite.frag`
/// don't read it), but latent-fragile if a future composite branch keys
/// on alpha for glass/decal classification. See #676 / DEN-6.
fn glass(color: [f32; 3]) -> Material {
    let mut material = Material {
        diffuse_color: color,
        material_kind: MATERIAL_KIND_GLASS,
        alpha: 0.25,
        ..Default::default()
    };
    material
        .apply_surface_behavior(byroredux_core::ecs::components::material::GLASS_SURFACE_BEHAVIOR);
    material
}

/// Self-illuminated probe. `mult` scales `emissive_color`.
fn emissive(color: [f32; 3], mult: f32) -> Material {
    use byroredux_core::ecs::components::material::EmissiveSource;
    Material {
        diffuse_color: color,
        emissive_color: color,
        emissive_mult: mult,
        emissive_source: EmissiveSource::Material,
        roughness: 0.9,
        ..Default::default()
    }
}

/// Spawn a renderable probe carrying `Transform`, `GlobalTransform`,
/// `MeshHandle`, `Material`, and `Name`. `GlobalTransform` is seeded to
/// match `Transform` so the first rendered frame is correct before
/// transform propagation runs.
fn spawn_object(
    world: &mut World,
    mesh: MeshHandle,
    tex: TextureHandle,
    pos: Vec3,
    rot: Quat,
    material: Material,
    name: &str,
) {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, rot, 1.0));
    world.insert(e, GlobalTransform::new(pos, rot, 1.0));
    world.insert(e, mesh);
    world.insert(e, tex);
    world.insert(e, material);
    name_entity(world, e, name);
}

/// Spawn a named point [`LightSource`] at `pos`. `radius` is the
/// influence falloff distance; `color` is the (un-tonemapped, linear)
/// radiance.
fn spawn_point_light(world: &mut World, pos: Vec3, radius: f32, color: [f32; 3], name: &str) {
    let light = world.spawn();
    world.insert(light, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(light, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        light,
        LightSource {
            radius,
            color,
            ..Default::default()
        },
    );
    name_entity(world, light, name);
}

fn name_entity(world: &mut World, entity: byroredux_core::ecs::EntityId, name: &str) {
    let interned = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern(name)
    };
    world.insert(entity, byroredux_core::ecs::components::Name(interned));
}

/// Accumulates uploaded meshes so their BLAS can be built in one batch,
/// matching the demo-scene upload pattern in `scene::setup_scene`.
struct MeshBuilder<'a> {
    ctx: &'a mut VulkanContext,
    pending: Vec<(u32, u32, u32)>,
}

impl<'a> MeshBuilder<'a> {
    fn new(ctx: &'a mut VulkanContext) -> Self {
        Self {
            ctx,
            pending: Vec::new(),
        }
    }

    fn box_mesh(&mut self, half: [f32; 3]) -> MeshHandle {
        let (v, i) = box_vertices_colored(half, [1.0, 1.0, 1.0]);
        self.upload(&v, &i)
    }

    fn sphere(&mut self, radius: f32) -> MeshHandle {
        let (v, i) = uv_sphere(radius, [1.0, 1.0, 1.0], 96, 128);
        self.upload(&v, &i)
    }

    fn upload(&mut self, verts: &[byroredux_renderer::Vertex], idxs: &[u32]) -> MeshHandle {
        let alloc = self.ctx.allocator.as_ref().unwrap();
        let rt = self.ctx.device_caps.ray_query_supported;
        let upload_ctx = GpuUploadCtx {
            device: &self.ctx.device,
            allocator: alloc,
            queue: &self.ctx.graphics_queue,
            command_pool: self.ctx.transfer_pool,
        };
        let handle = self
            .ctx
            .mesh_registry
            .upload(upload_ctx, verts, idxs, rt, None)
            .expect("Cornell mesh upload failed");
        self.pending
            .push((handle, verts.len() as u32, idxs.len() as u32));
        MeshHandle(handle)
    }

    /// Build BLAS for every uploaded mesh in one batched call.
    fn finish(self) {
        self.ctx.build_blas_batched(&self.pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::cornell_sun_mode;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
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
    }
}
