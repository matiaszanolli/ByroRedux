//! Per-sub-mesh instance spawning: resolve the mesh/material/texture paths,
//! upload (or reuse a cached) GPU mesh, and stamp the render + physics +
//! bounds components onto the placement child entity.
//!
//! Split out of `spawn.rs` (#2410 / TD1-007), which had crossed 2000 LOC on
//! production code alone — `spawn_mesh_instance` was 546 LOC of it. Contents
//! moved verbatim; only the visibility of the items `spawn.rs` still calls
//! was widened.

use super::*;

/// Effective per-mesh texture-slot paths, resolved in one StringPool
/// lock (#882). Promoted to module scope from `spawn_placed_instances`
/// so `resolve_mesh_paths` + `spawn_mesh_instance` can share it (#2057).
pub(super) struct ResolvedMeshPaths {
    textures: byroredux_nif::import::MaterialTextureSet<Option<String>>,
    material_path: Option<String>,
    name_sym: Option<byroredux_core::string::FixedString>,
}
fn resolve_to_owned(
    pool: &byroredux_core::string::StringPool,
    sym: Option<byroredux_core::string::FixedString>,
) -> Option<String> {
    sym.and_then(|s| pool.resolve(s)).map(|s| s.to_string())
}

/// Resolve every mesh's effective texture-slot paths + interned name
/// under a single StringPool lock (#882). Split out of
/// `spawn_placed_instances` (#2057).
pub(super) fn resolve_mesh_paths(
    world: &mut World,
    imported: &[byroredux_nif::import::ImportedMesh],
    refr_overlay: Option<&RefrTextureOverlay>,
) -> Vec<ResolvedMeshPaths> {
    let ov = refr_overlay;
    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
    imported
        .iter()
        .map(|mesh| {
            let mut textures = mesh
                .material
                .textures
                .map_ref(|path| resolve_to_owned(&pool, *path));
            // Effective texture slot paths. REFR overlay
            // (XATO/XTNM/XTXR) wins over the NIF-authored paths
            // when present; for slots the overlay left empty the
            // cached NIF's texture rides through. `None` on both
            // sides means the slot has no texture. See #584.
            textures.base_color = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.diffuse)
                    .or(mesh.material.textures.base_color),
            );
            // Oblivion/FO3 ship normal maps via the `<base>_n.dds`
            // load-time convention, not an explicit NIF slot. When the
            // mesh left both normal/bump slots empty, derive the sibling
            // from the (effective) diffuse path; it resolves like any
            // texture and fails soft if absent (#1303 / OBL-D4-NEW-01).
            textures.normal = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.normal).or(mesh.material.textures.normal),
            )
            .or_else(|| textures.base_color.as_deref().map(derive_normal_map_path));
            textures.emissive = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.glow).or(mesh.material.textures.emissive),
            );
            let effective_model_space_normals =
                mesh.material.model_space_normals || ov.is_some_and(|o| o.model_space_normals);
            if effective_model_space_normals {
                // Slot 7 changes role on model-space-normal materials: it is
                // alternate specular intensity/colour, not smoothness. Keep
                // the REFR override in the canonical standalone-specular
                // lane so it cannot change roughness downstream.
                textures.specular = resolve_to_owned(
                    &pool,
                    ov.and_then(|o| o.specular)
                        .or(mesh.material.textures.specular),
                );
            } else {
                textures.smooth_spec = resolve_to_owned(
                    &pool,
                    ov.and_then(|o| o.specular)
                        .or(mesh.material.textures.smooth_spec),
                );
            }
            textures.height = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.height).or(mesh.material.textures.height),
            );
            textures.environment = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.env)
                    .or(mesh.material.textures.environment),
            );
            textures.environment_mask = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.env_mask)
                    .or(mesh.material.textures.environment_mask),
            );
            textures.wrinkle = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.wrinkle)
                    .or(mesh.material.textures.wrinkle),
            );
            let material_path = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.material_path)
                    .or(mesh.material.material_path),
            );
            // Intern the mesh name in the same lock — see #882's
            // second hotspot. `mesh.name: Option<Arc<str>>`. The
            // `pool.intern` call must follow the resolves so the
            // `&pool` borrows from `resolve_to_owned` end before
            // the `&mut pool` re-borrow.
            let name_sym = mesh.name.as_deref().map(|n| pool.intern(n));
            ResolvedMeshPaths {
                textures,
                material_path,
                name_sym,
            }
        })
        .collect()
    // pool guard dropped here at end of block.
}

