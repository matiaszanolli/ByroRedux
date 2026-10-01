//! The RT oracle ladder (#5090 split of `cornell.rs`): rungs L0-L5 and
//! the skinned/mirrored variants, the manifest contract, mode parsing,
//! scene construction, and the shared-skin probe pair.
use super::builders::*;
use super::glass_dragon::SUN_DIR_RAW;
use super::*;

/// These are deliberately separate from the material-showcase variants above:
/// every rung adds exactly one variable, so a failed capture names the first
/// broken contract instead of producing another plausible-looking Cornell
/// image with several possible causes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CornellOracleRung {
    L0,
    L1,
    L2,
    L3,
    L4,
    L5,
    /// L1/L2 with a bone-posed receiver and model-space normal map.
    L1Skinned,
    L2Skinned,
    /// Two independently posed skinned receivers sharing ONE source mesh
    /// through the exact acquire/register flow the NPC loader uses, with
    /// a blocker shadowing only one of them.
    L1SkinnedShared,
    L1Point,
    L1SkinnedPoint,
    /// Local spot cone, with rigid/posed receivers and a posed blocker.
    L1Spot,
    L1SkinnedSpot,
    L2SkinnedSpot,
    /// L2 after warming unused mesh BLAS, for real cache-pressure recovery.
    L2CachePressure,
    /// Same tiny visible set, but the unused cache contains one large mesh.
    L2MixedCachePressure,
    /// L3/L4 with the source reflected across the camera's X plane.
    L3Mirrored,
    L4Mirrored,
}

/// Data contract shared by scene construction, analytic tests, and capture
/// tooling. Later rungs can extend this table without adding another
/// constructor.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CornellOracleManifest {
    pub name: &'static str,
    pub directional_radiance: [f32; 3],
    /// World-space unit vector from the surface toward the source.
    pub direction_toward_source: [f32; 3],
    pub blocker: bool,
    /// Local point-light + fog-volume transport is active. L3 is the open
    /// control and L4 changes only by adding an opaque partition.
    pub volumetric_probe: bool,
    /// L5 adds the canonical dielectric/metal/glass/normal-role probe row.
    pub material_probes: bool,
    pub camera_position: Vec3,
    pub camera_target: Vec3,
    pub primary_debug_view: &'static str,
    pub max_linear_error: f32,
}

// Normalized (1, 1, 2). The receiver faces +Z; the asymmetric source makes
// L2's hard shadow visible below-left of the blocker while retaining a
// hand-derivable N.L.
const ORACLE_LIGHT_DIRECTION: [f32; 3] = [0.408_248_3, 0.408_248_3, 0.816_496_6];
const ORACLE_CAMERA_POSITION: Vec3 = Vec3::new(0.0, 4.0, 10.0);
const ORACLE_CAMERA_TARGET: Vec3 = Vec3::new(0.0, 4.0, 0.0);
/// The surface-shadow oracle is intentionally unit-scale, but volumetrics use
/// Bethesda-world distances (70 units/metre) and a first froxel slab roughly
/// 44 units deep. Scaling L3/L4 gives the medium multiple depth samples while
/// preserving exactly the same projected scene and optical depth.
const ORACLE_VOLUMETRIC_SCALE: f32 = 100.0;

