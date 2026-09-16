//! ECS host adapter for the renderer-independent `byroredux-sdk` Studio API.
//!
//! The Studio document is also a material *gallery*: a Cornell room that
//! assets from every installed game are added to one at a time, each stood on
//! the floor beside the last, so every title's materials are judged under the
//! same light and shadow. The room refits around whatever is placed.
//!
//! Every placed asset — and the room itself — owns its entities through a
//! `CellRoot`, so removal goes through [`cell_loader::unload_cell`]'s mesh /
//! BLAS / texture reference accounting rather than a second teardown path.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use byroredux_core::ecs::{
    ActiveCamera, EntityId, GameProfileEntry, GameProfileRegistry, GlobalTransform, LocalBound,
    Material, Parent, Resource, Transform, World, WorldBound,
};
use byroredux_core::math::{EulerRot, Quat, Vec3};
use byroredux_renderer::VulkanContext;
use byroredux_sdk::identity::ObjectId;
use byroredux_sdk::studio::{
    filter_catalog, gallery_offset, pick_spheres, AssetBounds, AssetId, AssetSource, BoundSphere,
    CatalogGame, CatalogView, CornellFit, MaterialValue, ObjectSnapshot, PlacedAssetSnapshot,
    StudioCommand, StudioSnapshot, TransformValue,
};

use crate::asset_provider::{
    build_material_provider, build_texture_provider, MaterialProvider, TextureProvider,
};
use crate::cell_loader;
use crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES;

/// Browser rows shown for one filter; the snapshot reports the full count.
const CATALOG_PAGE_LIMIT: usize = 200;

/// Game label of an asset imported from the command line at boot.
const BOOT_GAME: &str = "cli";

#[derive(Debug, Clone, Copy)]
struct StudioObjectBinding {
    id: ObjectId,
    entity: EntityId,
    asset: AssetId,
}

#[derive(Debug, Clone)]
struct PlacedAsset {
    id: AssetId,
    game: String,
    path: String,
    /// `CellRoot` owning every entity the import spawned.
    owner: EntityId,
    /// The imported entity range, for re-packing the row after a removal.
    first: EntityId,
    last: EntityId,
    /// World-space envelope after placement.
    bounds: AssetBounds,
    object_count: usize,
}

#[derive(Debug, Clone)]
enum GalleryOp {
    Add { game: String, path: String },
    Remove(AssetId),
    Clear,
}

/// ECS-owned state behind the public renderer-independent Studio contract.
/// Raw entity IDs remain private to this adapter.
#[derive(Debug)]
pub(crate) struct StudioSession {
    source: AssetSource,
    objects: Vec<StudioObjectBinding>,
    selected: Option<ObjectId>,
    revision: u64,
    original_transforms: BTreeMap<ObjectId, TransformValue>,
    assets: Vec<PlacedAsset>,
    /// `CellRoot` owning the current room geometry and lights.
    room: Option<EntityId>,
    next_object_ordinal: usize,
    next_asset: u64,
    catalog: CatalogView,
    /// Gallery operations that need the renderer; drained by [`step`].
    pending: Vec<GalleryOp>,
    status: Option<String>,
    /// `studio.overlay` request, applied by the frame loop — the overlay
    /// state lives on the App, out of a console command's reach.
    overlay_request: Option<bool>,
}

impl Resource for StudioSession {}

/// One installed title the gallery can open.
struct InstalledGame {
    key: String,
    name: String,
    data_dir: PathBuf,
    entry: GameProfileEntry,
}

/// A title whose archives are open: providers exactly as a `--game` launch of
/// it would build them, plus its sorted asset listing.
struct OpenedGame {
    textures: TextureProvider,
    materials: MaterialProvider,
    assets: Vec<String>,
}

/// Archive state for the gallery, kept apart from [`StudioSession`] so a
/// snapshot never touches providers. Titles open lazily on first browse.
pub(crate) struct StudioArchives {
    games: Vec<InstalledGame>,
    opened: HashMap<String, OpenedGame>,
}

impl Resource for StudioArchives {}

/// Asset the command line imported before Studio opened (`--studio <nif>` or
/// `--studio --mesh <path>`): the entities `first..last`, imported under
/// `label`.
pub(crate) struct BootAsset {
    pub(crate) label: String,
    pub(crate) first: EntityId,
    pub(crate) last: EntityId,
}

