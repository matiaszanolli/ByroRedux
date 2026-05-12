//! CLI argument parsing for NIF loading + NIF-bytes-to-ECS pipeline.
//!
//! Hosts the four loose-NIF entry points and their shared `parse_import_and_merge`
//! helper:
//!   * [`load_nif_from_args`] — drives the engine's `--mesh` / `--tree` /
//!     `--bsa` CLI flags. Internal to [`super::setup_scene`].
//!   * [`load_nif_bytes`] — small wrapper used by the scripting harness
//!     and a couple of debug paths; importers parse → merge inline.
//!   * [`load_nif_bytes_with_skeleton`] — the heavyweight import path
//!     used by [`crate::npc_spawn`] to spawn skeleton + body + head
//!     hierarchies for NPCs.

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    Billboard, BillboardMode, GlobalTransform, LocalBound,
    Material, MeshHandle, Name, Parent, ParticleEmitter, SceneFlags, SkinnedMesh, TextureHandle,
    Transform, World, WorldBound, MAX_BONES_PER_MESH,
};
use byroredux_core::math::{Mat4, Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{
    build_material_provider, build_texture_provider, merge_bgsm_into_mesh,
    resolve_texture, MaterialProvider, TextureProvider,
};
use crate::components::{
    AlphaBlend, DarkMapHandle, ExtraTextureMaps, NormalMapHandle, TwoSided,
};
use crate::helpers::add_child;

/// Parse CLI arguments and load NIF data accordingly.
///
/// Supported flags:
///   `cargo run -- path/to/file.nif` — loose NIF file
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif` — extract from BSA
///   `cargo run -- --bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa`
///   `cargo run -- --bsa meshes.bsa --tree trees\joshua01.spt` — direct
///       SpeedTree visualiser (Phase 1.6). Renders the placeholder billboard
///       per the SpeedTree compatibility plan; useful for one-tree
///       reverse-engineering iteration without spinning up a whole cell.
pub(super) fn load_nif_from_args(world: &mut World, ctx: &mut VulkanContext) -> (usize, Option<EntityId>) {
    let args: Vec<String> = std::env::args().collect();

    // Collect BSA/BA2 archives (auto-detects format).
    let tex_provider = build_texture_provider(&args);
    let mut mat_provider = build_material_provider(&args);

    if let Some(bsa_idx) = args.iter().position(|a| a == "--bsa") {
        // BSA mode: --bsa <archive> {--mesh|--tree} <path_in_archive>.
        let bsa_path = match args.get(bsa_idx + 1) {
            Some(p) => p,
            None => {
                log::error!("--bsa requires an archive path");
                return (0, None);
            }
        };
        // `--tree` is shorthand for `--mesh` that documents the
        // user's intent to visualise a SpeedTree binary. The
        // routing inside `parse_import_and_merge` branches on the
        // path's `.spt` extension regardless of which flag was
        // used, so `--mesh foo.spt` works equivalently — `--tree`
        // exists for discoverability via `--help` / docs.
        let asset_path = match args
            .iter()
            .position(|a| a == "--mesh" || a == "--tree")
            .and_then(|i| args.get(i + 1))
        {
            Some(p) => p,
            None => {
                log::error!("--bsa requires --mesh <path> (or --tree <path> for `.spt`)");
                return (0, None);
            }
        };

        let archive = match byroredux_bsa::BsaArchive::open(bsa_path) {
            Ok(a) => a,
            Err(e) => {
                log::error!("Failed to open BSA '{}': {}", bsa_path, e);
                return (0, None);
            }
        };
        let data = match archive.extract(asset_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to extract '{}': {}", asset_path, e);
                return (0, None);
            }
        };
        log::info!("Extracted {} bytes from BSA '{}'", data.len(), asset_path);
        load_nif_bytes(
            world,
            ctx,
            &data,
            asset_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else if let Some(nif_path) = args.get(1) {
        if nif_path.starts_with("--") {
            return (0, None); // Skip flags that aren't NIF paths
        }
        // Loose file mode: <path.nif>
        let data = match std::fs::read(nif_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("Failed to read NIF file '{}': {}", nif_path, e);
                return (0, None);
            }
        };
        load_nif_bytes(
            world,
            ctx,
            &data,
            nif_path,
            &tex_provider,
            Some(&mut mat_provider),
        )
    } else {
        (0, None)
    }
}

/// Parse NIF bytes, import meshes with hierarchy, upload to GPU, and spawn ECS entities.
/// Returns (entity_count, root_entity).
pub(crate) fn load_nif_bytes(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> (usize, Option<EntityId>) {
    let (count, root, _local_map) = load_nif_bytes_with_skeleton(
        world,
        ctx,
        data,
        label,
        tex_provider,
        mat_provider,
        None,
        None,
    );
    (count, root)
}

/// Parse + import + BGSM-merge a NIF scene from raw bytes. Shared
/// helper for [`load_nif_bytes_with_skeleton`]'s cache-miss path
/// (where the result is wrapped in `Arc` and inserted into
/// [`crate::scene_import_cache::SceneImportCache`]) and its
/// hook-bypass path (where the per-NPC `pre_spawn_hook` then mutates
/// the result before spawn). Returns `None` on parse failure so the
/// caller can record a negative cache entry. See #880 / CELL-PERF-02.
///
/// Branches on `label`'s extension to route SpeedTree `.spt` bytes
/// through `byroredux_spt::parse_spt + import_spt_scene` instead of
/// the NIF parser. This is the loose-file / `--tree` direct-visualiser
/// path; cell loader REFRs go through `cell_loader::parse_and_import_spt`
/// which can also pull TREE record metadata for sizing + texture
/// override.
pub(super) fn parse_import_and_merge(
    world: &mut World,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> Option<byroredux_nif::import::ImportedScene> {
    let is_spt = label
        .rsplit('.')
        .next()
        .map(|ext| ext.eq_ignore_ascii_case("spt"))
        .unwrap_or(false);
    if is_spt {
        let scene = match byroredux_spt::parse_spt(data) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to parse SPT '{}': {}", label, e);
                return None;
            }
        };
        let mut pool = world.resource_mut::<StringPool>();
        // Direct-visualiser path has no TREE record context — the
        // importer's default params produce a 256×512 placeholder
        // textured with whatever leaf path the `.spt` itself
        // authored (tag 4003). Cell-loader REFRs hit the parallel
        // `cell_loader::parse_and_import_spt` path which threads
        // the TREE record's ICON / OBND through.
        let imported = byroredux_spt::import_spt_scene(
            &scene,
            &byroredux_spt::SptImportParams::default(),
            &mut pool,
        );
        // BGSM merge doesn't apply — `.spt` doesn't carry BGSM/BGEM
        // material refs. Drop the mat_provider unused for this path.
        let _ = mat_provider;
        return Some(imported);
    }
    let scene = match byroredux_nif::parse_nif(data) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to parse NIF '{}': {}", label, e);
            return None;
        }
    };
    let mut pool = world.resource_mut::<StringPool>();
    let mut imported = byroredux_nif::import::import_nif_scene_with_resolver(
        &scene,
        &mut pool,
        Some(tex_provider),
    );
    // FO4+ external material resolution (#493). NIF fields take
    // precedence; only empty slots fill in from the resolved
    // BGSM/BGEM chain. The merge interns through the same pool so
    // REFR overlays and per-mesh imports share the dedup table (#609).
    if let Some(provider) = mat_provider {
        for mesh in &mut imported.meshes {
            merge_bgsm_into_mesh(mesh, provider, &mut pool);
        }
    }
    Some(imported)
}

/// Variant of [`load_nif_bytes`] used by NPC spawn (M41.0 Phase 1b)
/// when assembling skeleton + body + head from three separate NIFs.
///
/// `external_skeleton`: when `Some(map)`, every skinning-bone name
/// lookup tries the external map first, falling back to this NIF's
/// local nodes. The body and head NIFs each spawn their own
/// (orphaned) copy of the skeleton's node hierarchy, but their
/// `SkinnedMesh.bones` references point at the SHARED skeleton
/// entities so all three palettes draw from one bone palette. Pre-fix
/// the body and head would each resolve against their own local
/// skeleton copies, leaving the head detached from the animated
/// skeleton.
///
/// Returns the local `node_by_name` map alongside the count and root
/// so the caller can chain it forward into the next NIF's external
/// skeleton parameter.
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_nif_bytes_with_skeleton(
    world: &mut World,
    ctx: &mut VulkanContext,
    data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    external_skeleton: Option<&std::collections::HashMap<std::sync::Arc<str>, EntityId>>,
    // M41.0 Phase 3b — optional callback invoked once after the
    // import returns and before the per-mesh GPU upload loop runs.
    // Lets the caller mutate `imported.meshes[i].positions`
    // (for FaceGen morph deformation: head NIF + EGM sliders) or
    // any other field the renderer reads from `ImportedMesh`.
    // `&mut dyn FnMut` (rather than a generic) keeps the function
    // monomorphisation-cheap; static-dispatch isn't worth a
    // generic parameter for a load-time call.
    pre_spawn_hook: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)>,
) -> (
    usize,
    Option<EntityId>,
    std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) {
    // #880 / CELL-PERF-02 — cache the parse + import + BGSM-merge
    // pipeline by lowercased path. Pre-fix every NPC spawn re-parsed
    // skeleton + body + hand NIFs from BSA bytes (~280 redundant
    // parses for Megaton-scale interiors). The cache is bypassed
    // when a `pre_spawn_hook` is provided (head-with-FaceGen-morph
    // path) because each NPC's morph is unique — caching the
    // already-morphed scene would hand the same face to every NPC.
    // The skeleton/body/hand calls all pass `pre_spawn_hook: None`,
    // so they hit the cache.
    let cache_key = label.to_ascii_lowercase();
    let cached_arc: Option<std::sync::Arc<byroredux_nif::import::ImportedScene>>;
    let mut owned_for_hook: Option<byroredux_nif::import::ImportedScene> = None;

    if let Some(hook) = pre_spawn_hook {
        // M41.0 Phase 3b — pre-spawn hook bypass. NPC head spawn
        // uses this hook to apply FaceGen FGGS / FGGA slider deltas
        // to `imported.meshes[head].positions` so the per-NPC unique
        // face shape lands in the GPU upload below. Recorded as a
        // bypass-parse so the cache's `parses` counter still
        // reflects total parse_nif invocations.
        {
            let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
            cache.record_bypass_parse();
        }
        let mut imported =
            match parse_import_and_merge(world, data, label, tex_provider, mat_provider) {
                Some(s) => s,
                None => return (0, None, std::collections::HashMap::new()),
            };
        hook(&mut imported);
        owned_for_hook = Some(imported);
        cached_arc = None;
    } else {
        // Cache routing: read-lock probe → parse + import + insert
        // on miss. Three-tier shape mirrors `cell_loader::load_references`
        // (#523). Negative-cache entries (failed parses) short-circuit
        // subsequent NPC spawns of the same path so the warning log
        // doesn't spam.
        let cached = {
            let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
            cache.get(&cache_key)
        };
        cached_arc = match cached {
            Some(Some(arc)) => Some(arc),
            Some(None) => {
                // Negative-cached parse failure — propagate the empty
                // result without re-parsing.
                return (0, None, std::collections::HashMap::new());
            }
            None => {
                let imported_opt =
                    parse_import_and_merge(world, data, label, tex_provider, mat_provider);
                let arc_opt = imported_opt.map(std::sync::Arc::new);
                let mut cache = world.resource_mut::<crate::scene_import_cache::SceneImportCache>();
                let stored = cache.insert(cache_key, arc_opt);
                match stored {
                    Some(arc) => Some(arc),
                    None => return (0, None, std::collections::HashMap::new()),
                }
            }
        };
    }

    // Bind a single `&ImportedScene` reference for the rest of the
    // function — the spawn loops only read. The borrow is anchored
    // in either `cached_arc` (cache hit / cache-miss insert) or
    // `owned_for_hook` (per-NPC FaceGen morph path); whichever one
    // is `Some` holds the live data.
    let imported: &byroredux_nif::import::ImportedScene = if let Some(ref s) = owned_for_hook {
        s
    } else {
        cached_arc
            .as_ref()
            .expect("either hook bypass or cache lookup must populate one branch")
            .as_ref()
    };

    // Phase 1: Spawn node entities (NiNode hierarchy).
    // node_index → EntityId mapping.
    // Also build a name → EntityId map so Phase 3 can resolve skinning
    // bone names to the entities they should drive. Skeleton nodes are
    // the only entities with unique names in a typical NIF, so collisions
    // (multiple nodes sharing a name) are rare; on collision we keep the
    // first spawn (root-most in depth-first order).
    let mut node_entities: Vec<EntityId> = Vec::with_capacity(imported.nodes.len());
    let mut node_by_name: std::collections::HashMap<std::sync::Arc<str>, EntityId> =
        std::collections::HashMap::with_capacity(imported.nodes.len());
    for node in &imported.nodes {
        let quat = Quat::from_xyzw(
            node.rotation[0],
            node.rotation[1],
            node.rotation[2],
            node.rotation[3],
        );
        let translation = Vec3::new(
            node.translation[0],
            node.translation[1],
            node.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, node.scale));
        world.insert(entity, GlobalTransform::IDENTITY);

        if let Some(ref name) = node.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
            node_by_name.entry(name.clone()).or_insert(entity);
        }

        // Attach collision data if present.
        if let Some((ref shape, ref body)) = node.collision {
            log::info!(
                "Collision attached to '{}': {:?} motion={:?} mass={:.1}",
                node.name.as_deref().unwrap_or("?"),
                std::mem::discriminant(shape),
                body.motion_type,
                body.mass,
            );
            world.insert(entity, shape.clone());
            world.insert(entity, body.clone());
        }

        // Attach Billboard component for NiBillboardNode-derived entities.
        // See #225 — nif import normalizes pre/post 10.1.0.0 mode layouts
        // into a single u16 before we map it to BillboardMode.
        if let Some(raw) = node.billboard_mode {
            world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
        }

        // Attach raw NiAVObject flags so gameplay systems can branch on
        // DISABLE_SORTING, SELECTIVE_UPDATE, IS_NODE, DISPLAY_OBJECT,
        // etc. without re-reading the source NIF. APP_CULLED (bit 0) is
        // already consumed by the import-time visibility filter in
        // `walk.rs`, so every spawned node arrives with that bit clear.
        // We still emit the component unconditionally (not gated on
        // `flags != 0`) so a future toggle-visible system can just flip
        // the bit on the existing component. See #222.
        if node.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(node.flags));
        }

        node_entities.push(entity);
    }

    // Phase 2: Set up Parent/Children relationships for nodes.
    for (node_idx, node) in imported.nodes.iter().enumerate() {
        if let Some(parent_idx) = node.parent_node {
            let child_entity = node_entities[node_idx];
            let parent_entity = node_entities[parent_idx];
            world.insert(child_entity, Parent(parent_entity));
            add_child(world, parent_entity, child_entity);
        }
    }

    // Phase 2.5: Particle emitters. The NIF importer surfaces every
    // NiParticleSystem / NiParticles / NiBSPArrayController as an
    // [`ImportedParticleEmitter`] tagged with its host node index, but
    // it doesn't carry per-emitter values — `NiPSysBlock` discards
    // every parsed field. We pick a heuristic ParticleEmitter preset
    // (torch_flame / smoke / magic_sparkles / generic flame fallback)
    // by scanning the host node's name. Every emitter is attached
    // directly to the host entity so the simulation sources its
    // world-space spawn origin from the host's GlobalTransform. See
    // #401 / audit OBL-D6-2.
    for emitter in &imported.particle_emitters {
        let Some(host_idx) = emitter.parent_node else {
            continue;
        };
        let Some(&host_entity) = node_entities.get(host_idx) else {
            continue;
        };
        let host_name = imported.nodes[host_idx]
            .name
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        // Embers / sparks check FIRST so a node like "FireSparks" lands
        // on the bright-glint preset rather than the larger torch flame
        // (the `fire` substring would otherwise win).
        let mut preset = if host_name.contains("spark")
            || host_name.contains("ember")
            || host_name.contains("cinder")
        {
            ParticleEmitter::embers()
        } else if host_name.contains("torch")
            || host_name.contains("fire")
            || host_name.contains("flame")
            || host_name.contains("brazier")
            || host_name.contains("candle")
        {
            ParticleEmitter::torch_flame()
        } else if host_name.contains("smoke")
            || host_name.contains("steam")
            || host_name.contains("ash")
        {
            ParticleEmitter::smoke()
        } else if host_name.contains("magic")
            || host_name.contains("enchant")
            || host_name.contains("sparkle")
            || host_name.contains("glow")
        {
            ParticleEmitter::magic_sparkles()
        } else {
            // Fallback — many vanilla NIFs don't name the host node
            // descriptively (e.g. just "EmitterNode"). Default to a
            // visible flame so the audit's "every torch invisible"
            // failure is still resolved end-to-end.
            ParticleEmitter::torch_flame()
        };
        // #707 / FX-2 — when the NIF authored a NiPSysColorModifier ->
        // NiColorData chain, the importer captured the keyframe
        // stream's first/last RGBA into `emitter.color_curve`.
        // Override the heuristic preset's start/end colour so authored
        // Dragonsreach embers / spell-cast colours / geyser steam read
        // distinctly from the generic preset values. Pre-fix the data
        // was parsed and immediately discarded — every torch looked
        // identical. The other preset fields (size_curve, lifetime,
        // emit_rate, etc.) stay at the heuristic preset's defaults
        // because the modifier only authors colour.
        if let Some(curve) = emitter.color_curve {
            preset.start_color = curve.start;
            preset.end_color = curve.end;
        }
        world.insert(host_entity, preset);
    }

    // Phase 3: Spawn mesh entities with parent links.
    let mut count = 0;
    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    for mesh in &imported.meshes {
        // M41.0 Phase 1b.x temp gate — vanilla FNV / FO3 actor body NIFs
        // ship 4 dismemberment-cap sub-meshes alongside the visible body
        // (`bodycaps`, `limbcaps`, `meatneck01`, `meathead01`). The
        // legacy engine hides them via `BSDismemberSkinInstance.partitions
        // [i].part_flag` until a body part is actually dismembered; we
        // don't honour that flag yet, so they render as inside-the-body
        // bloody geometry that looks like dark ribbons / spikes spilling
        // from the actor. Skipping by name keeps NPCs visually coherent
        // until the partition-flag visibility pipeline lands as its own
        // followup. Match-arm naming is conservative — these are exact
        // vanilla mesh-name conventions and won't false-positive on
        // anything else.
        let mesh_name = mesh.name.as_deref().unwrap_or("");
        if matches!(
            mesh_name,
            "bodycaps" | "limbcaps" | "meatneck01" | "meathead01"
        ) {
            log::debug!(
                "Phase 1b.x: skipping dismemberment cap '{}' until BSDismemberSkinInstance \
                 partition flags are wired",
                mesh_name,
            );
            continue;
        }

        let num_verts = mesh.positions.len();
        // Skinned vertices use the per-vertex bone indices + weights that
        // #151 / #177 extracted from NiSkinData / BSTriShape. Rigid
        // vertices pass zero weights and the shader's rigid-path routes
        // them through `pc.model` instead of the bone palette.
        let skin_vertex_data = mesh
            .skin
            .as_ref()
            .filter(|s| !s.vertex_bone_indices.is_empty() && !s.vertex_bone_weights.is_empty());
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                let position = mesh.positions[i];
                // Drop alpha — current `Vertex` color is 3-channel.
                // Imported colors carry RGBA so alpha is preserved on
                // the import side for a future 4-channel vertex format
                // (#618). Hair-tip / eyelash modulation will become
                // visible once the renderer's Vertex extends.
                let color = if i < mesh.colors.len() {
                    let c = mesh.colors[i];
                    [c[0], c[1], c[2]]
                } else {
                    [1.0, 1.0, 1.0]
                };
                let normal = if i < mesh.normals.len() {
                    mesh.normals[i]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let uv = if i < mesh.uvs.len() {
                    mesh.uvs[i]
                } else {
                    [0.0, 0.0]
                };
                // #783 / M-NORMALS — pull the per-vertex tangent (xyz +
                // bitangent sign) from the imported mesh when authored.
                // Empty `mesh.tangents` falls through to the zero-vec
                // default, which the fragment shader's perturbNormal
                // detects and routes to its screen-space derivative
                // fallback path. This preserves rendering correctness
                // for both Bethesda-with-tangents and synthetic-without
                // content paths.
                let tangent = if i < mesh.tangents.len() {
                    mesh.tangents[i]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                };
                if let Some(skin) = skin_vertex_data {
                    // Guard against parallel-vector truncation — if the
                    // sparse skin upload filled fewer vertices than the
                    // mesh has positions, fall back to rigid for the
                    // remainder rather than panicking on index.
                    if i < skin.vertex_bone_indices.len() && i < skin.vertex_bone_weights.len() {
                        let idx = skin.vertex_bone_indices[i];
                        let w = skin.vertex_bone_weights[i];
                        let mut v = Vertex::new_skinned(
                            position,
                            color,
                            normal,
                            uv,
                            [idx[0] as u32, idx[1] as u32, idx[2] as u32, idx[3] as u32],
                            w,
                        );
                        v.tangent = tangent;
                        return v;
                    }
                }
                let mut v = Vertex::new(position, color, normal, uv);
                v.tangent = tangent;
                v
            })
            .collect();

        let alloc = ctx.allocator.as_ref().unwrap();
        // upload_scene_mesh registers the vertices/indices into the global
        // geometry SSBO that RT ray queries sample for reflection UVs.
        // See #371.
        let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            &vertices,
            &mesh.indices,
            ctx.device_caps.ray_query_supported,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "Failed to upload NIF mesh '{}': {}",
                    mesh.name.as_deref().unwrap_or("?"),
                    e
                );
                continue;
            }
        };

        // Collect BLAS specs for batched build after the loop.
        blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32));

        // Mesh paths are interned `FixedString` handles (#609). Resolve
        // each populated slot to an owned `String` once for the
        // downstream `Material` component + texture-resolve calls. The
        // pool read lock is short-lived; the resolved Strings outlive it.
        let (
            owned_texture_path,
            owned_normal_map,
            owned_glow_map,
            owned_detail_map,
            owned_gloss_map,
            owned_dark_map,
            owned_parallax_map,
            owned_env_map,
            owned_env_mask,
            owned_material_path,
        ) = {
            let pool_read = world.resource::<StringPool>();
            let resolve_owned =
                |sym: Option<byroredux_core::string::FixedString>| -> Option<String> {
                    sym.and_then(|s| pool_read.resolve(s))
                        .map(|s| s.to_string())
                };
            (
                resolve_owned(mesh.texture_path),
                resolve_owned(mesh.normal_map),
                resolve_owned(mesh.glow_map),
                resolve_owned(mesh.detail_map),
                resolve_owned(mesh.gloss_map),
                resolve_owned(mesh.dark_map),
                resolve_owned(mesh.parallax_map),
                resolve_owned(mesh.env_map),
                resolve_owned(mesh.env_mask),
                resolve_owned(mesh.material_path),
            )
        };

        let tex_handle = resolve_texture(ctx, tex_provider, owned_texture_path.as_deref());

        let quat = Quat::from_xyzw(
            mesh.rotation[0],
            mesh.rotation[1],
            mesh.rotation[2],
            mesh.rotation[3],
        );
        let translation = Vec3::new(
            mesh.translation[0],
            mesh.translation[1],
            mesh.translation[2],
        );

        let entity = world.spawn();
        world.insert(entity, Transform::new(translation, quat, mesh.scale));
        world.insert(entity, GlobalTransform::IDENTITY);
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));

        // Attach bounding data (#217): LocalBound captures the mesh-local
        // sphere; WorldBound is a placeholder filled in by the bound
        // propagation system once GlobalTransform has been computed.
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
        world.insert(entity, WorldBound::ZERO);
        if mesh.has_alpha {
            world.insert(
                entity,
                AlphaBlend {
                    src_blend: mesh.src_blend_mode,
                    dst_blend: mesh.dst_blend_mode,
                },
            );
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        // #renderlayer — loose-NIF path has no REFR base record, so
        // the base layer defaults to Architecture (zero bias). The
        // per-mesh `is_decal` / `alpha_test_func` escalation still
        // applies — a NIF authored with explicit decal flags or
        // alpha-test cutout fringes gets the Decal layer regardless
        // of how it was spawned. NPC body / head / armor meshes
        // overwrite this with Actor in `npc_spawn::tag_descendants_as_actor`
        // after the spawn returns. Pre-#renderlayer this site also
        // inserted a `Decal` marker — retired in favour of
        // `RenderLayer::Decal`.
        {
            use byroredux_core::ecs::components::{
                escalate_small_static_to_clutter, render_layer_with_decal_escalation, RenderLayer,
            };
            // Loose-NIF spawn: no REFR, so no ref_scale to apply —
            // the mesh's local bound is its world bound. Same small-
            // STAT → Clutter rule as cell_loader so loose-loaded
            // desk papers don't z-fight against the desk loaded
            // alongside them.
            let layer = escalate_small_static_to_clutter(
                RenderLayer::Architecture,
                mesh.local_bound_radius,
            );
            let layer = render_layer_with_decal_escalation(layer, mesh.is_decal, mesh.alpha_test);
            world.insert(entity, layer);
        }
        // Carry `NiAVObject.flags` across — gameplay systems branch on
        // DISABLE_SORTING / SELECTIVE_UPDATE / DISPLAY_OBJECT bits
        // without touching the NIF source. APP_CULLED shapes never
        // reach this point (filtered import-side in walk.rs). See #222.
        if mesh.flags != 0 {
            world.insert(entity, SceneFlags::from_nif(mesh.flags));
        }
        // Attach material data (specular, emissive, glossiness, UV transform, etc.)
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                diffuse_color: mesh.diffuse_color,
                ambient_color: mesh.ambient_color,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: owned_normal_map.clone(),
                texture_path: owned_texture_path.clone(),
                material_path: owned_material_path.clone(),
                glow_map: owned_glow_map.clone(),
                detail_map: owned_detail_map.clone(),
                gloss_map: owned_gloss_map.clone(),
                dark_map: owned_dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
                z_test: mesh.z_test,
                z_write: mesh.z_write,
                z_function: mesh.z_function,
                shader_type_fields: if mesh.shader_type_fields.is_empty() {
                    None
                } else {
                    Some(Box::new(mesh.shader_type_fields.to_core()))
                },
                // #620 — BSEffectShaderProperty falloff cone (Skyrim+)
                // OR BSShaderNoLightingProperty falloff cone (FO3/FNV
                // SIBLING per #451). Either yields an `EffectFalloff`;
                // BSShaderNoLighting fills `soft_falloff_depth = 0.0`
                // since that block has no soft-depth field.
                effect_falloff: mesh
                    .effect_shader
                    .as_ref()
                    .map(
                        |es| byroredux_core::ecs::components::material::EffectFalloff {
                            start_angle: es.falloff_start_angle,
                            stop_angle: es.falloff_stop_angle,
                            start_opacity: es.falloff_start_opacity,
                            stop_opacity: es.falloff_stop_opacity,
                            soft_falloff_depth: es.soft_falloff_depth,
                        },
                    )
                    .or_else(|| {
                        mesh.no_lighting_falloff.as_ref().map(|nl| {
                            byroredux_core::ecs::components::material::EffectFalloff {
                                start_angle: nl.start_angle,
                                stop_angle: nl.stop_angle,
                                start_opacity: nl.start_opacity,
                                stop_opacity: nl.stop_opacity,
                                soft_falloff_depth: 0.0,
                            }
                        })
                    }),
                // #890 Stage 2 — see cell_loader.rs for the
                // identical packing site / explanation.
                effect_shader_flags: crate::cell_loader::pack_effect_shader_flags(
                    mesh.effect_shader.as_ref(),
                ),
            },
        );

        // Load and attach normal map texture handle.
        if let Some(ref nmap_path) = owned_normal_map {
            let h = resolve_texture(ctx, tex_provider, Some(nmap_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, NormalMapHandle(h));
            }
        }
        // Load and attach dark/lightmap texture handle.
        if let Some(ref dark_path) = owned_dark_map {
            let h = resolve_texture(ctx, tex_provider, Some(dark_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, DarkMapHandle(h));
            }
        }
        // #399 — three NiTexturingProperty extra slots packed into one
        // ECS component. Mirrors the cell_loader.rs path; only attached
        // when at least one slot resolved to a real texture handle.
        let mut resolve = |path: &Option<String>| -> u32 {
            path.as_deref()
                .map(|p| resolve_texture(ctx, tex_provider, Some(p)))
                .filter(|&h| h != ctx.texture_registry.fallback())
                .unwrap_or(0)
        };
        let glow_h = resolve(&owned_glow_map);
        let detail_h = resolve(&owned_detail_map);
        let gloss_h = resolve(&owned_gloss_map);
        let parallax_h = resolve(&owned_parallax_map);
        let env_h = resolve(&owned_env_map);
        let env_mask_h = resolve(&owned_env_mask);
        if glow_h != 0
            || detail_h != 0
            || gloss_h != 0
            || parallax_h != 0
            || env_h != 0
            || env_mask_h != 0
        {
            world.insert(
                entity,
                ExtraTextureMaps {
                    glow: glow_h,
                    detail: detail_h,
                    gloss: gloss_h,
                    parallax: parallax_h,
                    env: env_h,
                    env_mask: env_mask_h,
                    parallax_height_scale: mesh.parallax_height_scale.unwrap_or(0.04),
                    parallax_max_passes: mesh.parallax_max_passes.unwrap_or(4.0),
                },
            );
        }

        if let Some(ref name) = mesh.name {
            let mut pool = world.resource_mut::<StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
        }

        // Attach skinning binding if present. Resolves each bone name to
        // the entity spawned for that node in Phase 1. Missing bones are
        // kept as `None`; the palette system substitutes identity for them.
        if let Some(ref skin) = mesh.skin {
            if skin.bones.len() > MAX_BONES_PER_MESH {
                log::warn!(
                    "Skinned mesh '{}' has {} bones (> MAX_BONES_PER_MESH={}); skipping skinning",
                    mesh.name.as_deref().unwrap_or("?"),
                    skin.bones.len(),
                    MAX_BONES_PER_MESH
                );
            } else {
                let mut bones: Vec<Option<EntityId>> = Vec::with_capacity(skin.bones.len());
                let mut binds: Vec<Mat4> = Vec::with_capacity(skin.bones.len());
                let mut unresolved = 0_usize;
                let mut unresolved_names: Vec<&str> = Vec::new();
                for bone in &skin.bones {
                    // M41.0 Phase 1b: prefer the external skeleton
                    // map (set when the spawn function is assembling
                    // skeleton + body + head) so body/head NIF
                    // skinning resolves to the shared skeleton's
                    // entities, not the body/head's own orphaned
                    // local node copies.
                    let resolved = external_skeleton
                        .and_then(|m| m.get(&bone.name).copied())
                        .or_else(|| node_by_name.get(&bone.name).copied());
                    match resolved {
                        Some(e) => bones.push(Some(e)),
                        None => {
                            bones.push(None);
                            unresolved += 1;
                            if unresolved_names.len() < 8 {
                                unresolved_names.push(&bone.name);
                            }
                        }
                    }
                    binds.push(Mat4::from_cols_array_2d(&bone.bind_inverse));
                }
                // M41.0 Phase 1b.x — global_skin_transform investigation
                // resolved (#771 / LC-D3-NEW-01). Per nifly Skin.hpp:49-51,
                // NiSkinData::bones[i].boneTransform IS skin→bone
                // (compose-ready, includes the global offset). The
                // top-level skinTransform is therefore informational
                // only at runtime; `compute_palette_into` does NOT
                // multiply it. The first attempt at right-multiply
                // double-applied the global offset, which is why it
                // looked visually worse. Captured here for diagnostic
                // visibility (Doc Mitchell ships a non-identity cyclic
                // permutation; FO4+ BSSkin paths ship identity — the
                // asymmetry is informative).
                let global_skin_transform = Mat4::from_cols_array_2d(&skin.global_skin_transform);
                let root_entity = skin.skeleton_root.as_ref().and_then(|n| {
                    external_skeleton
                        .and_then(|m| m.get(n).copied())
                        .or_else(|| node_by_name.get(n).copied())
                });
                world.insert(
                    entity,
                    SkinnedMesh::new_with_global(root_entity, bones, binds, global_skin_transform),
                );
                if unresolved > 0 {
                    // M41.0 Phase 1b.x followup — unresolved bones land
                    // as `None` in `SkinnedMesh.bones`, and
                    // `compute_palette_into` substitutes
                    // `Mat4::IDENTITY` for those slots. Vertices weighted
                    // to such a slot end up at `vertex_local` (near NIF
                    // skin-space origin) while neighbours weighted to
                    // resolved bones land at world coords, producing
                    // triangle ribbons stretched from origin to the
                    // actor's placement. Logging the names so we can see
                    // which sub-skeleton convention is mismatched
                    // between the source NIF and the external skeleton
                    // map.
                    log::warn!(
                        "Skinned mesh '{}': {} bones ({} UNRESOLVED — names: {:?}), root={:?}",
                        mesh.name.as_deref().unwrap_or("?"),
                        skin.bones.len(),
                        unresolved,
                        unresolved_names,
                        skin.skeleton_root,
                    );
                } else {
                    log::info!(
                        "Skinned mesh '{}': {} bones (0 unresolved), root={:?}",
                        mesh.name.as_deref().unwrap_or("?"),
                        skin.bones.len(),
                        skin.skeleton_root,
                    );
                }
            }
        }

        // Set up parent relationship.
        if let Some(parent_idx) = mesh.parent_node {
            let parent_entity = node_entities[parent_idx];
            world.insert(entity, Parent(parent_entity));
            add_child(world, parent_entity, entity);
        }

        log::info!(
            "Loaded NIF mesh '{}': {} verts, {} tris, tex={:?}",
            mesh.name.as_deref().unwrap_or("unnamed"),
            num_verts,
            mesh.indices.len() / 3,
            mesh.texture_path,
        );
        count += 1;
    }

    // Batched BLAS build: single GPU submission for all NIF meshes.
    if !blas_specs.is_empty() {
        ctx.build_blas_batched(&blas_specs);
    }

    let root = node_entities.first().copied();

    // #261 — mesh-embedded controller chains (water UV scroll, torch
    // flame visibility, lava emissive pulse). `import_nif_scene`
    // collected every NiObjectNET.controller_ref chain into a single
    // looping clip. Register it and spawn an AnimationPlayer scoped to
    // the NIF root so the subtree-local name lookup works the same way
    // it does for KF clips.
    if let Some(nif_embedded_clip) = imported.embedded_clip.as_ref() {
        let float_ct = nif_embedded_clip.float_channels.len();
        let color_ct = nif_embedded_clip.color_channels.len();
        let bool_ct = nif_embedded_clip.bool_channels.len();
        let duration = nif_embedded_clip.duration;
        let clip_handle = {
            let mut pool = world.resource_mut::<StringPool>();
            let clip = crate::anim_convert::convert_nif_clip(nif_embedded_clip, &mut pool);
            drop(pool);
            let mut registry = world.resource_mut::<AnimationClipRegistry>();
            registry.add(clip)
        };
        let player_entity = world.spawn();
        let mut player = AnimationPlayer::new(clip_handle);
        if let Some(root) = root {
            player.root_entity = Some(root);
        }
        world.insert(player_entity, player);
        log::info!(
            "Embedded animation clip registered from '{}' ({:.2}s, {} float + {} color + {} bool channels) → handle {}",
            label,
            duration,
            float_ct,
            color_ct,
            bool_ct,
            clip_handle,
        );
    }

    log::info!(
        "Imported {} nodes + {} meshes from '{}'",
        imported.nodes.len(),
        count,
        label
    );
    (count + imported.nodes.len(), root, node_by_name)
}