/// Immutable per-placement context shared by `spawn_mesh_instance` —
/// bundles the REFR transform + render/overlay inputs so the helper's
/// signature stays legible. Split out of `spawn_placed_instances`
/// (#2057). All fields are `Copy`.
#[derive(Clone, Copy)]
pub(super) struct PlacementCtx<'a> {
    pub(super) tex_provider: &'a TextureProvider,
    pub(super) ref_pos: Vec3,
    pub(super) ref_rot: Quat,
    pub(super) ref_scale: f32,
    pub(super) base_layer: byroredux_core::ecs::components::RenderLayer,
    pub(super) mesh_cache_key: Option<&'a str>,
    pub(super) refr_overlay: Option<&'a RefrTextureOverlay>,
    pub(super) light_data: Option<&'a esm::cell::LightData>,
    pub(super) light_animation_flags: u32,
    pub(super) light_shadow_flags: u32,
    // #2439 (NIFAL-D2-01) — see `spawn_placed_instances`'s matching params.
    pub(super) light_kind: byroredux_core::ecs::LightKind,
    pub(super) light_direction: [f32; 3],
    pub(super) light_outer_angle: f32,
    pub(super) placement_root: byroredux_core::ecs::EntityId,
    pub(super) collision_fallback: MissingCollisionFallback,
    pub(super) spawned_nif_lights: usize,
}

/// Replace one authored alpha-over fog/smoke mesh with an analytic local
/// medium before any texture upload, raster entity, or BLAS work occurs.
pub(super) fn spawn_fog_mesh_instance(
    world: &mut World,
    pc: &PlacementCtx,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
) -> bool {
    use byroredux_core::ecs::{Name, Parent};

    let texture_path = paths.textures.base_color.as_deref();
    let fog_semantics = pc.mesh_cache_key.is_some_and(crate::fog::has_fog_token)
        || texture_path.is_some_and(crate::fog::has_fog_token)
        || mesh.name.as_deref().is_some_and(crate::fog::has_fog_token);
    let Some(fog_volume) = crate::fog::fog_volume_from_mesh(pc.mesh_cache_key, texture_path, mesh)
    else {
        if fog_semantics {
            log::debug!(
                target: "byroredux::fog",
                "authored fog mesh candidate kept on legacy path: model={:?} texture={:?} \
                 name={:?} has_alpha={} dst_blend={} material_kind={}",
                pc.mesh_cache_key,
                texture_path,
                mesh.name,
                mesh.material.has_alpha,
                mesh.material.dst_blend_mode,
                mesh.material.material_kind,
            );
        }
        return false;
    };
    log::debug!(
        target: "byroredux::fog",
        "replaced authored fog mesh with local volume: model={:?} texture={:?} name={:?}",
        pc.mesh_cache_key,
        texture_path,
        mesh.name,
    );

    let nif_rotation = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_position = Vec3::from_array(mesh.translation);
    let (world_position, world_rotation, world_scale) = GlobalTransform::compose_trs(
        pc.ref_pos,
        pc.ref_rot,
        pc.ref_scale,
        nif_position,
        nif_rotation,
        mesh.scale,
    );

    let entity = world.spawn();
    world.insert(
        entity,
        Transform::new(nif_position, nif_rotation, mesh.scale),
    );
    world.insert(
        entity,
        GlobalTransform::new(world_position, world_rotation, world_scale),
    );
    world.insert(entity, fog_volume);
    world.insert(entity, Parent(pc.placement_root));
    crate::helpers::add_child(world, pc.placement_root, entity);
    if let Some(symbol) = paths.name_sym {
        world.insert(entity, Name(symbol));
    }
    true
}