/// Open the Studio document: the boot asset (if any) becomes the gallery's
/// first entry, stood in a room fitted around it. Returns the camera pose.
pub(crate) fn open(
    world: &mut World,
    ctx: &mut VulkanContext,
    boot: Option<BootAsset>,
) -> (Vec3, Vec3) {
    let label = boot
        .as_ref()
        .map_or_else(|| "Material gallery".to_owned(), |boot| boot.label.clone());
    install_session(world, AssetSource { label });

    let games = installed_games(world);
    log::info!(
        "Studio gallery: {} installed game(s): {}",
        games.len(),
        games
            .iter()
            .map(|game| game.key.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    world.resource_mut::<StudioSession>().catalog.games = games
        .iter()
        .map(|game| CatalogGame {
            key: game.key.clone(),
            name: game.name.clone(),
        })
        .collect();
    world.insert_resource(StudioArchives {
        games,
        opened: HashMap::new(),
    });

    if let Some(boot) = boot {
        let cache_key = boot.label.to_ascii_lowercase();
        adopt_asset(
            world,
            BOOT_GAME,
            &boot.label,
            &cache_key,
            boot.first,
            boot.last,
        );
    }
    rebuild_room(world, ctx)
}

/// Insert an empty Studio document.
fn install_session(world: &mut World, source: AssetSource) {
    world.insert_resource(StudioSession {
        source,
        objects: Vec::new(),
        selected: None,
        revision: 0,
        original_transforms: BTreeMap::new(),
        assets: Vec::new(),
        room: None,
        next_object_ordinal: 0,
        next_asset: 1,
        catalog: CatalogView::default(),
        pending: Vec::new(),
        status: None,
        overlay_request: None,
    });
}

/// Ask the frame loop to show or hide the debug overlay, so scripted
/// captures of the room are not covered by the Studio window.
pub(crate) fn request_overlay(world: &World, visible: bool) -> Result<(), String> {
    let Some(mut session) = world.try_resource_mut::<StudioSession>() else {
        return Err("no Studio document is open (launch with --studio)".to_owned());
    };
    session.overlay_request = Some(visible);
    Ok(())
}

pub(crate) fn take_overlay_request(world: &World) -> Option<bool> {
    world
        .try_resource_mut::<StudioSession>()?
        .overlay_request
        .take()
}

/// Bind `entities` to `asset` as editable objects. IDs continue across
/// assets in canonical import order and are never reused within a document.
/// The first new object becomes the selection.
fn bind_objects(world: &mut World, asset: AssetId, entities: &[EntityId]) {
    // Read every transform before taking the session lock — never hold a
    // storage guard across a resource acquisition.
    let transforms: Vec<(EntityId, Option<TransformValue>)> = entities
        .iter()
        .map(|&entity| {
            (
                entity,
                world
                    .get::<Transform>(entity)
                    .map(|transform| transform_value(&transform)),
            )
        })
        .collect();
    let mut session = world.resource_mut::<StudioSession>();
    for (index, (entity, transform)) in transforms.into_iter().enumerate() {
        let id = ObjectId::from_import_ordinal(session.next_object_ordinal)
            .expect("Studio object count exceeds the ObjectId range");
        session.next_object_ordinal += 1;
        session
            .objects
            .push(StudioObjectBinding { id, entity, asset });
        if let Some(transform) = transform {
            session.original_transforms.insert(id, transform);
        }
        if index == 0 {
            session.selected = Some(id);
        }
    }
}

fn transform_value(transform: &Transform) -> TransformValue {
    let (x, y, z) = transform.rotation.to_euler(EulerRot::XYZ);
    TransformValue {
        translation: transform.translation.to_array(),
        rotation_degrees: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
        scale: transform.scale,
    }
}

/// The spawned entities in `first..last` that carry a bound (the editable
/// objects), sorted into canonical import order, and their world-space sphere
/// envelope. Runs transform + bound propagation first so the envelope is
/// current. Also used by the plain loose-NIF viewer to aim its camera.
pub(crate) fn loaded_objects(
    world: &mut World,
    first: EntityId,
    last: EntityId,
) -> (Vec<EntityId>, Option<AssetBounds>) {
    propagate(world);
    let mut objects: Vec<EntityId> = world
        .query::<LocalBound>()
        .map(|query| {
            query
                .iter()
                .filter_map(|(entity, _)| (entity >= first && entity < last).then_some(entity))
                .collect()
        })
        .unwrap_or_default();
    // Canonicalize the import order before assigning SDK ObjectIds. Entity
    // allocation follows the deterministic NIF/SPT traversal, while storage
    // iteration order is an implementation detail.
    objects.sort_unstable();
    let bounds = AssetBounds::from_spheres(objects.iter().filter_map(|&entity| {
        world.get::<WorldBound>(entity).map(|bound| BoundSphere {
            center: bound.center.to_array(),
            radius: bound.radius,
        })
    }));
    (objects, bounds)
}

/// The same two passes the scheduler runs every frame; idempotent at setup.
fn propagate(world: &mut World) {
    let mut transforms = byroredux_core::ecs::systems::make_transform_propagation_system();
    transforms(world, 0.0);
    let mut bounds = crate::systems::make_world_bound_propagation_system();
    bounds(world, 0.0);
}

pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {
    let (source_label, revision, selected, bindings, assets, catalog, status) = {
        let session = world.try_resource::<StudioSession>()?;
        (
            session.source.label.clone(),
            session.revision,
            session.selected,
            session.objects.clone(),
            session
                .assets
                .iter()
                .map(|asset| PlacedAssetSnapshot {
                    id: asset.id,
                    game: asset.game.clone(),
                    path: asset.path.clone(),
                    object_count: asset.object_count,
                })
                .collect::<Vec<_>>(),
            session.catalog.clone(),
            session.status.clone(),
        )
    };
    let objects = bindings
        .iter()
        .filter_map(|binding| {
            // #3445 (CONC-D3-2026-08-27b-03) — resolve the name FIRST via
            // the shared canonical-order helper (`Name` then the string-
            // interning resource, #313), whose own guards are fully
            // dropped before it returns, then acquire Transform/Material.
            // No storage lock is ever held across a different storage's
            // acquisition here, unlike the pre-fix shape where the
            // string-interning resource (acquired once, outside this
            // closure) stayed alive across every entity's Transform AND
            // Name read, inverting the tail every other caller in this
            // codebase respects.
            //
            // NOTE for future editors: this comment deliberately never
            // spells out the string-interning resource's own type name —
            // the sibling regression test in this file's own `mod tests`
            // scans this function's source for exactly that name, and
            // writing it here would make the test match its own
            // describing comment instead of real code.
            let name = crate::commands::shared::resolve_entity_name(world, binding.entity)
                .unwrap_or_default();
            let transform = transform_value(&*world.get::<Transform>(binding.entity)?);
            let material = world
                .get::<Material>(binding.entity)
                .map(|material| MaterialValue {
                    diffuse_color: material.diffuse_color,
                    metalness: material.metalness,
                    roughness: material.roughness,
                    alpha: material.alpha,
                    ior: material.ior,
                });
            Some(ObjectSnapshot {
                id: binding.id,
                name,
                transform,
                material,
            })
        })
        .collect();
    Some(StudioSnapshot {
        source_label,
        revision,
        selected,
        objects,
        assets,
        catalog,
        status,
    })
}

pub(crate) fn apply_command(world: &mut World, command: StudioCommand) {
    if world.try_resource::<StudioSession>().is_none() {
        return;
    }
    match command {
        StudioCommand::Select(object) => {
            let allowed = object.is_none_or(|object| entity_for(world, object).is_some());
            if allowed {
                world.resource_mut::<StudioSession>().selected = object;
            }
        }
        StudioCommand::PickFromView => pick_from_view(world),
        StudioCommand::SetTransform { object, value } => {
            let Some(entity) = entity_for(world, object) else {
                return;
            };
            if !valid_transform(value) {
                return;
            }
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                let radians = value.rotation_degrees.map(f32::to_radians);
                transform.translation = Vec3::from_array(value.translation);
                transform.rotation =
                    Quat::from_euler(EulerRot::XYZ, radians[0], radians[1], radians[2]);
                transform.scale = value.scale.clamp(0.001, 10_000.0);
                bump_revision(world);
            }
        }
        StudioCommand::ResetTransform(object) => {
            let original = world
                .resource::<StudioSession>()
                .original_transforms
                .get(&object)
                .copied();
            if let Some(value) = original {
                apply_command(world, StudioCommand::SetTransform { object, value });
            }
        }
        StudioCommand::SetMaterial { object, value } => {
            let Some(entity) = entity_for(world, object) else {
                return;
            };
            if !valid_material(value) {
                return;
            }
            if let Some(material) = world.get_mut::<Material>(entity) {
                material.diffuse_color = value.diffuse_color.map(|v| v.clamp(0.0, 1.0));
                material.metalness = value.metalness.clamp(0.0, 1.0);
                material.roughness = value.roughness.clamp(0.0, 1.0);
                material.alpha = value.alpha.clamp(0.0, 1.0);
                material.ior = value.ior.clamp(1.0, 3.0);
                bump_revision(world);
            }
        }
        StudioCommand::FrameSelection(object) => frame_selection(world, object),
        StudioCommand::BrowseCatalog { .. }
        | StudioCommand::AddAsset { .. }
        | StudioCommand::RemoveAsset(_)
        | StudioCommand::ClearAssets => {
            if let Err(error) = apply_gallery_command(world, command) {
                set_status(world, error);
            }
        }
    }
}

/// The gallery half of the Studio protocol. Needs only `&World` — browsing
/// fills the catalog and the rest queue for [`step`] — so the `studio.*`
/// console commands drive exactly the path the egui panel does.
pub(crate) fn apply_gallery_command(world: &World, command: StudioCommand) -> Result<(), String> {
    let Some(mut session) = world.try_resource_mut::<StudioSession>() else {
        return Err("no Studio document is open (launch with --studio)".to_owned());
    };
    let op = match command {
        StudioCommand::BrowseCatalog { game, filter } => {
            drop(session);
            return browse_catalog(world, game, filter);
        }
        StudioCommand::AddAsset { game, path } => GalleryOp::Add { game, path },
        StudioCommand::RemoveAsset(asset) => GalleryOp::Remove(asset),
        StudioCommand::ClearAssets => GalleryOp::Clear,
        other => return Err(format!("{other:?} is not a gallery command")),
    };
    session.pending.push(op);
    Ok(())
}

/// World-space envelope of every placed asset, for `studio.list`.
pub(crate) fn placed_asset_bounds(world: &World) -> Vec<(AssetId, AssetBounds)> {
    world
        .try_resource::<StudioSession>()
        .map(|session| {
            session
                .assets
                .iter()
                .map(|asset| (asset.id, asset.bounds))
                .collect()
        })
        .unwrap_or_default()
}

/// Installed games as `(key, name, archives_open)`, for `studio.games`.
pub(crate) fn installed_game_list(world: &World) -> Option<Vec<(String, String, bool)>> {
    let archives = world.try_resource::<StudioArchives>()?;
    Some(
        archives
            .games
            .iter()
            .map(|game| {
                (
                    game.key.clone(),
                    game.name.clone(),
                    archives.opened.contains_key(&game.key),
                )
            })
            .collect(),
    )
}

/// Apply queued gallery operations. They import or reclaim GPU resources, so
/// they run at the frame boundary with the renderer instead of inside the UI
/// output pass that queued them. The room is rebuilt once per batch.
pub(crate) fn step(world: &mut World, ctx: &mut VulkanContext) {
    let ops = match world.try_resource_mut::<StudioSession>() {
        Some(mut session) => std::mem::take(&mut session.pending),
        None => return,
    };
    if ops.is_empty() {
        return;
    }
    let mut removed_any = false;
    for op in ops {
        removed_any |= matches!(op, GalleryOp::Remove(_) | GalleryOp::Clear);
        let status = match op {
            GalleryOp::Add { game, path } => match add_asset(world, ctx, &game, &path) {
                Ok(objects) => format!("Added {game} · {path} ({objects} objects)"),
                Err(error) => {
                    log::warn!("Studio gallery: {error}");
                    error
                }
            },
            GalleryOp::Remove(asset) => match remove_asset(world, ctx, asset) {
                Some(path) => format!("Removed {path}"),
                None => "That asset is no longer in the room".to_owned(),
            },
            GalleryOp::Clear => {
                let assets: Vec<AssetId> = world
                    .resource::<StudioSession>()
                    .assets
                    .iter()
                    .map(|asset| asset.id)
                    .collect();
                for &asset in &assets {
                    remove_asset(world, ctx, asset);
                }
                format!("Cleared {} asset(s)", assets.len())
            }
        };
        set_status(world, status);
    }
    if removed_any {
        repack_assets(world);
    }
    rebuild_room(world, ctx);
    bump_revision(world);
    // Accumulated SVGF/TAA history belongs to the previous room.
    ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
}

fn set_status(world: &World, status: String) {
    world.resource_mut::<StudioSession>().status = Some(status);
}

/// Every profile whose data directory exists, in engine-support order.
fn installed_games(world: &World) -> Vec<InstalledGame> {
    let Some(registry) = world.try_resource::<GameProfileRegistry>() else {
        return Vec::new();
    };
    let defaults = crate::game_profiles::load_launch_defaults();
    let games_root = crate::game_profiles::resolve_games_root(defaults.games_root.as_deref());
    let mut games: Vec<InstalledGame> = registry
        .iter()
        .filter_map(|(key, entry)| {
            let data_dir = crate::game_profiles::resolve_profile_root(entry, &games_root);
            if data_dir.as_os_str().is_empty() || !data_dir.is_dir() {
                return None;
            }
            // Alternate releases share a key; the archives on disk choose.
            let entry = entry.for_data_dir(&data_dir);
            Some(InstalledGame {
                key: key.to_owned(),
                name: entry.name.clone(),
                data_dir,
                entry,
            })
        })
        .collect();
    games.sort_by_key(|game| {
        let support_order = byroredux_game_detect::catalog::STEAM_APPS
            .iter()
            .position(|app| app.profile == game.key)
            .unwrap_or(usize::MAX);
        (support_order, game.key.clone())
    });
    games
}

/// Open `key`'s archives if they are not open yet.
fn ensure_opened(world: &World, key: &str) -> Result<(), String> {
    let Some(mut archives) = world.try_resource_mut::<StudioArchives>() else {
        return Err("the Studio gallery has no game archives".to_owned());
    };
    if archives.opened.contains_key(key) {
        return Ok(());
    }
    let (entry, data_dir) = archives
        .games
        .iter()
        .find(|game| game.key == key)
        .map(|game| (game.entry.clone(), game.data_dir.clone()))
        .ok_or_else(|| format!("'{key}' is not an installed game"))?;
    let started = std::time::Instant::now();
    let args = crate::boot::profile_archive_args(&entry, &data_dir, key);
    let textures = build_texture_provider(&args).with_registry_namespace(key);
    let materials = build_material_provider(&args);
    let assets = textures.mesh_asset_paths();
    log::info!(
        "Studio gallery: opened {} ({} importable assets) in {:.1} s",
        entry.name,
        assets.len(),
        started.elapsed().as_secs_f64(),
    );
    archives.opened.insert(
        key.to_owned(),
        OpenedGame {
            textures,
            materials,
            assets,
        },
    );
    Ok(())
}

fn browse_catalog(world: &World, game: String, filter: String) -> Result<(), String> {
    ensure_opened(world, &game)?;
    let (page, total_assets) = {
        let archives = world.resource::<StudioArchives>();
        let assets = &archives.opened[&game].assets;
        (
            filter_catalog(assets, &filter, CATALOG_PAGE_LIMIT),
            assets.len(),
        )
    };
    let mut session = world.resource_mut::<StudioSession>();
    session.catalog.game = Some(game);
    session.catalog.filter = filter;
    session.catalog.page = page;
    session.catalog.total_assets = total_assets;
    Ok(())
}

/// Import one asset from `game` and place it. Returns its object count.
fn add_asset(
    world: &mut World,
    ctx: &mut VulkanContext,
    game: &str,
    path: &str,
) -> Result<usize, String> {
    ensure_opened(world, game)?;
    // The loader needs `&mut World` alongside the providers, so the opened
    // game leaves the resource for the duration of the import.
    let mut opened = world
        .resource_mut::<StudioArchives>()
        .opened
        .remove(game)
        .expect("ensure_opened just inserted it");
    let result = import_asset(world, ctx, game, path, &mut opened);
    world
        .resource_mut::<StudioArchives>()
        .opened
        .insert(game.to_owned(), opened);
    result
}

fn import_asset(
    world: &mut World,
    ctx: &mut VulkanContext,
    game: &str,
    path: &str,
    opened: &mut OpenedGame,
) -> Result<usize, String> {
    let bytes = opened
        .textures
        .extract_mesh_exact(path)
        .or_else(|| opened.textures.extract_mesh(path))
        .ok_or_else(|| format!("{path} is not in {game}'s mesh archives"))?;
    // Game-qualified label: `SceneImportCache` keys parsed scenes by label,
    // and two titles ship different files under the same path. The label's
    // extension still drives the `.spt` routing.
    let label = format!("{game}:{path}");
    let first = world.next_entity_id();
    let (count, _) = crate::scene::load_nif_bytes(
        world,
        ctx,
        &bytes,
        &label,
        &opened.textures,
        Some(&mut opened.materials),
    );
    crate::scene::flush_pending_loose_textures(ctx);
    let last = world.next_entity_id();
    if count == 0 {
        if last > first {
            // Reclaim nodes / emitters spawned before the import came up empty.
            let owner = world.spawn();
            cell_loader::stamp_cell_root(world, owner, first, last);
            cell_loader::unload_cell(world, ctx, owner);
        }
        return Err(format!("{game} · {path} imported no renderable geometry"));
    }
    Ok(adopt_asset(
        world,
        game,
        path,
        &label.to_ascii_lowercase(),
        first,
        last,
    ))
}

/// Take ownership of the freshly imported entities `first..last`: stamp them
/// under one `CellRoot`, stand them on the floor beside the assets already
/// placed, and bind their objects. Returns the object count.
fn adopt_asset(
    world: &mut World,
    game: &str,
    path: &str,
    cache_key: &str,
    first: EntityId,
    last: EntityId,
) -> usize {
    let owner = world.spawn();
    cell_loader::stamp_cell_root(world, owner, first, last);

    let (objects, sphere_bounds) = loaded_objects(world, first, last);
    // Prefer the tight vertex envelope: sphere bounds only enclose geometry,
    // so grounding on them would leave the asset floating above the floor.
    let bounds = scene_geometry_bounds(world, cache_key)
        .or(sphere_bounds)
        .unwrap_or(AssetBounds {
            min: [-1.0; 3],
            max: [1.0; 3],
        });
    let occupied = occupied_bounds(&world.resource::<StudioSession>());
    let offset = gallery_offset(occupied, bounds);
    translate_top_level(world, first, last, offset);
    propagate(world);

    let (id, object_count) = {
        let mut session = world.resource_mut::<StudioSession>();
        let id = AssetId::new(session.next_asset).expect("asset counter starts at 1");
        session.next_asset += 1;
        session.assets.push(PlacedAsset {
            id,
            game: game.to_owned(),
            path: path.to_owned(),
            owner,
            first,
            last,
            bounds: bounds.translated(offset),
            object_count: objects.len(),
        });
        (id, objects.len())
    };
    bind_objects(world, id, &objects);
    log::info!("Studio gallery: placed {game} · {path} ({object_count} objects)");
    object_count
}

fn scene_geometry_bounds(world: &World, cache_key: &str) -> Option<AssetBounds> {
    let scene = world
        .try_resource::<crate::scene_import_cache::SceneImportCache>()?
        .peek(cache_key)?;
    let (min, max) = scene.geometry_bounds()?;
    Some(AssetBounds { min, max })
}

fn occupied_bounds(session: &StudioSession) -> Option<AssetBounds> {
    session
        .assets
        .iter()
        .map(|asset| asset.bounds)
        .reduce(AssetBounds::union)
}

/// Move every root of the imported hierarchy (no `Parent`) by `offset`.
/// Children follow through propagation.
fn translate_top_level(world: &mut World, first: EntityId, last: EntityId, offset: [f32; 3]) {
    let offset = Vec3::from_array(offset);
    let roots: Vec<EntityId> = (first..last)
        .filter(|&entity| {
            world.get::<Transform>(entity).is_some() && world.get::<Parent>(entity).is_none()
        })
        .collect();
    for entity in roots {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation += offset;
        }
    }
}

