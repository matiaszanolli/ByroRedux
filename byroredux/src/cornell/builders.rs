//! Shared material builders, spawn helpers, and `MeshBuilder` (#5090
//! split of `cornell.rs`) used by every Cornell-family scene.
use super::*;

/// Matte dielectric — the diffuse Cornell surface.
pub(super) fn matte(color: [f32; 3]) -> Material {
    Material {
        diffuse_color: color,
        roughness: 0.95,
        metalness: 0.0,
        ..Default::default()
    }
}

/// Explicit PBR probe with caller-chosen metalness/roughness.
///
/// #2477 / REN-D21-2026-08-07-01 — this constructor, like every other
/// probe in the harness, leaves `effect_shader_flags` at
/// `Material::default()`'s `0`, so `MAT_FLAG_PBR_BSDF` is clear and the
/// shared direct-lighting BRDF (`include/lighting.glsl`,
/// `triangle.frag`) takes the legacy Lambert diffuse branch, not the
/// Disney (`disneyDiffuseSplit`) branch every BGSM/BGEM-sourced game
/// surface takes. See [`pbr_bsdf`] for the Disney-branch sibling.
pub(super) fn pbr(color: [f32; 3], metalness: f32, roughness: f32) -> Material {
    Material {
        diffuse_color: color,
        metalness,
        roughness,
        ..Default::default()
    }
}

/// Disney-BSDF sibling of [`pbr`] — sets `MAT_FLAG_PBR_BSDF` so the
/// shared direct-lighting BRDF takes the `disneyDiffuseSplit` branch
/// instead of legacy Lambert, matching every real BGSM/BGEM-sourced
/// surface (#1352 sets this bit for all `is_pbr` content). Without a
/// probe on this branch, a regression isolated to `disneyDiffuseSplit`
/// (or its sheen/subsurface lobe) bisects clean against Cornell and only
/// reproduces in-game — the false-all-clear failure mode #1942 fixed for
/// the sun path (#2477 / REN-D21-2026-08-07-01).
pub(super) fn pbr_bsdf(color: [f32; 3], metalness: f32, roughness: f32) -> Material {
    let mut material = pbr(color, metalness, roughness);
    material.effect_shader_flags |= byroredux_renderer::vulkan::material::material_flag::PBR_BSDF;
    material
}

/// [`pbr_bsdf`] sibling that also drives the four Disney lobe scalars no
/// source format authors — `subsurface`/`sheen`/`sheen_tint`/
/// `anisotropic` — so a probe on this constructor actually exercises
/// `disneyDiffuseSplit`'s distinguishing parameters instead of running it
/// with all three pinned at zero (degenerating back toward Burley-only).
/// #2514 / REN-D21-2026-08-07-02 — the enabling half of the
/// REN-D21-2026-08-07-01 gap: even with `MAT_FLAG_PBR_BSDF` set, no CPU
/// producer could reach these fields before this constructor + the
/// matching `mat.set` arms.
pub(super) fn pbr_bsdf_lobes(
    color: [f32; 3],
    metalness: f32,
    roughness: f32,
    subsurface: f32,
    sheen: f32,
    sheen_tint: f32,
    anisotropic: f32,
) -> Material {
    let mut material = pbr_bsdf(color, metalness, roughness);
    material.subsurface = subsurface;
    material.sheen = sheen;
    material.sheen_tint = sheen_tint;
    material.anisotropic = anisotropic;
    material
}