pub(crate) fn cornell_oracle_manifest(rung: CornellOracleRung) -> CornellOracleManifest {
    if matches!(
        rung,
        CornellOracleRung::L3Mirrored | CornellOracleRung::L4Mirrored
    ) {
        let blocked = rung == CornellOracleRung::L4Mirrored;
        let mut manifest = cornell_oracle_manifest(if blocked {
            CornellOracleRung::L4
        } else {
            CornellOracleRung::L3
        });
        manifest.name = if blocked {
            "l4_point_fog_partition_mirrored"
        } else {
            "l3_point_fog_mirrored"
        };
        return manifest;
    }
    if matches!(
        rung,
        CornellOracleRung::L2CachePressure | CornellOracleRung::L2MixedCachePressure
    ) {
        let mut manifest = cornell_oracle_manifest(CornellOracleRung::L2);
        manifest.name = if rung == CornellOracleRung::L2MixedCachePressure {
            "l2_mixed_cache_pressure"
        } else {
            "l2_cache_pressure"
        };
        return manifest;
    }
    if matches!(
        rung,
        CornellOracleRung::L1Point
            | CornellOracleRung::L1SkinnedPoint
            | CornellOracleRung::L1Spot
            | CornellOracleRung::L1SkinnedSpot
            | CornellOracleRung::L2SkinnedSpot
    ) {
        let mut manifest = cornell_oracle_manifest(CornellOracleRung::L1);
        manifest.directional_radiance = [0.0; 3];
        manifest.blocker = rung == CornellOracleRung::L2SkinnedSpot;
        manifest.name = match rung {
            CornellOracleRung::L1Point => "l1_point_lambert",
            CornellOracleRung::L1SkinnedPoint => "l1_skinned_point_lambert",
            CornellOracleRung::L1Spot => "l1_spot_lambert",
            CornellOracleRung::L1SkinnedSpot => "l1_skinned_spot_lambert",
            CornellOracleRung::L2SkinnedSpot => "l2_skinned_spot_blocker",
            _ => unreachable!(),
        };
        return manifest;
    }
    if matches!(
        rung,
        CornellOracleRung::L1Skinned | CornellOracleRung::L2Skinned
    ) {
        let mut manifest = cornell_oracle_manifest(if rung == CornellOracleRung::L1Skinned {
            CornellOracleRung::L1
        } else {
            CornellOracleRung::L2
        });
        manifest.name = if rung == CornellOracleRung::L1Skinned {
            "l1_skinned_lambert"
        } else {
            "l2_skinned_blocker"
        };
        return manifest;
    }
    if matches!(rung, CornellOracleRung::L1SkinnedShared) {
        let mut manifest = cornell_oracle_manifest(CornellOracleRung::L1);
        manifest.name = "l1_skinned_shared_pair";
        return manifest;
    }
    let (
        name,
        directional_radiance,
        blocker,
        volumetric_probe,
        material_probes,
        primary_debug_view,
    ) = match rung {
        CornellOracleRung::L0 => ("l0_dark_plane", [0.0; 3], false, false, false, "direct"),
        CornellOracleRung::L1 => (
            "l1_directional_lambert",
            [1.0; 3],
            false,
            false,
            false,
            "direct",
        ),
        CornellOracleRung::L2 => (
            "l2_opaque_blocker",
            [1.0; 3],
            true,
            false,
            false,
            "shadow_visibility",
        ),
        CornellOracleRung::L3 => (
            "l3_point_fog_open",
            [0.0; 3],
            false,
            true,
            false,
            "composite_term",
        ),
        CornellOracleRung::L4 => (
            "l4_point_fog_partition",
            [0.0; 3],
            true,
            true,
            false,
            "composite_term",
        ),
        CornellOracleRung::L5 => (
            "l5_material_roles",
            [1.0; 3],
            false,
            false,
            true,
            "material_lobe",
        ),
        CornellOracleRung::L1Skinned
        | CornellOracleRung::L1SkinnedShared
        | CornellOracleRung::L2Skinned
        | CornellOracleRung::L1Point
        | CornellOracleRung::L1SkinnedPoint
        | CornellOracleRung::L1Spot
        | CornellOracleRung::L1SkinnedSpot
        | CornellOracleRung::L2SkinnedSpot
        | CornellOracleRung::L2CachePressure
        | CornellOracleRung::L2MixedCachePressure
        | CornellOracleRung::L3Mirrored
        | CornellOracleRung::L4Mirrored => unreachable!(),
    };
    CornellOracleManifest {
        name,
        directional_radiance,
        direction_toward_source: ORACLE_LIGHT_DIRECTION,
        blocker,
        volumetric_probe,
        material_probes,
        camera_position: ORACLE_CAMERA_POSITION
            * if volumetric_probe {
                ORACLE_VOLUMETRIC_SCALE
            } else {
                1.0
            },
        camera_target: ORACLE_CAMERA_TARGET
            * if volumetric_probe {
                ORACLE_VOLUMETRIC_SCALE
            } else {
                1.0
            },
        primary_debug_view,
        // Linear-light probe tolerance. Image-level thresholds remain owned by
        // the capture runner rather than being hidden in this scene builder.
        max_linear_error: 0.015,
    }
}