/// Close the gaps a removal leaves: slide every remaining asset, in placement
/// order, to where [`gallery_offset`] would put it now, so the room refits
/// around the objects instead of empty floor. Objects keep their relative
/// edits; the recorded "original" transforms move with them so Reset
/// Transform does not jump back into the gap.
fn repack_assets(world: &mut World) {
    let assets: Vec<(AssetId, EntityId, EntityId, AssetBounds)> = world
        .resource::<StudioSession>()
        .assets
        .iter()
        .map(|asset| (asset.id, asset.first, asset.last, asset.bounds))
        .collect();
    let mut occupied: Option<AssetBounds> = None;
    for (id, first, last, bounds) in assets {
        let offset = gallery_offset(occupied, bounds);
        let placed = bounds.translated(offset);
        occupied = Some(occupied.map_or(placed, |union| union.union(placed)));
        if offset == [0.0; 3] {
            continue;
        }
        translate_top_level(world, first, last, offset);
        let mut session = world.resource_mut::<StudioSession>();
        let moved: Vec<ObjectId> = session
            .objects
            .iter()
            .filter(|binding| binding.asset == id)
            .map(|binding| binding.id)
            .collect();
        for object in moved {
            if let Some(original) = session.original_transforms.get_mut(&object) {
                for (axis, delta) in original.translation.iter_mut().zip(offset) {
                    *axis += delta;
                }
            }
        }
        if let Some(asset) = session.assets.iter_mut().find(|asset| asset.id == id) {
            asset.bounds = placed;
        }
    }
    propagate(world);
}