/// `MATERIAL_KIND_GLASS` probe — forces the glass-smooth roughness so the
/// IOR refraction path engages (the gate keys on
/// `materialKind == MATERIAL_KIND_GLASS && roughness < 0.35`, not `alpha`),
/// matching the spawn-time `classify_glass_into_material` contract.
/// `alpha: 0.25` below sets `finalAlpha` for these probes to ~0.25 (not
/// 1.0). It is unconsumed by the *composite/TAA passes* specifically
/// (`taa.comp`/`composite.frag` don't read it), and latent-fragile if a
/// future composite branch keys on alpha for glass/decal classification.
/// See #676 / DEN-6.
///
/// It is NOT inert engine-wide (#2515): the value reaches
/// `GpuMaterial.material_alpha` through `to_gpu_material` and is hashed by
/// `hash_gpu_material_fields` — since #4201 that is the byte hash of the
/// built struct, so `material_alpha` participates through its byte
/// representation — which `MaterialTable::intern` keys on. So it is part
/// of the material dedup identity, and changing it
/// splits or merges material-table slots — these glass probes already
/// occupy a slot distinct from an otherwise identical opaque dielectric
/// purely because of it. Relevant to anyone measuring dedup ratio via
/// `ctx.scratch` (#780 / PERF-N1) against the Cornell scene.
pub(super) fn glass(color: [f32; 3]) -> Material {
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
pub(super) fn emissive(color: [f32; 3], mult: f32) -> Material {
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

/// `MATERIAL_KIND_FIRE_REFRACTION` probe (#2249 / REN-D21-03). This kind
/// overloads `Material::ior` as the authored distortion strength — see
/// `triangle.frag`'s fire-refraction branch, `clamp(mat.ior, 0.0, 1.0)` —
/// not a real refractive index. The caller must also attach a
/// `MaterialTextureHandles` with a non-zero `textures.normal`
/// ([`synthesize_wavy_normal_map`]): without one, `N == macroN` at every
/// fragment and `tangentWarp = N - macroN * dot(N, macroN)` is
/// structurally zero regardless of this value.
pub(super) fn fire_refraction(distortion_strength: f32) -> Material {
    Material {
        diffuse_color: [1.0, 1.0, 1.0],
        material_kind: MATERIAL_KIND_FIRE_REFRACTION,
        ior: distortion_strength,
        ..Default::default()
    }
}

/// Synthesize a small tangent-space normal map with a spatially-varying
/// wave pattern (#2249 / REN-D21-03). Cornell has no on-disk textures, so
/// without this the fire-refraction probe's normal map slot stays at the
/// bindless-0 "absent" sentinel and its distortion path is a structural
/// no-op (see [`fire_refraction`]). Flat/neutral (`(0,0,1)` everywhere)
/// would compile and shade but still leave `tangentWarp` zero since
/// `N == macroN`; the wave pattern guarantees genuine per-fragment
/// disagreement between the two.
pub(super) fn synthesize_wavy_normal_map(ctx: &mut VulkanContext) -> u32 {
    const SIZE: u32 = 16;
    let pixels = wavy_normal_map_pixels(SIZE);
    let alloc = ctx.allocator.as_ref().unwrap();
    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator: alloc,
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    ctx.texture_registry
        .register_rgba(upload_ctx, SIZE, SIZE, &pixels)
        .expect("Cornell normal-map synth upload failed")
}

/// Pixel data for [`synthesize_wavy_normal_map`], split out so the pattern
/// itself is testable without a Vulkan device. RGBA8, tangent-space
/// encoding (`channel = component * 0.5 + 0.5`), a sine wave over both
/// axes so no two rows/columns share the same normal.
pub(super) fn wavy_normal_map_pixels(size: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = x as f32 / size as f32;
            let v = y as f32 / size as f32;
            let nx = (u * std::f32::consts::TAU * 3.0).sin() * 0.6;
            let ny = (v * std::f32::consts::TAU * 3.0).sin() * 0.6;
            let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
            pixels.push(((nx * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(((ny * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(((nz * 0.5 + 0.5) * 255.0) as u8);
            pixels.push(255u8);
        }
    }
    pixels
}

/// Spawn a renderable probe carrying `Transform`, `GlobalTransform`,
/// `MeshHandle`, `Material`, and `Name`. `GlobalTransform` is seeded to
/// match `Transform` so the first rendered frame is correct before
/// transform propagation runs.
pub(super) fn spawn_object(
    world: &mut World,
    mesh: MeshHandle,
    tex: TextureHandle,
    pos: Vec3,
    rot: Quat,
    material: Material,
    name: &str,
) -> byroredux_core::ecs::EntityId {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, rot, 1.0));
    world.insert(e, GlobalTransform::new(pos, rot, 1.0));
    world.insert(e, mesh);
    world.insert(e, tex);
    world.insert(e, material);
    name_entity(world, e, name);
    e
}

/// Spawn a named point [`LightSource`] at `pos`. `radius` is the
/// influence falloff distance; `color` is the (un-tonemapped, linear)
/// radiance.
pub(super) fn spawn_point_light(world: &mut World, pos: Vec3, radius: f32, color: [f32; 3], name: &str) {
    let light = world.spawn();
    world.insert(light, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(light, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        light,
        LightSource::from_legacy_world_units(
            radius,
            color,
            byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            1.0,
            byroredux_core::ecs::LightKind::Point,
            [0.0; 3],
            0.0,
            byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
        ),
    );
    name_entity(world, light, name);
}

/// Spawn a named local [`FogVolume`] probe, centered on the entity's own
/// `GlobalTransform` (`FogBounds::center` left at the origin). `pos` and
/// `half_extents` are world units, same convention as every mesh probe in
/// this file. `extinction_per_meter` / `single_scatter_albedo` are
/// authored exactly like a real content producer would — the collection
/// path (`render/fog_volumes.rs`) applies the same `WORLD_UNITS_PER_METER`
/// conversion either way, so there's no Cornell-specific scaling here.
pub(super) fn spawn_fog_volume(world: &mut World, pos: Vec3, half_extents: Vec3, name: &str) {
    spawn_fog_volume_with_extinction(world, pos, half_extents, 40.0, name);
}

pub(super) fn spawn_fog_volume_with_extinction(
    world: &mut World,
    pos: Vec3,
    half_extents: Vec3,
    extinction_per_meter: f32,
    name: &str,
) {
    let e = world.spawn();
    world.insert(e, Transform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(e, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
    world.insert(
        e,
        FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents,
                shape: FogShape::Box,
            }),
            extinction_per_meter,
            single_scatter_albedo: [0.92, 0.92, 0.97],
            edge_softness: 0.35,
            profile: FogProfile::Homogeneous,
            emissive_radiance: [0.0; 3],
            emission_temperature_k: 0.0,
            source: FogSource::AuthoredMesh,
        },
    );
    name_entity(world, e, name);
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CombustionProbeKind {
    Flame,
    Explosion {
        start_delay_seconds: f32,
        lifetime_seconds: f32,
    },
    NuclearExplosion {
        start_delay_seconds: f32,
        lifetime_seconds: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CombustionProbeSpec {
    pub(super) kind: CombustionProbeKind,
    pub(super) position: Vec3,
    pub(super) half_extents: Vec3,
    pub(super) name: &'static str,
}

/// Opt-in canonical combustion source used without game data. The kind is
/// resolved here at the scene-production boundary; the renderer receives the
/// same [`FogVolume`] contract as imported particle and runtime effects.
pub(super) fn spawn_combustion_probe(world: &mut World, spec: CombustionProbeSpec) {
    let e = world.spawn();
    world.insert(e, Transform::new(spec.position, Quat::IDENTITY, 1.0));
    world.insert(e, GlobalTransform::new(spec.position, Quat::IDENTITY, 1.0));
    let (profile, shape, regime, extinction_per_meter) = match spec.kind {
        CombustionProbeKind::Flame => (
            FogProfile::Flame,
            FogShape::Ellipsoid,
            CombustionRegime::FLAME,
            6.0,
        ),
        CombustionProbeKind::Explosion { .. } => (
            FogProfile::OilExplosion,
            FogShape::Sphere,
            CombustionRegime::EXPLOSION,
            10.0,
        ),
        CombustionProbeKind::NuclearExplosion { .. } => (
            FogProfile::NuclearExplosion,
            FogShape::Sphere,
            CombustionRegime::NUCLEAR_EXPLOSION,
            12.0,
        ),
    };
    let emissive_radiance = regime
        .emissive_radiance()
        .expect("the finite Cornell combustion probe temperature is representable");
    world.insert(
        e,
        FogVolume {
            bounds: Some(FogBounds {
                center: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                half_extents: spec.half_extents,
                shape,
            }),
            extinction_per_meter,
            single_scatter_albedo: regime.single_scatter_albedo(),
            edge_softness: 0.3,
            profile,
            emissive_radiance,
            emission_temperature_k: regime.temperature_k(),
            source: FogSource::RuntimeEffect,
        },
    );
    let timeline = match spec.kind {
        CombustionProbeKind::Explosion {
            start_delay_seconds,
            lifetime_seconds,
        }
        | CombustionProbeKind::NuclearExplosion {
            start_delay_seconds,
            lifetime_seconds,
        } => Some((start_delay_seconds, lifetime_seconds)),
        CombustionProbeKind::Flame => None,
    };
    if let Some((start_delay_seconds, lifetime_seconds)) = timeline {
        let now_seconds = { world.resource::<TotalTime>().0 };
        world.insert(
            e,
            CombustionState::one_shot(now_seconds + start_delay_seconds, lifetime_seconds),
        );
    }
    name_entity(world, e, spec.name);
}

pub(super) fn name_entity(world: &mut World, entity: byroredux_core::ecs::EntityId, name: &str) {
    let interned = {
        let mut pool = world.resource_mut::<StringPool>();
        pool.intern(name)
    };
    world.insert(entity, byroredux_core::ecs::components::Name(interned));
}

/// Accumulates uploaded meshes so their BLAS can be built in one batch,
/// matching the demo-scene upload pattern in `scene::setup_scene`.
pub(super) struct MeshBuilder<'a> {
    pub(super) ctx: &'a mut VulkanContext,
    pending: Vec<(u32, u32, u32)>,
}

impl<'a> MeshBuilder<'a> {
    pub(super) fn new(ctx: &'a mut VulkanContext) -> Self {
        Self {
            ctx,
            pending: Vec::new(),
        }
    }

    pub(super) fn box_mesh(&mut self, half: [f32; 3]) -> MeshHandle {
        let (v, i) = box_vertices_colored(half, [1.0, 1.0, 1.0]);
        self.upload(&v, &i)
    }

    pub(super) fn sphere(&mut self, radius: f32) -> MeshHandle {
        let (v, i) = uv_sphere(radius, [1.0, 1.0, 1.0], 96, 128);
        self.upload(&v, &i)
    }

    pub(super) fn upload(&mut self, verts: &[byroredux_renderer::Vertex], idxs: &[u32]) -> MeshHandle {
        let alloc = self.ctx.allocator.as_ref().unwrap();
        let rt = self.ctx.device_caps.ray_query_supported;
        let upload_ctx = GpuUploadCtx {
            device: &self.ctx.device,
            allocator: alloc,
            queue: &self.ctx.graphics_queue,
            command_pool: self.ctx.transfer_pool,
        };
        // Cornell geometry participates in ordinary scene rendering, even
        // when a real NIF is loaded beside it. Register it in the global
        // geometry pool as well as retaining its per-mesh buffers for BLAS.
        // A per-mesh-only upload works while Cornell is the whole scene, but
        // becomes invalid as soon as a NIF enables the global multi-draw path:
        // that path reads every batch through global offsets.
        let handle = self
            .ctx
            .mesh_registry
            .upload_scene_mesh(upload_ctx, verts, idxs, rt, None)
            .expect("Cornell scene-mesh upload failed");
        self.pending
            .push((handle, verts.len() as u32, idxs.len() as u32));
        MeshHandle(handle)
    }

    /// Upload the same immutable source geometry twice through the NPC
    /// loader's acquire/register flow. The second upload must alias the
    /// first handle instead of allocating a second buffer pair. Only the
    /// representative enters the BLAS batch — re-entering an alias would
    /// drop and rebuild the same handle's BLAS (mirrors `spawn_nif_mesh`'s
    /// fresh-source-only spec push).
    pub(super) fn upload_shared_pair(
        &mut self,
        verts: &[byroredux_renderer::Vertex],
        idxs: &[u32],
    ) -> (MeshHandle, MeshHandle) {
        let alloc = self.ctx.allocator.as_ref().unwrap();
        let rt = self.ctx.device_caps.ray_query_supported;
        let probe = SceneMeshUpload {
            vertices: verts,
            indices: idxs,
            rt_enabled: rt,
            cache_key: None,
        };
        let (matching, fingerprint) = self.ctx.mesh_registry
            .acquire_matching_scene_mesh_with_fingerprint(&probe);
        let first = match matching {
            Some(handle) => handle,
            None => {
                let upload_ctx = GpuUploadCtx {
                    device: &self.ctx.device,
                    allocator: alloc,
                    queue: &self.ctx.graphics_queue,
                    command_pool: self.ctx.transfer_pool,
                };
                let handle = self
                    .ctx
                    .mesh_registry
                    .upload_scene_mesh(upload_ctx, verts, idxs, rt, None)
                    .expect("Cornell shared scene-mesh upload failed");
                self.ctx.mesh_registry.register_scene_geometry_for_sharing_with_fingerprint(
                    handle,
                    Some(fingerprint),
                );
                handle
            }
        };
        let second = self
            .ctx
            .mesh_registry
            .acquire_matching_scene_mesh(&probe)
            .expect("second identical upload must alias the shared source mesh");
        assert_eq!(
            first, second,
            "shared-pair fixture stopped sharing its source mesh"
        );
        self.pending
            .push((first, verts.len() as u32, idxs.len() as u32));
        (MeshHandle(first), MeshHandle(second))
    }

    /// Build BLAS for every uploaded mesh in one batched call.
    pub(super) fn finish(self) {
        self.ctx.build_blas_batched(&self.pending);
    }
}