/// Parse `--cornell-oracle` (l0-l5 and actor/local-light variants) without silently falling back
/// to the demo scene on a typo.
pub(crate) fn cornell_oracle_rung(args: &[String]) -> Result<Option<CornellOracleRung>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--cornell-oracle") else {
        return Ok(None);
    };
    let value = args.get(index + 1).ok_or_else(|| {
        "--cornell-oracle requires one of: l0, l1, l2, l3, l4, l5, l1-skinned, l1-skinned-shared, l2-skinned, l1-point, l1-skinned-point, l1-spot, l1-skinned-spot, l2-skinned-spot, l2-cache-pressure, l2-cache-pressure-large, l3-mirrored, l4-mirrored"
            .to_string()
    })?;
    let rung = match value.to_ascii_lowercase().as_str() {
        "l0" => CornellOracleRung::L0,
        "l1" => CornellOracleRung::L1,
        "l2" => CornellOracleRung::L2,
        "l3" => CornellOracleRung::L3,
        "l4" => CornellOracleRung::L4,
        "l5" => CornellOracleRung::L5,
        "l1-skinned" => CornellOracleRung::L1Skinned,
        "l1-skinned-shared" => CornellOracleRung::L1SkinnedShared,
        "l2-skinned" => CornellOracleRung::L2Skinned,
        "l1-point" => CornellOracleRung::L1Point,
        "l1-skinned-point" => CornellOracleRung::L1SkinnedPoint,
        "l1-spot" => CornellOracleRung::L1Spot,
        "l1-skinned-spot" => CornellOracleRung::L1SkinnedSpot,
        "l2-skinned-spot" => CornellOracleRung::L2SkinnedSpot,
        "l2-cache-pressure" => CornellOracleRung::L2CachePressure,
        "l2-cache-pressure-large" => CornellOracleRung::L2MixedCachePressure,
        "l3-mirrored" => CornellOracleRung::L3Mirrored,
        "l4-mirrored" => CornellOracleRung::L4Mirrored,
        _ => {
            return Err(format!(
                "unknown Cornell oracle rung '{value}'; expected one of: l0, l1, l2, l3, l4, l5, l1-skinned, l1-skinned-shared, l2-skinned, l1-point, l1-skinned-point, l1-spot, l1-skinned-spot, l2-skinned-spot, l2-cache-pressure, l2-cache-pressure-large, l3-mirrored, l4-mirrored"
            ));
        }
    };
    Ok(Some(rung))
}

/// Parse the diagnostic world translation applied to every Cornell oracle
/// object and its fixed camera. This keeps the analytic scene identical while
/// exercising camera-relative rendering and absolute ray-query coordinates.
pub(crate) fn cornell_oracle_world_offset(args: &[String]) -> Result<Vec3, String> {
    let Some(index) = args
        .iter()
        .position(|arg| arg == "--cornell-oracle-world-offset")
    else {
        return Ok(Vec3::ZERO);
    };
    let value = args.get(index + 1).ok_or_else(|| {
        "--cornell-oracle-world-offset requires finite comma-separated x,y,z".to_string()
    })?;
    let parts: Vec<_> = value.split(',').collect();
    if parts.len() != 3 {
        return Err(format!(
            "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
        ));
    }
    let mut coordinates = [0.0; 3];
    for (slot, part) in coordinates.iter_mut().zip(parts) {
        *slot = part.parse::<f32>().map_err(|_| {
            format!(
                "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
            )
        })?;
        if !slot.is_finite() {
            return Err(format!(
                "--cornell-oracle-world-offset requires finite comma-separated x,y,z, got '{value}'"
            ));
        }
    }
    Ok(Vec3::from_array(coordinates))
}