/// Forget `asset` in the document (objects, originals, selection). Returns
/// the removed record so the caller can reclaim its entities.
fn forget_asset(world: &World, asset: AssetId) -> Option<PlacedAsset> {
    let mut session = world.resource_mut::<StudioSession>();
    let index = session
        .assets
        .iter()
        .position(|placed| placed.id == asset)?;
    let removed = session.assets.remove(index);
    let gone: Vec<ObjectId> = session
        .objects
        .iter()
        .filter(|binding| binding.asset == asset)
        .map(|binding| binding.id)
        .collect();
    session.objects.retain(|binding| binding.asset != asset);
    for object in &gone {
        session.original_transforms.remove(object);
    }
    if session
        .selected
        .is_some_and(|selected| gone.contains(&selected))
    {
        session.selected = session.objects.first().map(|binding| binding.id);
    }
    Some(removed)
}

fn remove_asset(world: &mut World, ctx: &mut VulkanContext, asset: AssetId) -> Option<String> {
    let removed = forget_asset(world, asset)?;
    cell_loader::unload_cell(world, ctx, removed.owner);
    Some(format!("{} · {}", removed.game, removed.path))
}

/// Room envelope before anything is placed: a one-metre cube on the floor.
fn empty_room_bounds() -> AssetBounds {
    let half = byroredux_core::lighting::BETHESDA_UNITS_PER_METER * 0.5;
    AssetBounds {
        min: [-half, 0.0, -half],
        max: [half, 2.0 * half, half],
    }
}