/// Spawn the render entity (+ optional physics ghost + ESM light
/// fallback) for one imported sub-mesh. Returns `true` when an entity
/// was spawned (the caller then increments its placement count); a
/// failed GPU upload returns `false`, mirroring the pre-split `continue`.
/// Split out of `spawn_placed_instances` (#2057).
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_mesh_instance(
    world: &mut World,
    ctx: &mut VulkanContext,
    pc: &PlacementCtx,
    cached: &CachedNifImport,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
    sub_mesh_index: usize,
    count: usize,
    blas_specs: &mut Vec<(u32, u32, u32)>,
    synthesized_collision_proxy: &mut bool,
) -> bool {
    use byroredux_core::ecs::{Name, Parent};
    use byroredux_renderer::Vertex;
    let PlacementCtx {
        tex_provider,
        ref_pos,
        ref_rot,
        ref_scale,
        base_layer,
        mesh_cache_key,
        refr_overlay,
        light_data,
        light_animation_flags,
        light_shadow_flags,
        light_kind,
        light_direction,
        light_outer_angle,
        placement_root,
        collision_fallback,
        spawned_nif_lights,
    } = *pc;
    let num_verts = mesh.positions.len();
    let sub_mesh_index_u32 = sub_mesh_index as u32;

    // #879 / CELL-PERF-01 — refcounted GPU mesh dedup. First
    // placement of `chair.nif` uploads the vertex/index pair and
    // registers it under `(model_path, sub_mesh_index)`; the next
    // 39 chair placements bump the entry's refcount and reuse
    // the same `mesh_handle` (and the same BLAS — skipping the
    // batched BLAS build entry for the cached hit). Without
    // `mesh_cache_key` (terrain / single-NIF CLI view) the cache
    // is bypassed and we keep the legacy fresh-upload-per-call
    // shape.
    let cache_hit_handle =
        mesh_cache_key.and_then(|key| ctx.mesh_registry.acquire_cached(key, sub_mesh_index_u32));

    let mesh_handle = if let Some(handle) = cache_hit_handle {
        // Cached: skip the CPU vertex-build, the GPU upload, AND
        // the BLAS batch entry. The existing BLAS for this handle
        // is already attached to live placements in earlier cells
        // (or earlier in this same cell).
        handle
    } else {
        // #2410 / TD1-007 — the per-attribute fallback assembly the issue
        // asks to extract already exists as `imported_mesh_to_vertices`
        // (`lod_support.rs`, serving the object- and placement-LOD paths).
        // The inline copy here was semantically identical — same colour /
        // normal / UV defaults, same authored-tangent copy — so this reuses
        // it rather than adding a second spelling of the same logic.
        let vertices: Vec<Vertex> = super::super::lod_support::imported_mesh_to_vertices(mesh);

        let alloc = ctx.allocator.as_ref().unwrap();
        let upload_ctx = GpuUploadCtx {
            device: &ctx.device,
            allocator: alloc,
            queue: &ctx.graphics_queue,
            command_pool: ctx.transfer_pool,
        };
        // Effect surfaces need a BLAS even though they must not cast opaque
        // shadows. Authored glass assemblies put emissive BSEffectShader
        // layers behind their outer shell (Skyrim's alchemy workbench is the
        // regression fixture); reflection/refraction rays must see those
        // layers. TLAS visibility masks put them in VISIBILITY_LAYER_EFFECT,
        // which shadow traversal excludes while optical/GI rays may include.
        let for_rt = ctx.device_caps.ray_query_supported
            && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
            && !mesh.material.is_decal;
        let upload_result = match mesh_cache_key {
            Some(key) => ctx.mesh_registry.register_scene_mesh_keyed(
                upload_ctx,
                &vertices,
                &mesh.indices,
                for_rt,
                None,
                (key, sub_mesh_index_u32),
            ),
            None => ctx.mesh_registry.upload_scene_mesh(
                upload_ctx,
                &vertices,
                &mesh.indices,
                for_rt,
                None,
            ),
        };
        let handle = match upload_result {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Failed to upload mesh: {}", e);
                return false;
            }
        };

        // Fresh ray-visible surface upload — this handle needs a BLAS. Subsequent
        // cache hits for the same `(path, sub_mesh_index)` reuse
        // this BLAS entry without re-submitting.
        if for_rt {
            blas_specs.push((handle, num_verts as u32, mesh.indices.len() as u32));
        }
        handle
    };

    // Pre-resolved texture slot paths from the single-lock
    // pre-pass above (#882). Cloned per-mesh because the Material
    // ECS component owns its `Option<String>` fields and the
    // resolved-paths Vec stays alive across this iteration; the
    // alternative — moving paths out of `resolved_paths[i]` — would
    // need a swap-with-default to keep the Vec indexable for the
    // texture-handle resolves below. Per-slot clone is one
    // allocation per populated slot per mesh, same as the pre-fix
    // `resolve_owned(...).clone()` pattern at the Material struct
    // construction site.
    let eff_textures = paths.textures.clone();
    let eff_texture_path = eff_textures.base_color.clone();
    let eff_material_path = paths.material_path.clone();

    // Load texture (shared resolve: cache → BSA → fallback).
    // #610 — pass the diffuse-slot `TexClampMode` so the bindless
    // descriptor's sampler picks the matching `VkSamplerAddressMode`
    // pair. CLAMP-authored decals / scope reticles / Oblivion
    // architecture trim no longer render with the legacy
    // REPEAT/REPEAT bleed.
    let tex_handle = resolve_texture_with_clamp(
        ctx,
        tex_provider,
        eff_texture_path.as_deref(),
        mesh.material.texture_clamp_mode,
    );

    // #544 — mesh entities now sit in the NIF-local frame and
    // descend from the placement root. The transform-propagation
    // system composes `placement_root` (the REFR transform) onto
    // them each frame to produce the world-space `GlobalTransform`
    // the renderer / BLAS / lighting consume. Pre-#544 every mesh
    // pre-baked the REFR composition into its own `Transform`,
    // which left it anchored to nothing the embedded animation
    // clip could walk to.
    //
    // The composed `final_*` values are still computed up front
    // because the `GlobalTransform` we seed on the mesh has to
    // match what the propagation pass will compute on the first
    // tick — anything that reads `GlobalTransform` before then
    // (renderer's per-frame data collection, BLAS build below)
    // gets a correctly-placed value in the meantime.
    let nif_quat = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_pos = Vec3::new(
        mesh.translation[0],
        mesh.translation[1],
        mesh.translation[2],
    );

    // World-space placement — used only to seed the initial
    // `GlobalTransform`. `Transform` itself stays NIF-local so
    // the propagation pass produces the same value next tick. The
    // parent→child composition order lives in `compose_trs`.
    let (final_pos, final_rot, final_scale) =
        GlobalTransform::compose_trs(ref_pos, ref_rot, ref_scale, nif_pos, nif_quat, mesh.scale);

    // Diagnostic: log meshes with significant NIF-internal offsets
    // (these are wall/structural pieces most likely to show positioning issues)
    let nif_offset_len = nif_pos.length();
    if nif_offset_len > 50.0 {
        log::debug!(
            "  NIF offset {:.0} for mesh {:?}: nif_pos=({:.0},{:.0},{:.0}) \
                 final=({:.0},{:.0},{:.0})",
            nif_offset_len,
            mesh.name,
            nif_pos.x,
            nif_pos.y,
            nif_pos.z,
            final_pos.x,
            final_pos.y,
            final_pos.z,
        );
    }

    let entity = world.spawn();
    // NIF-local Transform for hierarchy propagation; world-space
    // GlobalTransform for first-tick consumers. See #544.
    world.insert(entity, Transform::new(nif_pos, nif_quat, mesh.scale));
    world.insert(
        entity,
        GlobalTransform::new(final_pos, final_rot, final_scale),
    );
    // #1213 / D1-NEW-02 — seed LocalBound from the mesh-local
    // bounding sphere (`ImportedMesh.local_bound_center`,
    // `.local_bound_radius`, both extracted by the NIF importer
    // from `NiTriShapeData.center` / `BsTriShape.center` or
    // computed from vertex positions). The bounds-propagation
    // system at `byroredux/src/systems/bounds.rs:43-66` reads
    // this row and produces a world-space `WorldBound` each
    // frame; pre-#1213 no LocalBound row was ever inserted, so
    // every WorldBound stayed at the component default (zero
    // sphere) and downstream culling / RT-budget / cell-bounds
    // consumers fell through to coarser approximations.
    world.insert(
        entity,
        LocalBound::new(
            Vec3::new(
                mesh.local_bound_center[0],
                mesh.local_bound_center[1],
                mesh.local_bound_center[2],
            ),
            mesh.local_bound_radius,
        ),
    );
    // Sibling to the LocalBound insert above. `bounds.rs` Pass 1 at
    // line 61-63 only *updates* a pre-existing `WorldBound` row —
    // it does not insert one — so a missing seed row means the
    // entity stays at `WorldBound::default()` (zero sphere) and is
    // invisible to ray-cast picking, frustum culling, and the
    // skinned-LRU bounds heuristic. The propagation pass overwrites
    // this `ZERO` with the real value on the next tick.
    world.insert(entity, WorldBound::ZERO);
    // #1235 / LC-D1-NEW-01 — attach SceneFlags for parity with the
    // loose-NIF loader (`scene/nif_loader.rs:789-791`). APP_CULLED
    // shapes never reach this point (filtered import-side in
    // `walk/mod.rs`); the remaining NiAVObject bits ride through
    // for downstream consumers.
    if mesh.flags != 0 {
        world.insert(entity, SceneFlags::from_nif(mesh.flags));
    }
    // #2206 / NIFAL-D4-02 — per-mesh parity with the loose-NIF loader
    // (`scene/nif_loader.rs`'s `node.billboard_mode` attach). The flat
    // cell-loader walk spawns one entity per mesh with no node entities
    // at all, so the nearest-ancestor `NiBillboardNode` mode set by
    // `walk_node_flat` rides on the mesh itself instead of a node.
    if let Some(raw) = mesh.billboard_mode {
        world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
    }
    // Parent/Children edge → embedded animation clip's subtree
    // walk discovers this mesh through `placement_root`.
    world.insert(entity, Parent(placement_root));
    crate::helpers::add_child(world, placement_root, entity);
    // Name from `ImportedMesh.name` so the clip's node-keyed
    // channels (`FixedString` interned at parse time, #340)
    // resolve through `build_subtree_name_map` to this entity.
    // Pre-#544 the cell-loader path skipped this insert, so even
    // if `Parent` had been wired the channels would have failed
    // their name lookup and silently no-op'd.
    //
    // Pre-#882 this site re-acquired a `world.resource_mut::<
    // StringPool>()` write lock per mesh. The intern is now done
    // in the pre-pass above; this site only consumes the cached
    // `FixedString`.
    if let Some(sym) = paths.name_sym {
        world.insert(entity, Name(sym));
    }
    world.insert(entity, MeshHandle(mesh_handle));
    world.insert(entity, TextureHandle(tex_handle));
    // Canonical material translation — the single boundary that
    // resolves a raw `ImportedMesh` into the engine `Material`
    // (PBR resolved, glass classified once, flag union packed).
    // The cell path contributes the REFR-overlay model-space-normals
    // bit as `extra_material_flags`; everything else is shared with
    // the loose-NIF path. See `material_translate.rs`.
    let extra_material_flags = refr_overlay
        .filter(|o| o.model_space_normals)
        .map(|_| byroredux_renderer::vulkan::material::material_flag::MODEL_SPACE_NORMALS)
        .unwrap_or(0);
    let material = crate::material_translate::translate_material(
        &mesh.material,
        mesh.name.as_deref(),
        crate::material_translate::ResolvedPaths {
            textures: eff_textures.clone(),
            material_path: eff_material_path.clone(),
        },
        extra_material_flags,
    );
    let material_kind = material.material_kind;
    world.insert(entity, material);
    // PERF-D3-NEW-02 / #1136 — classify FX-decoration meshes at spawn
    // time so build_render_data can skip them via a component query
    // instead of running 6 substring scans per draw per frame.
    if let Some(ref tp) = eff_texture_path {
        if texture_path_is_fx_mesh(tp, material_kind) {
            world.insert(entity, IsFxMesh);
        }
    }
    // Resolve every secondary semantic role with the SAME authored clamp mode
    // as base colour. The shared helper is also used by loose-NIF spawning so
    // structures, clutter, actors, and exterior statics cannot drift.
    let texture_handles = resolve_material_texture_handles_with_clamp(
        ctx,
        tex_provider,
        &eff_textures,
        tex_handle,
        mesh.material.texture_clamp_mode,
    );
    let normal_has_alpha = texture_handles.normal != 0
        && ctx
            .texture_registry
            .handle_has_alpha(texture_handles.normal);
    world.insert(
        entity,
        MaterialTextureHandles {
            textures: texture_handles,
            normal_has_alpha,
            parallax_height_scale: mesh.material.parallax_height_scale.unwrap_or(0.04),
            parallax_max_passes: mesh.material.parallax_max_passes.unwrap_or(4.0),
        },
    );
    // #1480 / REN-D22-NEW-01 — resolve the normal-alpha-as-spec roughness
    // ONCE into the canonical Material now that MaterialTextureHandles is
    // attached, instead of recomputing it per
    // draw in the render path. Reads the same components the renderer
    // reads, so the value is identical — only canonical + tooling-visible.
    crate::material_translate::resolve_normal_alpha_spec_roughness(world, entity);
    let implicit_decal_blend = decal_uses_implicit_alpha_blend(
        mesh.material.is_decal,
        mesh.material.has_alpha,
        mesh.material.alpha_test,
        mesh.material.alpha_threshold,
    );
    if mesh.material.has_alpha || implicit_decal_blend {
        // FO4's dedicated decal pass composites texture alpha even when the
        // BGSM generic blend function is `None` for low-threshold soft
        // decals. Preserve explicit factors; otherwise use alpha-over.
        let (src_blend, dst_blend) = if mesh.material.has_alpha {
            (mesh.material.src_blend_mode, mesh.material.dst_blend_mode)
        } else {
            (6, 7)
        };
        world.insert(
            entity,
            AlphaBlend {
                src_blend,
                dst_blend,
            },
        );
    }
    if mesh.material.is_decal {
        world.insert(entity, IsDecalMesh);
    }
    if mesh.material.two_sided {
        world.insert(entity, TwoSided);
    }
    // #renderlayer — derive the per-entity content-class layer.
    // Base layer comes from the REFR's record type
    // (`stat.record_type.render_layer()`); the per-mesh
    // `mesh.material.is_decal` (NIF-flagged decals — blood splats, scorch
    // marks) and `mesh.material.alpha_test_func != 0` (alpha-tested rugs /
    // posters / fences / cutout foliage) escalate to
    // [`RenderLayer::Decal`] regardless of the base, so any
    // coplanar overlay wins its z-fight against the surface
    // beneath. Architecture (zero bias) is the safe default for
    // the rare "neither base nor mesh hints decal" path.
    //
    // Pre-#renderlayer this site also inserted a `Decal` marker
    // component when `mesh.material.is_decal` — that marker is retired now
    // that `RenderLayer::Decal` carries the same signal end-to-end.
    {
        use byroredux_core::ecs::components::{
            escalate_small_static_to_clutter, render_layer_with_decal_escalation,
        };
        // Small-STAT escalation runs first so decorative clutter
        // authored as STAT (paper piles, folders, clipboards on
        // desks — Bethesda's record-type classifier can't tell
        // these from architectural STATs without spatial extent)
        // gets the Clutter bias before the decal gate sees it.
        // Decal escalation still wins for alpha-tested overlays
        // and NIF-flagged decals regardless of size.
        //
        // The post-escalation layer is the RENDER-z-bias signal
        // (`RenderLayer` ECS component). #1294 moved the collision
        // trimesh-fallback gate below off this post-escalation
        // layer onto `base_layer` so SF sub-decomposed architecture
        // (per-LOD per-material sub-meshes < 50 units each, but
        // composing into a 1000-unit wall) doesn't get its
        // collider stripped on a render-side optimization.
        let layer =
            escalate_small_static_to_clutter(base_layer, mesh.local_bound_radius * ref_scale);
        let layer = render_layer_with_decal_escalation(
            layer,
            mesh.material.is_decal,
            mesh.material.alpha_test,
        );
        world.insert(entity, layer);
    }

    // F3 (2026-05-27) — synthesize a static TriMesh collider from
    // the render geometry when the NIF authored NO bhk collision.
    // This is the FO4+ case: those games moved static architecture
    // collision into the Havok content-system blob
    // (`bhkNPCollisionObject` → `bhkPhysicsSystem`), which our
    // `extract_collision` doesn't deserialize yet (a multi-day
    // project — see docs/audits/FALLOUT_SYMPTOMS F3). Without any
    // static collider the M28.5 character controller has nothing
    // to ground against and the player falls through the floor.
    //
    // The render mesh is a coarse but serviceable stand-in for the
    // authored collision hull on structural architecture (floors,
    // walls, ramps). Gated tightly so we don't turn clutter, decals,
    // or skinned actors into expensive trimesh colliders:
    //   - `collisions_empty` — the NIF gave us no bhk shape, so
    //     we're not double-covering FNV/FO3/Skyrim (which parse bhk).
    //   - `RenderLayer::Architecture` — structural only; clutter and
    //     decals are escalated away from this layer above.
    //   - `!mesh.skinned` — never synthesize for animated bodies.
    //   - `!mesh.material.is_decal && !mesh.material.alpha_test` — skip overlay planes.
    //   - ≥ 1 triangle of geometry.
    // Scale: the physics sync places bodies by GlobalTransform
    // translation+rotation only (it ignores scale — bhk shapes bake
    // havok_scale into their verts at extract time). So we bake the
    // composed `final_scale` into the trimesh verts here to match
    // the rendered geometry.
    // #1294 — gate on `base_layer` (pre-escalation REFR record-type
    // classification), NOT `final_layer` (post-escalation render
    // layer). The small-STAT-to-Clutter escalation
    // (`escalate_small_static_to_clutter`) is a RENDER z-bias
    // optimization that demotes architecturally-classified meshes
    // with a small bounding-sphere radius (< 50 units) to the
    // Clutter render layer so decorative STATs (papers / folders
    // / clipboards) win the coplanar z-fight against desks. It was
    // never intended to gate collision generation.
    //
    // For Starfield content the gate-on-`final_layer` site rejected
    // every wall / floor / ramp on Cydonia because SF NIFs are
    // heavily decomposed into per-material per-LOD sub-meshes
    // (an industrial platform = 6 BSGeometry blocks each with 4
    // LOD slots = 24 sub-meshes), each individual sub-mesh smaller
    // than the 50-unit threshold. Even though the COMPOSITE REFR
    // is a giant wall, the per-mesh radius escalates to Clutter
    // and the trimesh fallback skips it → zero static colliders →
    // character free-falls indefinitely from frame 0 (`rapier_bodies=1`
    // diagnostic warn at `character.rs:290`).
    //
    // `base_layer` reflects the REFR's base record type
    // (STAT/MSTT/FURN/DOOR/… → Architecture; NPC_ → Actor; etc.).
    // That's the correct "should this be a static collider?" signal
    // — independent of per-mesh sub-decomposition. NPC actors (Actor
    // base) and small clutter (record-type-classified Clutter) both
    // skip the fallback as before; only the misclassified
    // sub-decomposed architecture changes behaviour.
    if collision_fallback == MissingCollisionFallback::ArchitectureTriMesh
        && mesh.skin.is_none()
        && !mesh.material.is_decal
        && !mesh.material.alpha_test
        && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
        && mesh.positions.len() >= 3
        && mesh.indices.len() >= 3
    {
        // Shared with the exterior LAND path — see
        // `spawn_trimesh_collider_ghost`. The render `entity` keeps its
        // MeshHandle and enters BLAS+TLAS normally (RT shadows/GI on
        // FO4/Starfield architecture); the ghost is physics-only.
        let source_form = world
            .get::<FormIdComponent>(placement_root)
            .map(|form_id| form_id.0);
        *synthesized_collision_proxy |= spawn_trimesh_collider_ghost(
            world,
            &mesh.positions,
            &mesh.indices,
            final_pos,
            final_rot,
            final_scale,
            source_form,
        );
    }
    // Attach ESM light_data ONLY if the NIF didn't actually spawn
    // any lights (avoids duplicates) and only on the first mesh
    // (avoids N copies when a lamp NIF has multiple sub-meshes).
    //
    // Pre-#632 this gated on `nif_lights.is_empty()` — wrong
    // because zero-colour placeholders take a slot in the array
    // but get filtered out at the spawn loop above. Cells with
    // light-bulb meshes (Prospector Saloon) rendered dark even
    // though both the NIF placeholder and the ESM LIGH record
    // agreed there should be a light. Track real spawns instead.
    if let Some(ld) = light_data {
        if spawned_nif_lights == 0 && count == 0 {
            // Phase 18 (reverted in Phase 19.7) — the
            // flame-node offset spawn lived here, but the
            // substring-based pattern match
            // (`flame` / `fire` / `attachlight`) hit
            // false-positives on at least one Skyrim candle
            // NIF (upper shelf in Sleeping Giant Inn — visible
            // as "no light emitted at all" from that REFR's
            // light placement). Restoring the pre-Phase-18
            // attach-to-mesh-entity-at-ref_pos behaviour.
            //
            // The Phase 18 *capture* path stays — every cached
            // NIF still records `flame_attach_offset` at parse
            // time. A future re-enable with tighter pattern
            // matching (e.g. `^Flame[0-9]+$` regex, or
            // requiring an `AttachFire` block specifically)
            // can consume the captured offset without
            // re-walking the NIF.
            let _ = cached.flame_attach_offset;

            world.insert(
                entity,
                LightSource::from_legacy_world_units(
                    light_radius_or_default(ld.radius),
                    ld.color,
                    ld.flags,
                    ld.falloff_exponent,
                    light_kind,
                    light_direction,
                    light_outer_angle,
                    light_shadow_flags,
                ),
            );
            // Phase 17 — animation companion at the placement root,
            // same position as the mesh entity. The caller has already
            // decoded source-game LIGH flags into shared behavior.
            attach_light_flicker_if_needed(world, entity, ld, ref_pos, light_animation_flags);
        }
    }
    true
}