impl CornellOracleManifest {
    /// Expected legacy-Lambert direct term on the +Z receiver. Oracle materials
    /// set IOR=1 and specular strength=0, so Fresnel and specular are exactly
    /// absent and the clustered-light arm reduces to albedo * Li * N.L.
    pub(crate) fn expected_unshadowed_direct(self, albedo: [f32; 3]) -> [f32; 3] {
        let n_dot_l = self.direction_toward_source[2].max(0.0);
        [
            albedo[0] * self.directional_radiance[0] * n_dot_l,
            albedo[1] * self.directional_radiance[1] * n_dot_l,
            albedo[2] * self.directional_radiance[2] * n_dot_l,
        ]
    }
}

/// Unit-length [`SUN_DIR_RAW`].
pub(super) fn sun_dir() -> [f32; 3] {
    SUN_DIR_RAW.normalize().to_array()
}

/// Construct the controlled L0-L5 correctness scene selected by
/// `--cornell-oracle`. The richer `--cornell` showcase remains untouched.
pub(crate) fn setup_cornell_oracle_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    rung: CornellOracleRung,
    world_offset: Vec3,
) -> (Vec3, Vec3) {
    let manifest = cornell_oracle_manifest(rung);
    let skinned = matches!(
        rung,
        CornellOracleRung::L1Skinned
            | CornellOracleRung::L2Skinned
            | CornellOracleRung::L1SkinnedPoint
            | CornellOracleRung::L1SkinnedSpot
            | CornellOracleRung::L2SkinnedSpot
    );
    if manifest.volumetric_probe {
        // Include the volumetric integral but bypass presentation exposure,
        // grading and stochastic dither. The capture then remains a direct
        // HDR-linear transport oracle.
        ctx.set_render_debug_mode(RenderDebugMode::CompositeTerm);
    } else if manifest.material_probes {
        ctx.set_render_debug_mode(RenderDebugMode::MaterialLobe);
    }
    let expected_unshadowed = manifest.expected_unshadowed_direct([1.0; 3]);
    world.insert_resource(CellLightingRes {
        ambient: [0.0; 3],
        directional_color: manifest.directional_radiance,
        directional_dir: manifest.direction_toward_source,
        is_interior: true,
        fog_color: [0.0; 3],
        fog_near: 100_000.0,
        fog_far: 1_000_000.0,
        fog_medium: crate::fog::FogMedium::from_legacy_ramp(100_000.0, 1_000_000.0, None),
        // Preserve the manifest's radiance exactly instead of applying the
        // legacy 0.6 XCLL fallback calibration.
        directional_fade: Some(1.0),
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

    if matches!(rung, CornellOracleRung::L1SkinnedShared) {
        return setup_shared_skin_pair_scene(world, ctx, world_offset, &manifest);
    }

    if matches!(
        rung,
        CornellOracleRung::L2CachePressure | CornellOracleRung::L2MixedCachePressure
    ) {
        // Model a previous cell's cached BLAS, not extra visible casters.
        // Unique handles prevent deduplication. With a small (but viable)
        // residency budget this leaves too little space for the active pair;
        // ordinary frame-driven eviction/recovery must bring them back.
        let mut cache = MeshBuilder::new(ctx);
        if rung == CornellOracleRung::L2MixedCachePressure {
            // A cache-wide mean wrongly prices both tiny current casters as
            // this unused high-detail mesh and refuses recovery forever.
            let (vertices, indices) = uv_sphere(1.0, [1.0; 3], 32, 48);
            cache.upload(&vertices, &indices);
        } else {
            for index in 0..64 {
                cache.box_mesh([0.5 + index as f32 * 0.01; 3]);
            }
        }
        cache.finish();
    }
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let oracle_scale = if manifest.volumetric_probe {
        ORACLE_VOLUMETRIC_SCALE
    } else {
        1.0
    };
    let receiver_half = [4.0, 4.0, 0.05].map(|v| v * oracle_scale);
    let receiver_mesh = if skinned {
        oracle_skinned_box(&mut builder, receiver_half)
    } else {
        builder.box_mesh(receiver_half)
    };
    // L3/L4 use a black surface so the final capture contains only
    // in-scattered volumetric radiance; direct and indirect surface terms
    // cannot masquerade as a fog visibility result.
    let receiver_color = if manifest.volumetric_probe {
        [0.0; 3]
    } else {
        [1.0; 3]
    };
    let mut oracle_matte = matte(receiver_color);
    oracle_matte.ior = 1.0;
    oracle_matte.specular_strength = 0.0;
    let receiver = spawn_object(
        world,
        receiver_mesh,
        neutral,
        Vec3::new(0.0, 4.0, -0.05) * oracle_scale + world_offset,
        Quat::IDENTITY,
        oracle_matte.clone(),
        "oracle_receiver",
    );
    if skinned {
        attach_oracle_skin(world, receiver, Vec3::new(0.0, 4.0, -0.05) + world_offset);
        // The visible face points +X in the bind pose and +Z after posing.
        // A root-only model-space normal transform incorrectly lights +X.
        let normal_map = builder
            .ctx
            .texture_registry
            .register_rgba(
                GpuUploadCtx {
                    device: &builder.ctx.device,
                    allocator: builder.ctx.allocator.as_ref().unwrap(),
                    queue: &builder.ctx.graphics_queue,
                    command_pool: builder.ctx.transfer_pool,
                },
                // register_rgba uses an sRGB view; encode 0.5 as 188 so the
                // sampled vector is +X (normal DDS views normally use UNORM).
                1,
                1,
                &[255, 188, 188, 255],
            )
            .expect("oracle model-space normal upload");
        let mut material = oracle_matte.clone();
        material.effect_shader_flags |=
            byroredux_renderer::shader_constants::MAT_FLAG_MODEL_SPACE_NORMALS;
        world.insert(receiver, material);
        world.insert(
            receiver,
            MaterialTextureHandles {
                textures: byroredux_nif::import::MaterialTextureSet {
                    normal: normal_map,
                    ..Default::default()
                },
                normal_has_alpha: false,
                tint_has_alpha: false,
                parallax_height_scale: 0.0,
                parallax_max_passes: 0.0,
            },
        );
    }

    if manifest.blocker && !manifest.volumetric_probe {
        let blocker_mesh = if skinned {
            oracle_skinned_box(&mut builder, [0.75; 3])
        } else {
            builder.box_mesh([0.75; 3])
        };
        let blocker = spawn_object(
            world,
            blocker_mesh,
            neutral,
            Vec3::new(0.0, 4.0, 0.75) + world_offset,
            Quat::IDENTITY,
            oracle_matte.clone(),
            "oracle_blocker",
        );
        if skinned {
            attach_oracle_skin(world, blocker, Vec3::new(0.0, 4.0, 0.75) + world_offset);
        }
    }

    if manifest.volumetric_probe {
        // The local medium fills the camera-to-receiver segment. L3 is the
        // open control. L4 adds one thin, edge-on partition at x=0: points on
        // its left must be shadowed from the right-side point light while the
        // right half remains an unchanged lit control. Because the partition
        // is edge-on to the camera, its only substantial image-space effect is
        // the visibility boundary in the fog rather than a broad foreground
        // surface.
        spawn_fog_volume_with_extinction(
            world,
            Vec3::new(0.0, 4.0, 5.0) * oracle_scale + world_offset,
            Vec3::new(3.5, 3.5, 4.0) * oracle_scale,
            40.0 / oracle_scale,
            "oracle_fog_volume",
        );
        spawn_point_light(
            world,
            Vec3::new(
                if matches!(
                    rung,
                    CornellOracleRung::L3Mirrored | CornellOracleRung::L4Mirrored
                ) {
                    -2.5
                } else {
                    2.5
                },
                4.0,
                5.0,
            ) * oracle_scale
                + world_offset,
            20.0 * oracle_scale,
            [2.0; 3],
            "oracle_point_light",
        );

        if manifest.blocker {
            // Half a native unit thick after scaling: enough for a robust
            // ray-query hit, but narrow enough in screen space that a failed
            // XY reconstruction cannot hide its halo inside a broad surface.
            let partition_mesh = builder.box_mesh([0.005, 4.0, 4.0].map(|v| v * oracle_scale));
            spawn_object(
                world,
                partition_mesh,
                neutral,
                Vec3::new(0.0, 4.0, 4.0) * oracle_scale + world_offset,
                Quat::IDENTITY,
                oracle_matte,
                "oracle_opaque_partition",
            );
        }
    }

    if manifest.material_probes {
        let probe = builder.sphere(0.55);
        let probes = [
            (-2.4, matte([0.72, 0.72, 0.72]), "l5_dielectric"),
            (-0.8, pbr_bsdf([0.90, 0.72, 0.22], 1.0, 0.18), "l5_metal"),
            (0.8, glass([0.82, 0.92, 1.0]), "l5_glass"),
            (2.4, matte([0.72, 0.72, 0.72]), "l5_normal_role"),
        ];
        let mut normal_probe = None;
        for (x, material, name) in probes {
            let entity = spawn_object(
                world,
                probe,
                neutral,
                Vec3::new(x, 4.0, 0.65) + world_offset,
                Quat::IDENTITY,
                material,
                name,
            );
            if name == "l5_normal_role" {
                normal_probe = Some(entity);
            }
        }
        let normal_map = synthesize_wavy_normal_map(builder.ctx);
        world.insert(
            normal_probe.expect("L5 normal-role probe must be spawned"),
            MaterialTextureHandles {
                textures: byroredux_nif::import::MaterialTextureSet {
                    normal: normal_map,
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
    }
    builder.finish();

    if rung == CornellOracleRung::L2MixedCachePressure {
        let handles: Vec<u32> = world
            .query::<MeshHandle>()
            .unwrap()
            .iter()
            .map(|(_, handle)| handle.0)
            .collect();
        if let Some(accel) = ctx.accel_manager.as_mut() {
            let required_bytes = accel.required_static_blas_bytes(&handles);
            let unused_bytes = accel.static_blas_bytes() - required_bytes;
            // Device-specific build sizes vary. Keep the unused mesh below
            // budget while its cache-wide mean would overprice the pair.
            let budget = unused_bytes + unused_bytes / 2;
            assert!(
                unused_bytes > required_bytes * 8,
                "mixed-cache fixture needs a genuinely larger unused mesh"
            );
            accel.override_blas_budget_for_test(budget);
            // Model a returned mesh whose BLAS was previously reclaimed.
            // Retain source geometry, and respect normal deferred retirement.
            for handle in handles {
                accel.drop_blas(handle);
            }
            log::warn!(
                "RT TEST mixed-cache recovery: unused_bytes={unused_bytes} required_bytes={required_bytes} budget={budget}"
            );
        }
    }

    if matches!(
        rung,
        CornellOracleRung::L1Point
            | CornellOracleRung::L1SkinnedPoint
            | CornellOracleRung::L1Spot
            | CornellOracleRung::L1SkinnedSpot
            | CornellOracleRung::L2SkinnedSpot
    ) {
        let entity = world.spawn();
        let position = Vec3::new(2.0, 6.0, 4.0) + world_offset;
        world.insert(entity, Transform::from_translation(position));
        world.insert(entity, GlobalTransform::new(position, Quat::IDENTITY, 1.0));
        let spot = matches!(
            rung,
            CornellOracleRung::L1Spot
                | CornellOracleRung::L1SkinnedSpot
                | CornellOracleRung::L2SkinnedSpot
        );
        // Exercise the actual game boundary: FO4's NonShadow Spotlight is
        // a cone with FULL material-aware RT visibility, despite its name.
        let geometry = if spot {
            crate::systems::translate_light(
                &byroredux_plugin::esm::cell::LightData {
                    radius: 8.0,
                    color: [1.0; 3],
                    flags: 0x4000,
                    fov_degrees: 40.0,
                    period_secs: 0.0,
                    intensity_amplitude: 0.0,
                    movement_amplitude: 0.0,
                    falloff_exponent: 1.0,
                    xpwr_form_id: None,
                    starfield_light_type: 0,
                },
                byroredux_plugin::esm::reader::GameKind::Fallout4,
                Quat::from_rotation_arc(Vec3::X, -Vec3::from_array(ORACLE_LIGHT_DIRECTION)),
            )
        } else {
            crate::systems::LightGeometry::default()
        };
        let mut light = LightSource::from_legacy_world_units(
            8.0,
            [1.0; 3],
            0,
            1.0,
            geometry.kind,
            geometry.direction,
            geometry.outer_angle,
            0,
        );
        // A punctual source gives a deterministic analytic attenuation.
        light.emitter.source_radius = byroredux_core::lighting::Meters::ZERO;
        world.insert(entity, light);
    }

    log::info!(
        "Cornell oracle {} ready: blocker={}, volumetric={}, material_probes={}, debug={}, world_offset={:?}, \
         expected unshadowed direct={:?}, linear tolerance={:.4}",
        manifest.name,
        manifest.blocker,
        manifest.volumetric_probe,
        manifest.material_probes,
        manifest.primary_debug_view,
        world_offset,
        expected_unshadowed,
        manifest.max_linear_error,
    );
    (
        manifest.camera_position + world_offset,
        manifest.camera_target + world_offset,
    )
}

/// `l1-skinned-shared` — two skinned receivers that share ONE source mesh
/// through the NPC loader's acquire/register flow (`MeshBuilder::
/// upload_shared_pair`), independently posed (net identity vs net +45°
/// yaw), plus an opaque blocker centred on the light ray through the
/// second receiver's face so only it is shadowed.
///
/// Expected analytic values (albedo 1, unit radiance, IOR 1 / no
/// specular): receiver A, N=(0,0,1), N·L = 2/√6 — the same value as the
/// single-actor L1 gates; receiver B, N=(sin45°, 0, cos45°), N·L = √3/2.
/// A leaked pose between the pair (palette or slot aliasing) reproduces
/// A's value on B, and a shared/merged shadow state would darken A's
/// unobstructed control field.
fn setup_shared_skin_pair_scene(
    world: &mut World,
    ctx: &mut VulkanContext,
    world_offset: Vec3,
    manifest: &CornellOracleManifest,
) -> (Vec3, Vec3) {
    let neutral = TextureHandle(ctx.texture_registry.neutral_fallback());
    let mut builder = MeshBuilder::new(ctx);
    let (vertices, indices) = oracle_skinned_box_vertices([2.0, 4.0, 0.05]);
    let (mesh_a, mesh_b) = builder.upload_shared_pair(&vertices, &indices);
    let mut oracle_matte = matte([1.0; 3]);
    oracle_matte.ior = 1.0;
    oracle_matte.specular_strength = 0.0;

    // Receiver A: bone -90° yaw cancels the +90° bind pose → net identity.
    let receiver_a = spawn_object(
        world,
        mesh_a,
        neutral,
        Vec3::new(-3.0, 4.0, 0.0) + world_offset,
        Quat::IDENTITY,
        oracle_matte.clone(),
        "oracle_shared_receiver_a",
    );
    attach_oracle_skin_with_pose(
        world,
        receiver_a,
        Vec3::new(-3.0, 4.0, 0.0) + world_offset,
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
    );

    // Receiver B: same source handle, bone -45° yaw → net +45° yaw.
    let receiver_b = spawn_object(
        world,
        mesh_b,
        neutral,
        Vec3::new(3.0, 4.0, 0.0) + world_offset,
        Quat::IDENTITY,
        oracle_matte.clone(),
        "oracle_shared_receiver_b",
    );
    attach_oracle_skin_with_pose(
        world,
        receiver_b,
        Vec3::new(3.0, 4.0, 0.0) + world_offset,
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_4),
    );

    // Blocker centred on the light ray through B's face centre: only B is
    // shadowed; receiver A's field stays an unobstructed control.
    let blocker_mesh = builder.box_mesh([0.75; 3]);
    spawn_object(
        world,
        blocker_mesh,
        neutral,
        Vec3::new(3.0, 4.0, 0.0) + world_offset
            + Vec3::from_array(manifest.direction_toward_source) * 2.5,
        Quat::IDENTITY,
        oracle_matte,
        "oracle_shared_blocker",
    );
    builder.finish();

    log::info!(
        "Cornell oracle {} ready: two posed receivers share one source mesh; \
         blocker shadows only receiver B",
        manifest.name,
    );
    (
        manifest.camera_position + world_offset,
        manifest.camera_target + world_offset,
    )
}

/// Inverse-pose geometry so a -90° bone yaw restores the L1/L2 world geometry.
/// Unlike a root rotation, this forces both raster and RT to consume the skin.
fn oracle_skinned_box(builder: &mut MeshBuilder<'_>, half: [f32; 3]) -> MeshHandle {
    let (vertices, indices) = oracle_skinned_box_vertices(half);
    builder.upload(&vertices, &indices)
}

/// Vertex prep shared by the single-receiver rungs and the shared-source
/// pair: positions/normals/tangents pre-rotated +90° yaw (the inverse of
/// the standard -90° bone pose), every vertex bound to bone 0.
fn oracle_skinned_box_vertices(
    half: [f32; 3],
) -> (Vec<byroredux_renderer::Vertex>, Vec<u32>) {
    let (mut vertices, indices) = box_vertices_colored(half, [1.0; 3]);
    let inverse_pose = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    for vertex in &mut vertices {
        vertex.position = (inverse_pose * Vec3::from(vertex.position)).to_array();
        vertex.normal = (inverse_pose * Vec3::from(vertex.normal)).to_array();
        let tangent =
            inverse_pose * Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
        vertex.tangent[..3].copy_from_slice(&tangent.to_array());
        vertex.bone_indices = [0; 4];
        vertex.bone_weights = [1.0, 0.0, 0.0, 0.0];
    }
    (vertices, indices)
}

fn attach_oracle_skin(world: &mut World, entity: byroredux_core::ecs::EntityId, position: Vec3) {
    attach_oracle_skin_with_pose(
        world,
        entity,
        position,
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
    );
}

/// Single-bone skin binding with an explicit bone pose. The standard rungs
/// cancel the bind pose's +90° yaw with -90°; the shared-source pair also
/// uses -45° so the second receiver keeps a visibly different orientation.
fn attach_oracle_skin_with_pose(
    world: &mut World,
    entity: byroredux_core::ecs::EntityId,
    position: Vec3,
    bone_rotation: Quat,
) {
    use byroredux_core::ecs::{RenderLayer, SkinnedMesh};
    let bone = world.spawn();
    world.insert(bone, Transform::new(position, bone_rotation, 1.0));
    world.insert(bone, GlobalTransform::new(position, bone_rotation, 1.0));
    world.insert(
        entity,
        SkinnedMesh {
            skeleton_root: Some(bone),
            bones: vec![Some(bone)],
            bind_inverses: vec![byroredux_core::math::Mat4::IDENTITY],
            global_skin_transform: byroredux_core::math::Mat4::IDENTITY,
        },
    );
    world.insert(entity, RenderLayer::Actor);
}