/// Replace the room with one fitted around everything placed, and frame it.
fn rebuild_room(world: &mut World, ctx: &mut VulkanContext) -> (Vec3, Vec3) {
    let (old_room, occupied) = {
        let mut session = world.resource_mut::<StudioSession>();
        (session.room.take(), occupied_bounds(&session))
    };
    if let Some(room) = old_room {
        cell_loader::unload_cell(world, ctx, room);
    }
    let fit = CornellFit::around(occupied.unwrap_or_else(empty_room_bounds));
    let first = world.next_entity_id();
    let (camera, target) = crate::cornell::setup_studio_room(world, ctx, fit);
    let last = world.next_entity_id();
    let owner = world.spawn();
    cell_loader::stamp_cell_root(world, owner, first, last);
    world.resource_mut::<StudioSession>().room = Some(owner);
    aim_camera(world, camera, target);
    (camera, target)
}

fn entity_for(world: &World, object: ObjectId) -> Option<EntityId> {
    world
        .resource::<StudioSession>()
        .objects
        .iter()
        .find_map(|binding| (binding.id == object).then_some(binding.entity))
}

fn bump_revision(world: &World) {
    let mut session = world.resource_mut::<StudioSession>();
    session.revision = session.revision.saturating_add(1);
}

fn pick_from_view(world: &World) {
    let Some((origin, direction)) = crate::interaction::camera_ray(world) else {
        return;
    };
    let objects = world.resource::<StudioSession>().objects.clone();
    let spheres = objects.into_iter().filter_map(|binding| {
        let bound = world.get::<WorldBound>(binding.entity)?;
        Some((
            binding.id,
            BoundSphere {
                center: bound.center.to_array(),
                radius: bound.radius,
            },
        ))
    });
    let selected = pick_spheres(origin.to_array(), direction.to_array(), spheres);
    world.resource_mut::<StudioSession>().selected = selected;
}

