//! Cornell-box test harness — a self-contained reference scene for
//! validating ray-traced materials and lighting without on-disk game
//! data. Activated with the `--cornell` CLI flag (handled in
//! [`crate::scene::setup_scene`]).
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

/// Build the Cornell box into `world`, uploading all meshes + BLAS through
/// `ctx`. Returns `(camera_position, camera_target)` so the caller can
/// place the fly-camera looking into the open front of the box.
pub(crate) fn setup_cornell_scene(world: &mut World, ctx: &mut VulkanContext) -> (Vec3, Vec3) {
    // Dark interior so the single ceiling light dominates — the classic
    // Cornell look. Directional zeroed (no sun); interior flag set so the
    // renderer treats any residual directional as fill. Fog pushed far
    // out of the way.
    world.insert_resource(CellLightingRes {
        ambient: [0.03, 0.03, 0.03],
        directional_color: [0.0, 0.0, 0.0],
        directional_dir: [0.0, -1.0, 0.0],
        is_interior: true,
        fog_color: [0.0, 0.0, 0.0],
        fog_near: 100_000.0,
        fog_far: 1_000_000.0,
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
        (h_slab, Vec3::new(0.0, HEIGHT + T, 0.0), WHITE, "ceiling"),
        (back_slab, Vec3::new(0.0, cy, -HALF_W - T), WHITE, "back_wall"),
        (side_slab, Vec3::new(-HALF_W - T, cy, 0.0), RED, "left_wall_red"),
        (side_slab, Vec3::new(HALF_W + T, cy, 0.0), GREEN, "right_wall_green"),
    ];
    for &(mesh, pos, color, name) in walls {
        spawn_object(world, mesh, neutral, pos, Quat::IDENTITY, matte(color), name);
    }

    // ── Ceiling area light ──────────────────────────────────────────
    // An emissive panel (the visible light) plus a point LightSource just
    // below it (the actual direct illumination — emissive-only GI is a
    // known weak spot this harness is meant to expose).
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

    // ── Camera-side key/fill light ──────────────────────────────────
    // The ceiling light alone sits *behind* the front probe rows, so
    // their camera-facing hemispheres fall into near-shadow and no
    // material differences are visible. This second light, placed high
    // and off to one side near the camera, rakes the camera-facing
    // sides — giving each probe a GGX highlight whose shape/size reveals
    // roughness, and an albedo-tinted (vs white) specular that reveals
    // metalness. Dimmer than the key so the Cornell colour-bleed look
    // survives. (Whether GI *alone* should fill these faces is a
    // separate question tracked for a later pass.)
    spawn_point_light(
        world,
        Vec3::new(2.0, HEIGHT * 0.8, HALF_W + 1.0),
        40.0,
        [1.1, 1.1, 1.15],
        "camera_fill_light",
    );

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
    // Glass is OPAQUE (no AlphaBlend, opaque neutral texture → finalAlpha
    // 1.0): the IOR refraction ray IS the transmission — it samples the
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
        "Cornell box ready: {} entities. Tweak materials live via `mat.list` / \
         `mat.set <id> <field> <value>` over byro-dbg.",
        world.next_entity_id()
    );

    // Camera: stand outside the open front, slightly above mid-height,
    // looking at the room center.
    let target = Vec3::new(0.0, HEIGHT * 0.45, 0.0);
    let pos = Vec3::new(0.0, HEIGHT * 0.55, HALF_W + 6.0);
    (pos, target)
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

/// `MATERIAL_KIND_GLASS` probe — forces the glass-smooth roughness and a
/// transmissive alpha so the IOR refraction path engages, matching the
/// spawn-time `classify_glass_into_material` contract.
fn glass(color: [f32; 3]) -> Material {
    Material {
        diffuse_color: color,
        material_kind: MATERIAL_KIND_GLASS,
        roughness: 0.10,
        metalness: 0.0,
        alpha: 0.25,
        ..Default::default()
    }
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

    fn upload(
        &mut self,
        verts: &[byroredux_renderer::Vertex],
        idxs: &[u32],
    ) -> MeshHandle {
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