fn frame_selection(world: &mut World, object: ObjectId) {
    let Some(entity) = entity_for(world, object) else {
        return;
    };
    let (center, radius) = world
        .get::<WorldBound>(entity)
        .map(|bound| (bound.center, bound.radius.max(0.5)))
        .unwrap_or_else(|| {
            let center = world
                .get::<GlobalTransform>(entity)
                .map(|value| value.translation)
                .unwrap_or(Vec3::ZERO);
            (center, 1.0)
        });
    let position = center + Vec3::Z * (radius * 3.0).max(2.0);
    aim_camera(world, position, center);
}

/// Put the active camera at `position` looking at `target`, seeding the fly
/// camera's yaw/pitch so the first mouse movement continues from this pose
/// instead of snapping (same convention as the spawn camera, #2383(4)).
fn aim_camera(world: &mut World, position: Vec3, target: Vec3) {
    let Some(camera) = world.try_resource::<ActiveCamera>().map(|camera| camera.0) else {
        return;
    };
    let forward = (target - position).normalize_or_zero();
    if forward == Vec3::ZERO {
        return;
    }
    let (yaw, pitch) = crate::scene::yaw_pitch_from_forward(forward);
    let rotation = crate::systems::camera_look_rotation(yaw, pitch);
    if let Some(mut input) = world.try_resource_mut::<crate::components::InputState>() {
        input.yaw = yaw;
        input.pitch = pitch;
    }
    if let Some(transform) = world.get_mut::<Transform>(camera) {
        transform.translation = position;
        transform.rotation = rotation;
    }
    if let Some(transform) = world.get_mut::<GlobalTransform>(camera) {
        transform.translation = position;
        transform.rotation = rotation;
    }
}

fn valid_transform(value: TransformValue) -> bool {
    value.translation.into_iter().all(f32::is_finite)
        && value.rotation_degrees.into_iter().all(f32::is_finite)
        && value.scale.is_finite()
}

fn valid_material(value: MaterialValue) -> bool {
    value.diffuse_color.into_iter().all(f32::is_finite)
        && [value.metalness, value.roughness, value.alpha, value.ior]
            .into_iter()
            .all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_session(world: &mut World) {
        install_session(
            world,
            AssetSource {
                label: "fixture.nif".to_owned(),
            },
        );
    }

    fn place(world: &mut World, entities: &[EntityId]) -> AssetId {
        let id = {
            let mut session = world.resource_mut::<StudioSession>();
            let id = AssetId::new(session.next_asset).unwrap();
            session.next_asset += 1;
            session.assets.push(PlacedAsset {
                id,
                game: "fixture".to_owned(),
                path: format!("meshes\\probe{}.nif", id.get()),
                owner: 0,
                first: 0,
                last: 0,
                bounds: AssetBounds {
                    min: [0.0; 3],
                    max: [1.0; 3],
                },
                object_count: entities.len(),
            });
            id
        };
        bind_objects(world, id, entities);
        id
    }

    #[test]
    fn typed_transform_command_mutates_only_document_objects() {
        let mut world = World::new();
        let object = world.spawn();
        let outsider = world.spawn();
        world.insert(object, Transform::IDENTITY);
        world.insert(outsider, Transform::IDENTITY);
        fixture_session(&mut world);
        place(&mut world, &[object]);
        let object_id = ObjectId::new(1).unwrap();
        let outsider_id = ObjectId::new(2).unwrap();
        let value = TransformValue {
            translation: [1.0, 2.0, 3.0],
            rotation_degrees: [0.0, 90.0, 0.0],
            scale: 2.0,
        };
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                object: object_id,
                value,
            },
        );
        apply_command(
            &mut world,
            StudioCommand::SetTransform {
                object: outsider_id,
                value,
            },
        );

        let transform = world.get::<Transform>(object).unwrap();
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(transform.scale, 2.0);
        assert_eq!(
            world.get::<Transform>(outsider).unwrap().translation,
            Vec3::ZERO
        );
        assert_eq!(world.resource::<StudioSession>().revision, 1);
    }

    #[test]
    fn snapshot_exposes_document_ids_not_ecs_entity_ids() {
        let mut world = World::new();
        for _ in 0..8 {
            world.spawn();
        }
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        fixture_session(&mut world);
        place(&mut world, &[entity]);

        let snapshot = snapshot(&world).unwrap();
        assert_ne!(entity as u64, snapshot.objects[0].id.get());
        assert_eq!(snapshot.objects[0].id, ObjectId::new(1).unwrap());
        assert_eq!(snapshot.selected, Some(snapshot.objects[0].id));
        assert_eq!(snapshot.assets.len(), 1);
        assert_eq!(snapshot.assets[0].object_count, 1);
    }

    #[test]
    fn object_ids_continue_across_assets_and_removal_forgets_only_its_own() {
        let mut world = World::new();
        let entities: Vec<EntityId> = (0..3)
            .map(|_| {
                let entity = world.spawn();
                world.insert(entity, Transform::IDENTITY);
                entity
            })
            .collect();
        fixture_session(&mut world);
        let first = place(&mut world, &entities[..2]);
        let second = place(&mut world, &entities[2..]);
        {
            let session = world.resource::<StudioSession>();
            let ids: Vec<u64> = session.objects.iter().map(|b| b.id.get()).collect();
            assert_eq!(ids, vec![1, 2, 3]);
            assert_eq!(
                session.selected,
                ObjectId::new(3),
                "newest asset is selected"
            );
        }

        let removed = forget_asset(&world, second).expect("asset was placed");
        assert_eq!(removed.id, second);
        let session = world.resource::<StudioSession>();
        assert_eq!(session.objects.len(), 2);
        assert!(session.objects.iter().all(|binding| binding.asset == first));
        assert!(!session
            .original_transforms
            .contains_key(&ObjectId::new(3).unwrap()));
        assert_eq!(
            session.selected,
            ObjectId::new(1),
            "selection falls back to a surviving object"
        );
        drop(session);
        assert!(forget_asset(&world, second).is_none());
    }

    #[test]
    fn repack_closes_the_gap_a_removal_leaves() {
        let mut world = World::new();
        let left = world.spawn();
        let middle = world.spawn();
        let right = world.spawn();
        let end = world.spawn();
        for entity in [left, middle, right] {
            world.insert(entity, Transform::IDENTITY);
            world.insert(entity, GlobalTransform::IDENTITY);
        }
        fixture_session(&mut world);
        // Three unit-wide assets already laid out along X, each owning one
        // entity; the middle one is then removed.
        let mut ids = Vec::new();
        for (index, (first, last)) in [(left, middle), (middle, right), (right, end)]
            .into_iter()
            .enumerate()
        {
            let x = index as f32 * 10.0;
            let id = AssetId::new(index as u64 + 1).unwrap();
            world
                .resource_mut::<StudioSession>()
                .assets
                .push(PlacedAsset {
                    id,
                    game: "fixture".to_owned(),
                    path: format!("meshes\\probe{index}.nif"),
                    owner: 0,
                    first,
                    last,
                    bounds: AssetBounds {
                        min: [x - 0.5, 0.0, -0.5],
                        max: [x + 0.5, 1.0, 0.5],
                    },
                    object_count: 1,
                });
            ids.push(id);
        }
        world.get_mut::<Transform>(right).unwrap().translation.x = 20.0;
        forget_asset(&world, ids[1]).expect("middle asset was placed");

        repack_assets(&mut world);

        let session = world.resource::<StudioSession>();
        let right_bounds = session.assets[1].bounds;
        assert!(
            right_bounds.min[0] < 10.0,
            "the right asset slides into the gap: {right_bounds:?}"
        );
        assert_eq!(session.assets[0].bounds.center().x, 0.0);
        drop(session);
        let moved = world.get::<Transform>(right).unwrap().translation.x;
        assert!(moved < 20.0, "the entity moved with its bounds ({moved})");
    }

    #[test]
    fn placement_moves_hierarchy_roots_but_not_their_children() {
        let mut world = World::new();
        let root = world.spawn();
        let child = world.spawn();
        let loose_mesh = world.spawn();
        let outside = world.spawn();
        for entity in [root, child, loose_mesh, outside] {
            world.insert(entity, Transform::IDENTITY);
        }
        world.insert(child, Parent(root));
        translate_top_level(&mut world, root, outside, [1.0, 2.0, 3.0]);
        let moved = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(world.get::<Transform>(root).unwrap().translation, moved);
        assert_eq!(
            world.get::<Transform>(loose_mesh).unwrap().translation,
            moved
        );
        assert_eq!(
            world.get::<Transform>(child).unwrap().translation,
            Vec3::ZERO
        );
        assert_eq!(
            world.get::<Transform>(outside).unwrap().translation,
            Vec3::ZERO
        );
    }

    #[test]
    fn gallery_commands_queue_for_the_renderer_step() {
        let mut world = World::new();
        fixture_session(&mut world);
        apply_command(
            &mut world,
            StudioCommand::AddAsset {
                game: "fnv".to_owned(),
                path: r"meshes\clutter\bottle.nif".to_owned(),
            },
        );
        apply_command(&mut world, StudioCommand::ClearAssets);
        assert_eq!(world.resource::<StudioSession>().pending.len(), 2);

        apply_command(
            &mut world,
            StudioCommand::BrowseCatalog {
                game: "fnv".to_owned(),
                filter: String::new(),
            },
        );
        assert!(
            world.resource::<StudioSession>().status.is_some(),
            "browsing without archives reports why instead of panicking"
        );
    }

    /// #3445 (CONC-D3-2026-08-27b-03) — behavioral half of the fix: routing
    /// name resolution through the shared `resolve_entity_name` helper
    /// instead of a locally-held `StringPool` guard must not change the
    /// resolved name.
    #[test]
    fn snapshot_still_resolves_entity_names_through_the_shared_helper() {
        let mut world = World::new();
        world.insert_resource(byroredux_core::string::StringPool::new());
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        let symbol = {
            let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
            pool.intern("HeadHuman01")
        };
        world.insert(entity, byroredux_core::ecs::Name(symbol));
        fixture_session(&mut world);
        place(&mut world, &[entity]);

        let snapshot = snapshot(&world).unwrap();
        // `StringPool::intern` lowercases (Bethesda's own filesystem
        // case-insensitivity convention) — resolved name comes back
        // lowercase regardless of how it was authored.
        assert_eq!(snapshot.objects[0].name, "headhuman01");
    }

    /// #3445 (CONC-D3-2026-08-27b-03) — structural pin: `snapshot`'s own
    /// function body must never directly acquire `StringPool` or `Name`
    /// again. Pre-fix, `pool` was acquired once outside the per-entity
    /// closure and held across every entity's `Transform` AND `Name`
    /// reads, inverting the canonical `… → Name → StringPool` tail
    /// (#313) that `resolve_entity_name` and the debug evaluator both
    /// respect. The fix delegates to that shared helper instead, whose
    /// own guards are fully dropped before `snapshot` ever touches
    /// `Transform`/`Material` — so a correct `snapshot` body should
    /// contain neither `StringPool` nor a direct `Name` read at all.
    #[test]
    fn snapshot_body_does_not_directly_acquire_string_pool_or_name() {
        let src = include_str!("studio_host.rs");
        let start = src
            .find("pub(crate) fn snapshot(world: &World) -> Option<StudioSnapshot> {")
            .expect("snapshot must still exist with this signature");
        let rest = &src[start..];
        let end = rest
            .find("\npub(crate) fn ")
            .expect("no terminator found for snapshot");
        let body = &rest[..end];

        assert!(
            body.contains("resolve_entity_name"),
            "snapshot must resolve names through the shared canonical-order \
             helper, not re-derive the acquisition itself"
        );
        assert!(
            !body.contains("StringPool"),
            "snapshot must never directly acquire StringPool again — that's \
             the exact regression #3445 fixed"
        );
        assert!(
            !body.contains("::<byroredux_core::ecs::Name>") && !body.contains("::<Name>"),
            "snapshot must never directly acquire Name again — resolve_entity_name \
             already does, in the canonical order"
        );
    }
}
