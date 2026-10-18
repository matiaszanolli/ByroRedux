//! Interior cell loading + lighting resolution.
//!
//! `load_cell_with_masters` is the interior entry point — drives the
//! REFR walk, BLAS build, water plane, lighting resolution, and
//! cell-root stamping (by running [`InteriorCellApplyJob`] to completion,
//! #5589). The exterior entry point lives in
//! [`super::exterior`] and shares this module's `stamp_cell_root`
//! helper. `CellLoadResult` is the shape returned to the engine
//! caller.

use std::time::{Duration, Instant};

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellFormId, CellRoot, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::{CellLightingRes, CellRootIndex};

use super::load_order::{parse_record_indexes_in_load_order, LoadOrder};
use super::references::{ReferenceLoadJob, ReferenceLoadProgress};
use super::water;
use super::FrameTimeBudget;

/// Result of loading a cell.
/// Where the wall clock of one interior cell load went, by phase.
///
/// #3559 — measured per-frame tails show a single blocking first frame of
/// 29 s on FNV and 10 s on FO3 (p95s of 16.8 / 19.9 ms, so it is one frame,
/// not a distribution). Interior cell load runs on the render thread, and
/// under a windowed run that is a hung window rather than merely a slow
/// frame. The finding's own suggested fix names instrumenting *which* phase
/// owns it as the necessary first step, because the existing telemetry only
/// bounds the total.
///
/// Mirrors [`super::UnloadPhaseTimings`], which the exterior streaming path
/// already reports — the same phase-attribution shape, on the load side.
/// The synchronous entry point still drives all phases on the caller's
/// thread, while the interior door-transition path uses
/// [`InteriorCellApplyJob`] to chunk the reference phase against the shared
/// `STREAMING_APPLY_BUDGET`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellLoadPhaseTimings {
    /// `parse_record_indexes_in_load_order` — full-record parse of the
    /// master chain. Its own comment already budgets "~1 s extra to parse
    /// the surrounding categories on FNV / Skyrim", so it is the first
    /// suspect for the FNV number.
    pub esm_parse: Duration,
    /// FO4+ precombined-mesh spawn (`spawn_precombined_meshes`). Zero on
    /// every pre-FO4 title.
    pub precombined: Duration,
    /// `load_references` — the REFR walk: NIF import, texture resolve,
    /// collider build, BLAS build.
    pub references: Duration,
    /// Lighting resolution, region ambient, cell-root stamping, and the
    /// resource writes that close the load.
    pub finalization: Duration,
}

/// Stored on the world by every interior-load call site so the phase split
/// of the most recent load is readable after the fact (#3559).
impl byroredux_core::ecs::Resource for CellLoadPhaseTimings {}

impl CellLoadPhaseTimings {
    /// Total measured wall clock. Not necessarily the whole call: only the
    /// bracketed phases are counted, so a gap between this and an outer
    /// measurement is itself the signal that a phase is missing here.
    pub fn total(&self) -> Duration {
        self.esm_parse
            .saturating_add(self.precombined)
            .saturating_add(self.references)
            .saturating_add(self.finalization)
    }

    /// The single largest phase and its share of [`Self::total`], as
    /// `(name, fraction)`. `None` when nothing was measured.
    pub fn dominant(&self) -> Option<(&'static str, f32)> {
        let total = self.total().as_secs_f32();
        if total <= 0.0 {
            return None;
        }
        [
            ("esm_parse", self.esm_parse),
            ("precombined", self.precombined),
            ("references", self.references),
            ("finalization", self.finalization),
        ]
        .into_iter()
        .max_by_key(|(_, d)| *d)
        .map(|(name, d)| (name, d.as_secs_f32() / total))
    }
}

pub struct CellLoadResult {
    pub cell_name: String,
    pub entity_count: usize,
    /// Chosen spawn point (Y-up), used for initial camera/player
    /// positioning. Prefers the cell's first door's own placement — a
    /// guaranteed walkable threshold — over the bounding-box centroid of
    /// every placed REFR, which has no such guarantee and could land inside
    /// a wall, a stairwell void, or outside the interior shell entirely for
    /// L-shaped/multi-wing cells. See `references::load_references`.
    /// When [`Self::spawn_pose`] is `Some`, this is its position.
    pub center: Vec3,
    /// The authored floor-level pose a direct load (`coc`) places the player
    /// on — the cell's `COCMarkerHeading`, else a linked door's arrival
    /// `XTEL`. `None` when the cell authors neither; `center` then carries
    /// the loader's heuristic point. See `interior_spawn`.
    pub spawn_pose: Option<super::interior_spawn::SpawnPose>,
    /// Interior cell lighting (ambient + directional).
    pub lighting: Option<byroredux_plugin::esm::cell::CellLighting>,
    /// CELL DATA Show Sky / Behave Like Exterior bit.
    pub show_sky: bool,
    /// #4416 — the cell's `XCIM` image space, decoded (identity when the
    /// cell authors none, or the IMGS has no decodable grade).
    pub image_space: byroredux_scripting::ImageSpace,
    /// Resolved REGN ambient-sound directive (EX-16 item 1, #2372) — the
    /// cell's highest-priority tagging region's `Sound` entry, if any.
    /// `Default` (both fields `None`) when the cell has no `XCLR` regions
    /// or none of them author a `Sound` RDAT.
    pub region_ambient: crate::components::RegionAmbientRes,
    /// #3559 — per-phase wall clock for this load.
    pub phases: CellLoadPhaseTimings,
}

/// Ambient-only "no authored data" interior default — installed by
/// [`apply_interior_cell_lighting`] when `resolve_cell_lighting` returns
/// `None` (its case 3: no `XCLL`, no resolvable `LTMP`).
///
/// FNV-D1-01: pre-fix, all four interior-load call sites simply skipped
/// installing anything on `None`, silently leaving whatever
/// `CellLightingRes` the *previous* cell had installed — reproducing the
/// #1340/#1282 stale-lighting failure (wrong ambient/fog, exterior sun
/// leaking into a sealed interior) for this one input shape. Neutral gray
/// and zero directional contribution are a deliberate inert default, not
/// an approximation of any specific game's typical values — there is no
/// authored data here to approximate. Fog pushed far out of range mirrors
/// the same "effectively off" convention `cornell.rs`'s synthetic interior
/// uses. `is_interior` stays `true` so the #1282 gate still keeps a scene
/// directional out of the sealed cell.
const ENGINE_DEFAULT_INTERIOR_AMBIENT: [f32; 3] = [0.15, 0.15, 0.15];

fn engine_default_interior_lighting() -> CellLightingRes {
    // #5814 — the shared no-tail build; the ten None fields and the
    // medium fit live in CellLightingRes::bare.
    CellLightingRes::bare(
        ENGINE_DEFAULT_INTERIOR_AMBIENT,
        [0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        true,
        ENGINE_DEFAULT_INTERIOR_AMBIENT,
        100_000.0,
        1_000_000.0,
    )
}

/// Apply a freshly-loaded interior cell's resolved
/// [`CellLighting`](esm::cell::CellLighting) — or the engine default when
/// `resolve_cell_lighting` found none — to the renderer's `CellLightingRes`.
/// Shared by *every* interior-load entry point — the startup `--cell` path
/// (`scene.rs`), the M40 door-walk transition
/// ([`super::transition::InteriorCellApply`]),
/// the `cell.load` debug command (`debug_load.rs`), and the M45.1 live-load
/// apply (`save_io.rs`) — so they cannot drift.
///
/// Pre-#1340 only the startup path applied it, so an interior reached at
/// runtime rendered with the *previous* cell's `CellLightingRes`: wrong
/// ambient/fog, exterior clear color, and the directional sun leaking into
/// a sealed interior — the exact failure #1282 gated on `is_interior`.
/// Pre-FNV-D1-01, the `None` case reintroduced the same failure for cells
/// with neither `XCLL` nor a resolvable `LTMP` — every caller's
/// `if let Some(ref lit) = result.lighting { .. }` guard skipped the apply
/// entirely instead of installing anything, so a `None` inherited whatever
/// the previous cell left behind. Taking `Option` here and always
/// installing *something* is the fix — callers no longer branch on it.
///
/// For `Some`, routes the authored XCLL Euler angles through
/// XCLL stores azimuth and cyclic elevation, not a REFR Euler triple. The
/// dedicated spherical conversion preserves the authored horizontal angle;
/// routing it through the shared REFR helper discards azimuth because an
/// X-axis rotation cannot move the `(1,0,0)` model vector.
/// `is_interior` is always `true` — `load_cell_with_masters` only loads
/// interior cells, and the flag makes `CellLightingRes` skip the
/// directional as a scene light to prevent wall light leakage. The 9
/// extended XCLL fields (`fog_clip`, `directional_ambient`, …) are
/// propagated by `from_cell_lighting` (#861).
/// #4416 — a cell's `XCIM` image space, decoded through the IMGS index.
fn cell_image_space(
    cell: &esm::cell::CellData,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> byroredux_scripting::ImageSpace {
    cell.image_space_form
        .and_then(|form| index.image_spaces.get(&form))
        .and_then(|imgs| imgs.image_space)
        .unwrap_or_default()
}

pub(crate) fn apply_interior_cell_lighting(
    world: &mut World,
    lighting: Option<&esm::cell::CellLighting>,
    show_sky: bool,
    image_space: byroredux_scripting::ImageSpace,
) {
    let res = match lighting {
        Some(lit) => {
            let dir_v = xcll_direction_yup(lit.directional_azimuth, lit.directional_elevation);
            let dir = [dir_v.x, dir_v.y, dir_v.z];
            log::info!(
                "Cell lighting: ambient={:?} directional={:?} dir={:?} fog={:?} near={:.0} far={:.0}",
                lit.ambient,
                lit.directional_color,
                dir,
                lit.fog_color,
                lit.fog_near,
                lit.fog_far,
            );
            CellLightingRes::from_cell_lighting(lit, dir, true)
        }
        None => {
            log::info!(
                "Cell lighting: no XCLL and no resolvable LTMP — installing engine-default interior lighting"
            );
            engine_default_interior_lighting()
        }
    };
    world.insert_resource(res);
    world.insert_resource(crate::components::InteriorSkyExposureRes(show_sky));
    // #4902 — the outdoor sky an interior shows through windows, Show Sky and
    // apertures is a canonical resource, not a render-loop fallback.
    crate::scene::install_interior_outdoor_defaults(world);
    // #4416 — the interior's own base grade for the IMAD composition; the
    // weather system leaves it alone while `is_interior` holds.
    world.insert_resource(byroredux_scripting::ImageSpaceBase(image_space));
}

/// Convert XCLL's signed-degree azimuth/elevation pair into the renderer's
/// Y-up unit direction **toward** the directional source.
///
/// The on-disk elevation is cyclic: 0° is horizontal, 90° points below, and
/// 270° points overhead. That sign is pinned by FalloutNV.esm's 252 active
/// directionals: 96 use `(azimuth=0°, elevation=270°)`, the dominant authored
/// key-light preset. Source Z-up `(x,y,z)` becomes renderer Y-up `(x,z,-y)`.
pub(super) fn xcll_direction_yup(azimuth: f32, elevation: f32) -> Vec3 {
    let (sin_azimuth, cos_azimuth) = azimuth.sin_cos();
    let (sin_elevation, cos_elevation) = elevation.sin_cos();
    Vec3::new(
        cos_elevation * cos_azimuth,
        -sin_elevation,
        -cos_elevation * sin_azimuth,
    )
}

/// Surface parsed `GLOB` runtime values as the `Globals` `World` resource,
/// but only when it isn't already present (#1668, #1865 / SCR-D6-NEW-03).
///
/// Shared by every cell-load entry point (this module's interior path and
/// [`super::exterior::load_one_exterior_cell`]) so they can't drift back
/// into disagreement. Rebuilding unconditionally on every load would
/// silently discard any runtime `Globals::set` mutation the moment another
/// cell loads afterward — dormant today (no production writer exists yet)
/// but a landmine for the pending `SetGlobalValue` Papyrus writer.
pub(crate) fn ensure_globals_resource(
    world: &mut World,
    records: &std::collections::HashMap<u32, byroredux_plugin::esm::records::GlobalRecord>,
) {
    if world
        .try_resource::<byroredux_scripting::globals::Globals>()
        .is_none()
    {
        world.insert_resource(byroredux_scripting::globals::Globals::from_records(records));
        world.insert_resource(byroredux_scripting::globals::GameClockGlobals::from_records(records));
    }
}

pub(crate) fn stamp_cell_root(
    world: &mut World,
    cell_root: EntityId,
    first: EntityId,
    last: EntityId,
) {
    register_cell_root(world, cell_root);
    stamp_cell_root_range(world, cell_root, first, last);
}

/// Register a cell root before a resumable load starts mutating the world.
///
/// Incremental exterior application needs a reclaim handle as soon as its
/// first terrain/reference entity is spawned. Registering the root up front
/// lets cancellation use the normal [`super::unload_cell`] path even when the
/// cell has only been partially applied.
pub(crate) fn register_cell_root(world: &mut World, cell_root: EntityId) {
    world.insert(cell_root, CellRoot(cell_root));
    if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
        idx.map.entry(cell_root).or_default().push(cell_root);
    }
}

/// Stamp one newly-created entity range onto an already registered cell root.
///
/// `first..last` is deliberately half-open and may be empty. The resumable
/// exterior loader calls this after every cooperative slice; the synchronous
/// loaders call it once through [`stamp_cell_root`].
pub(crate) fn stamp_cell_root_range(
    world: &mut World,
    cell_root: EntityId,
    first: EntityId,
    last: EntityId,
) {
    // Every spawned entity in `first..last` gets a `CellRoot` row
    // regardless of whether it received any other components. The unload
    // path filters `CellRoot` storage by `cell_root`, so this stamp is
    // what makes the entity reachable from `unload_cell` (post-#791,
    // also via the `CellRootIndex` populated below).
    //
    // #3388 — one component type over a contiguous entity range is
    // exactly the shape `insert_batch` exists for, so the `TypeId`
    // lookups, `RwLock::get_mut` and `downcast_mut` of `World::insert`'s
    // preamble are paid once instead of per entity. `SparseSetStorage`
    // takes the default `insert_bulk` (a loop of `insert`), so the
    // per-entity semantics — including overwrite-safety, which the
    // interior loader's re-stamp relies on — are unchanged. The index
    // half below has been batched since #885.
    world.insert_batch((first..last).map(|eid| (eid, CellRoot(cell_root))));
    // Populate the inverted index. Production always registers the
    // resource at App init (`main.rs:258`); test fixtures that drive
    // stamp_cell_root through reduced setups may not. Skip silently in
    // that case — `unload_cell` will also skip and fall through to an
    // empty victim set, which is the same observable behaviour the
    // unload path had pre-#791 for cells whose query found no rows.
    if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
        let entry = idx.map.entry(cell_root).or_insert_with(Vec::new);
        let span = last.saturating_sub(first) as usize;
        entry.reserve(span);
        // `extend` over a known-size `Copy` range lets the compiler
        // inline as a typed memcpy and elide per-push bounds checks
        // — same final layout as the prior per-eid push loop. #885.
        entry.extend(first..last);
    }
}

/// Load an interior cell with explicit master plugins.
///
/// `masters` is an ordered list of master ESM paths (base game first,
/// then any required DLC masters); `esm_path` is the main plugin
/// being loaded (DLC or mod). Each plugin's FormIDs are remapped to
/// global load-order indices before being merged into a single cell
/// index, so cross-plugin REFRs (e.g. a Dawnguard interior placing a
/// Skyrim.esm STAT) resolve correctly.
///
/// Pre-#561 the cell loader only accepted a single ESM and silently
/// rendered empty interiors when REFRs pointed into a missing master.
/// This entry point closes the audit's SK-D6-01 gap by threading
/// `parse_esm_with_load_order` through the cell-loader pipeline.
///
/// On unresolved REFR `base_form_id` lookups, the warning summary now
/// names the missing plugin so the failure mode is diagnosable
/// instead of silent. See M46.0 / #561.
/// SAVE-D6-02 — non-destructive pre-flight for a live load.
///
/// Parses the plugin set and confirms `cell_editor_id` resolves, WITHOUT
/// mutating any world or GPU state. Mirrors the first two phases of
/// [`load_cell_with_masters`] (parse → cell lookup); those are exactly the
/// two phases the live `load <slot>` failure modes hit (missing/corrupt
/// ESM, renamed/absent cell editor id), and both are non-destructive in the
/// full loader too. The caller runs this *before* tearing down the running
/// cell, so a reload that can't succeed keeps the current cell instead of
/// stranding the player in an empty world.
///
/// The full [`load_cell_with_masters`] re-parses (a few seconds, paid once
/// per user-initiated load) — cheap insurance against an unrecoverable
/// session. Returns `Ok(())` when the cell is loadable.
pub fn validate_cell_loadable(
    masters: &[String],
    esm_path: &str,
    cell_editor_id: &str,
) -> anyhow::Result<()> {
    let plugin_paths: Vec<&str> = masters
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(esm_path))
        .collect();
    let (index, _load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
    let cell_key = cell_editor_id.to_ascii_lowercase();
    anyhow::ensure!(
        index.cells.cells.contains_key(&cell_key),
        "Cell '{}' not found among {} interior cells in the saved plugin set",
        cell_editor_id,
        index.cells.cells.len(),
    );
    Ok(())
}

#[tracing::instrument(
    name = "load_cell_with_masters",
    skip_all,
    fields(esm = esm_path, cell = cell_editor_id, master_count = masters.len()),
)]
pub fn load_cell_with_masters(
    masters: &[String],
    esm_path: &str,
    cell_editor_id: &str,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
) -> anyhow::Result<CellLoadResult> {
    // The synchronous load is the resumable job run to completion under an
    // unlimited budget (#5589). It used to be a ~54% line-for-line twin of
    // `InteriorCellApplyJob` — plugin chaining, cell lookup, precombined,
    // water, reference load, lighting, spawn pose, region music, resolver
    // install, phase log — and every interior-load fix (#4416, #4909, #5484)
    // had to land in both copies. `load_references` was already an
    // unlimited-budget wrapper over the budgeted loader the job drives, so
    // the shared path is the same reference loader either way.
    let mut job = InteriorCellApplyJob::begin(
        masters,
        esm_path,
        cell_editor_id,
        world,
        ctx,
        tex_provider,
        mat_provider.as_deref_mut(),
    )?;
    loop {
        let mut budget = FrameTimeBudget::unlimited();
        match job.advance(
            world,
            ctx,
            tex_provider,
            mat_provider.as_deref_mut(),
            &mut budget,
        ) {
            InteriorCellApplyProgress::Complete(result) => return Ok(result),
            // An unlimited budget never yields; looping keeps this correct
            // (rather than `unreachable!`) if that ever changes.
            InteriorCellApplyProgress::Pending(pending) => job = pending,
        }
    }
}

/// The interior cell `cell_editor_id` names, or a self-diagnosing error.
///
/// Phase 20.2 — when the requested cell doesn't exist, the error lists the
/// cells whose editor ID contains the requested name (case-insensitive), which
/// turns "cell not found" into a typo/suffix diagnostic; failing that, a
/// 4-character prefix match (the prefix right, the suffix wrong — `InstAdvSys01`
/// for `InstSRBLab02`); failing that, the first 20 cells.
fn find_interior_cell<'a>(
    index: &'a esm::records::EsmIndex,
    cell_editor_id: &str,
) -> anyhow::Result<&'a esm::cell::CellData> {
    let cell_key = cell_editor_id.to_ascii_lowercase();
    index.cells.cells.get(&cell_key).ok_or_else(|| {
        let needle = cell_key.as_str();
        let matches: Vec<&str> = index
            .cells
            .cells
            .values()
            .filter(|c| c.editor_id.to_ascii_lowercase().contains(needle))
            .take(20)
            .map(|c| c.editor_id.as_str())
            .collect();
        let (label, examples) = if matches.is_empty() {
            let prefix_len = needle.len().min(4);
            let prefix = &needle[..prefix_len];
            let prefix_matches: Vec<&str> = index
                .cells
                .cells
                .values()
                .filter(|c| c.editor_id.to_ascii_lowercase().starts_with(prefix))
                .take(20)
                .map(|c| c.editor_id.as_str())
                .collect();
            if prefix_matches.is_empty() {
                let any: Vec<&str> = index
                    .cells
                    .cells
                    .values()
                    .take(20)
                    .map(|c| c.editor_id.as_str())
                    .collect();
                ("first 20 cells", any)
            } else {
                ("cells matching prefix", prefix_matches)
            }
        } else {
            ("cells containing substring", matches)
        };
        anyhow::anyhow!(
            "Cell '{}' not found. {} interior cells available. {} ({}): {:?}",
            cell_editor_id,
            index.cells.cells.len(),
            label,
            examples.len(),
            examples,
        )
    })
}

/// Resumable main-thread application of one interior cell.
///
/// The existing public [`load_cell_with_masters`] path remains synchronous,
/// but a door transition can retain this job between frames. ESM parsing and
/// the atomic precombined/water prefix happen in [`Self::begin`]; the large
/// REFR phase advances through the same cursor as exterior streaming, so
/// SCOL/PKIN expansion and multi-NIF NPC spawns do not monopolize one frame.
/// Every slice stamps its newly-created entities onto the root, making a
/// cancelled partial load reclaimable through the normal unload path.
pub(crate) struct InteriorCellApplyJob {
    masters: Vec<String>,
    esm_path: String,
    cell_editor_id: String,
    index: esm::records::EsmIndex,
    load_order: LoadOrder,
    cell_root: EntityId,
    /// #5484 — hashes whose `_oc.nif` bake drew geometry. Keys
    /// `CellData::absorbed_ref_bakes` at the reference gate.
    baked_hashes: Vec<u32>,
    references: Option<Box<ReferenceLoadJob>>,
    phases: CellLoadPhaseTimings,
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum InteriorCellApplyProgress {
    Pending(InteriorCellApplyJob),
    Complete(CellLoadResult),
}

impl InteriorCellApplyJob {
    /// Parse and install the atomic prefix of an interior load.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin(
        masters: &[String],
        esm_path: &str,
        cell_editor_id: &str,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
    ) -> anyhow::Result<Self> {
        let plugin_paths: Vec<&str> = masters
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(esm_path))
            .collect();
        let mut phases = CellLoadPhaseTimings::default();
        let phase_started = Instant::now();
        let (index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
        phases.esm_parse = phase_started.elapsed();

        let cell = find_interior_cell(&index, cell_editor_id)?;
        // M47.3 — the references below attach their ObScript object scripts as they
        // spawn, so the tables must exist first (`populate_scene_runtime` is too late).
        crate::asset_provider::ensure_obscript_tables(world, &index);

        log::info!(
            "Loading cell '{}' (form {:08X}): {} placed references",
            cell.editor_id,
            cell.form_id,
            cell.references.len(),
        );

        // Register the reclaim root before any GPU/ECS work. This mirrors
        // ExteriorCellApplyJob::begin and makes even a first-slice cancel
        // safe.
        let cell_root = world.spawn();
        register_cell_root(world, cell_root);
        world.insert(cell_root, CellFormId(cell.form_id));
        let prefix_first = world.next_entity_id();

        let phase_started = Instant::now();
        let (_pc_spawned, _pc_misses, baked_hashes) = super::precombined::spawn_precombined_meshes(
            cell,
            Vec3::ZERO,
            world,
            ctx,
            tex_provider,
            mat_provider,
            esm_path,
            &plugin_paths,
        );
        phases.precombined = phase_started.elapsed();

        // Keep water in the atomic prefix. Its texture resolve is flushed by
        // the reference phase's completion path, so it must be present before
        // that phase can finish.
        if let Some(water_height) = water::interior_water_height(cell.water_height) {
            let (water_center, water_half_extent) = water::interior_water_placement(
                cell.references.iter().map(|reference| reference.position),
            );
            if water::spawn_water_plane(
                world,
                ctx,
                tex_provider,
                &index.waters,
                water_height,
                cell.water_type_form,
                water_center,
                water_half_extent,
                None,
                index.game,
            )
            .is_none()
            {
                log::warn!(
                    "  Cell '{}': water plane spawn failed — no water will render",
                    cell.editor_id
                );
            }
        }
        // Entity IDs are monotonic, so this range covers only the entities
        // created by the precombined/water prefix, not the root itself.
        stamp_cell_root_range(world, cell_root, prefix_first, world.next_entity_id());

        Ok(Self {
            masters: masters.to_vec(),
            esm_path: esm_path.to_string(),
            cell_editor_id: cell_editor_id.to_string(),
            index,
            load_order,
            cell_root,
            baked_hashes,
            references: None,
            phases,
        })
    }

    /// Advance the REFR phase against the caller's frame deadline.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance(
        mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> InteriorCellApplyProgress {
        let cell_key = self.cell_editor_id.to_ascii_lowercase();
        let cell = self
            .index
            .cells
            .cells
            .get(&cell_key)
            .expect("interior cell existed when its apply job began");
        let absorbed =
            super::precombined::effective_absorbed_refs(&cell.absorbed_ref_bakes, &self.baked_hashes);
        let first_entity = world.next_entity_id();
        let phase_started = Instant::now();
        let progress = super::references::load_references_budgeted(
            &cell.references,
            &self.index.cells,
            &self.index,
            &self.index.races,
            self.index.game,
            world,
            ctx,
            tex_provider,
            mat_provider,
            &self.cell_editor_id,
            &self.load_order,
            &absorbed,
            self.references.take(),
            budget,
        );
        self.phases.references = self
            .phases
            .references
            .saturating_add(phase_started.elapsed());
        let last_entity = world.next_entity_id();
        stamp_cell_root_range(world, self.cell_root, first_entity, last_entity);
        // #4699 — CELL `XOWN` for placements with no ownership of their own.
        crate::inventory::stamp_cell_ownership(
            world,
            cell.ownership.as_ref(),
            first_entity,
            last_entity,
        );

        match progress {
            ReferenceLoadProgress::Pending(references) => {
                self.references = Some(references);
                super::references::flush_pending_cell_textures_on_yield(ctx);
                InteriorCellApplyProgress::Pending(self)
            }
            ReferenceLoadProgress::Complete(result) => {
                self.finish(world, ctx, tex_provider, result)
            }
        }
    }

    /// Reclaim all ECS/GPU work produced by an unfinished interior load.
    pub(crate) fn cancel(mut self, world: &mut World, ctx: &mut VulkanContext) {
        if let Some(references) = self.references.take() {
            references.cancel(world);
        }
        super::unload_cell(world, ctx, self.cell_root);
    }

    fn finish(
        self,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        result: super::references::RefLoadResult,
    ) -> InteriorCellApplyProgress {
        let InteriorCellApplyJob {
            masters,
            esm_path,
            cell_editor_id,
            index,
            load_order,
            cell_root,
            mut phases,
            ..
        } = self;
        let cell_key = cell_editor_id.to_ascii_lowercase();
        let cell = index
            .cells
            .cells
            .get(&cell_key)
            .expect("interior cell existed when its apply job began");
        let phase_started = Instant::now();
        let resolved_lighting = resolve_cell_lighting(cell, &index);
        let image_space = cell_image_space(cell, &index);
        let show_sky = cell.show_sky.unwrap_or(false);
        crate::scene::apply_show_sky_interior_climate(
            world,
            ctx,
            tex_provider,
            &index,
            cell,
        );
        log::info!("Cell lighting: {:?}", resolved_lighting);

        let navmesh_first = world.next_entity_id();
        crate::components::spawn_navmesh_tiles(world, &cell.navmeshes);
        stamp_cell_root_range(world, cell_root, navmesh_first, world.next_entity_id());
        crate::asset_provider::populate_scene_runtime(world, &index);
        crate::asset_provider::populate_idle_clip_runtime(world, &index, tex_provider);
        crate::asset_provider::populate_object_graphs(world, tex_provider);
        crate::asset_provider::populate_skyrim_walk_clip(world, &index, tex_provider);
        crate::asset_provider::populate_archetype_idles(world, &index, tex_provider);
        crate::asset_provider::populate_draugr_combat_clips(world, &index, tex_provider);

        let cell_name = cell.editor_id.clone();
        let entity_count = result.entity_count;
        let (center, spawn_pose) = super::interior_spawn::resolve_interior_spawn(
            &cell.references,
            &index.cells,
            result.center,
            &cell_name,
        );
        let region_ambient =
            crate::components::RegionAmbientRes::resolve(&cell.regions, &index.regions);
        let previous_music_form = world
            .try_resource::<crate::components::RegionAmbientRes>()
            .and_then(|r| r.music_form);
        if previous_music_form != region_ambient.music_form {
            crate::asset_provider::dispatch_region_ambient_music(
                world,
                &index.sounds,
                region_ambient.music_form,
            );
        }

        ensure_globals_resource(world, &index.globals);
        let form_resolver =
            super::load_order::GlobalFormIdResolver::from_load_order_with_records_and_factions(
                &load_order,
                &index.record_types,
                &index.factions,
            );
        world.insert_resource(super::LoadedCellIndex(std::sync::Arc::new(index)));
        super::load_order::install_form_resolver(
            world,
            form_resolver,
            std::sync::Arc::new(load_order),
        );
        world.insert_resource(super::CurrentCellRoot(Some(cell_root)));
        world.insert_resource(super::CurrentCellContext {
            cell_editor_id: cell_editor_id.clone(),
            esm_path,
            masters,
        });

        debug_assert_eq!(
            ctx.texture_registry.pending_dds_upload_count(),
            0,
            "interior cell load left DDS uploads unflushed — a resolve_texture \
             call has moved below load_references' flush (#3320)"
        );

        phases.finalization = phase_started.elapsed();
        log::info!(
            "Cell load phases: esm_parse={:.3}s precombined={:.3}s references={:.3}s \
             finalization={:.3}s total={:.3}s dominant={}",
            phases.esm_parse.as_secs_f32(),
            phases.precombined.as_secs_f32(),
            phases.references.as_secs_f32(),
            phases.finalization.as_secs_f32(),
            phases.total().as_secs_f32(),
            phases
                .dominant()
                .map(|(name, share)| format!("{name} ({:.0}%)", share * 100.0))
                .unwrap_or_else(|| "none".to_string()),
        );

        InteriorCellApplyProgress::Complete(CellLoadResult {
            cell_name,
            entity_count,
            center,
            spawn_pose,
            lighting: resolved_lighting,
            show_sky,
            image_space,
            region_ambient,
            phases,
        })
    }
}

const XCLL_INHERIT_AMBIENT: u32 = 0x001;
const XCLL_INHERIT_DIRECTIONAL_COLOR: u32 = 0x002;
const XCLL_INHERIT_FOG_COLOR: u32 = 0x004;
const XCLL_INHERIT_FOG_NEAR: u32 = 0x008;
const XCLL_INHERIT_FOG_FAR: u32 = 0x010;
const XCLL_INHERIT_DIRECTIONAL_ROTATION: u32 = 0x020;
const XCLL_INHERIT_DIRECTIONAL_FADE: u32 = 0x040;
const XCLL_INHERIT_FOG_CLIP: u32 = 0x080;
const XCLL_INHERIT_FOG_POWER: u32 = 0x100;
const XCLL_INHERIT_FOG_MAX: u32 = 0x200;
const XCLL_INHERIT_LIGHT_FADE: u32 = 0x400;

fn lighting_from_template(template: &esm::records::LgtmRecord) -> esm::cell::CellLighting {
    esm::cell::CellLighting {
        ambient: template.ambient,
        directional_color: template.directional,
        directional_azimuth: template.directional_rotation[0],
        directional_elevation: template.directional_rotation[1],
        fog_color: template.fog_color,
        fog_near: template.fog_near,
        fog_far: template.fog_far,
        directional_fade: template.directional_fade,
        fog_clip: template.fog_clip,
        fog_power: template.fog_power,
        fog_far_color: template.fog_far_color,
        fog_max: template.fog_max,
        light_fade_begin: template.light_fade_begin,
        light_fade_end: template.light_fade_end,
        directional_ambient: template.directional_ambient,
        specular_color: template.specular_color,
        specular_alpha: template.specular_alpha,
        fresnel_power: template.fresnel_power,
        inheritance_flags: None,
        // #5002 — Starfield LGTM DATA carries the same height-fog model
        // as XCLL and is decoded at the parser boundary; forward it so a
        // plugin CELL with LTMP and no XCLL keeps its authored heights.
        starfield: template.starfield.clone(),
    }
}

/// Apply Skyrim+'s XCLL `Inherits` mask. Optional values replace local
/// values only when the LGTM actually carries that field, preserving valid
/// local data when an older/short template lacks the extended layout.
fn inherit_lighting_fields(
    local: &mut esm::cell::CellLighting,
    template: &esm::records::LgtmRecord,
    flags: u32,
) {
    if flags & XCLL_INHERIT_AMBIENT != 0 {
        local.ambient = template.ambient;
        if template.directional_ambient.is_some() {
            local.directional_ambient = template.directional_ambient;
        }
        if template.specular_color.is_some() {
            local.specular_color = template.specular_color;
        }
        if template.specular_alpha.is_some() {
            local.specular_alpha = template.specular_alpha;
        }
        if template.fresnel_power.is_some() {
            local.fresnel_power = template.fresnel_power;
        }
    }
    if flags & XCLL_INHERIT_DIRECTIONAL_COLOR != 0 {
        local.directional_color = template.directional;
    }
    if flags & XCLL_INHERIT_FOG_COLOR != 0 {
        local.fog_color = template.fog_color;
        if template.fog_far_color.is_some() {
            local.fog_far_color = template.fog_far_color;
        }
    }
    if flags & XCLL_INHERIT_FOG_NEAR != 0 {
        local.fog_near = template.fog_near;
    }
    if flags & XCLL_INHERIT_FOG_FAR != 0 {
        local.fog_far = template.fog_far;
    }
    if flags & XCLL_INHERIT_DIRECTIONAL_ROTATION != 0 {
        local.directional_azimuth = template.directional_rotation[0];
        local.directional_elevation = template.directional_rotation[1];
    }
    if flags & XCLL_INHERIT_DIRECTIONAL_FADE != 0 && template.directional_fade.is_some() {
        local.directional_fade = template.directional_fade;
    }
    if flags & XCLL_INHERIT_FOG_CLIP != 0 && template.fog_clip.is_some() {
        local.fog_clip = template.fog_clip;
    }
    if flags & XCLL_INHERIT_FOG_POWER != 0 && template.fog_power.is_some() {
        local.fog_power = template.fog_power;
    }
    if flags & XCLL_INHERIT_FOG_MAX != 0 && template.fog_max.is_some() {
        local.fog_max = template.fog_max;
    }
    if flags & XCLL_INHERIT_LIGHT_FADE != 0 {
        if template.light_fade_begin.is_some() {
            local.light_fade_begin = template.light_fade_begin;
        }
        if template.light_fade_end.is_some() {
            local.light_fade_end = template.light_fade_end;
        }
    }
}

/// Resolve the CELL XCLL / LTMP chain before renderer translation.
///
/// Skyrim+ XCLL is a partial override: its final `Inherits` word chooses
/// field groups from LTMP. Pre-Skyrim XCLL has no mask and remains a full
/// override. A missing XCLL synthesizes all available fields from LTMP;
/// no resolvable authoring returns `None` for the engine-default caller.
pub(crate) fn resolve_cell_lighting(
    cell: &esm::cell::CellData,
    index: &esm::records::EsmIndex,
) -> Option<esm::cell::CellLighting> {
    if let Some(mut lit) = cell.lighting.clone() {
        let flags = lit.inheritance_flags.unwrap_or(0);
        if flags != 0 {
            if let Some(template) = cell
                .lighting_template_form
                .and_then(|form| index.lighting_templates.get(&form))
            {
                inherit_lighting_fields(&mut lit, template, flags);
            }
        }
        return Some(lit);
    }
    let template_form = cell.lighting_template_form?;
    let template = index.lighting_templates.get(&template_form)?;
    Some(lighting_from_template(template))
}

#[cfg(test)]
mod stamp_cell_root_range_tests {
    use super::*;
    use crate::components::CellRootIndex;
    use byroredux_core::ecs::World;

    fn world_with_index() -> World {
        let mut world = World::new();
        world.insert_resource(CellRootIndex::new());
        world
    }

    /// #3388 — the switch from a per-entity `world.insert` loop to
    /// `insert_batch` must leave the component half's coverage
    /// untouched: every entity in the half-open range gets a `CellRoot`
    /// row, and nothing outside it does. That row is what makes the
    /// entity reachable from `unload_cell`, so a gap here is a leak.
    #[test]
    fn every_entity_in_the_range_gets_a_cell_root_row() {
        let mut world = world_with_index();
        let root = world.spawn();
        let first = world.next_entity_id();
        let inside: Vec<_> = (0..5).map(|_| world.spawn()).collect();
        let last = world.next_entity_id();
        let outside = world.spawn();

        stamp_cell_root_range(&mut world, root, first, last);

        for eid in &inside {
            assert_eq!(
                world.get::<CellRoot>(*eid).map(|c| c.0),
                Some(root),
                "entity {eid} in [{first},{last}) was not stamped",
            );
        }
        assert!(
            world.get::<CellRoot>(outside).is_none(),
            "an entity past `last` must not be claimed by the cell",
        );
    }

    /// `insert_batch` dispatches to `insert_bulk`, which `SparseSetStorage`
    /// takes at its default (a loop of `insert`) — so the overwrite
    /// semantics the interior loader's re-stamp relies on survive the
    /// batching. Last writer wins.
    #[test]
    fn a_later_stamp_overwrites_an_earlier_roots_claim() {
        let mut world = world_with_index();
        let first_root = world.spawn();
        let second_root = world.spawn();
        let first = world.next_entity_id();
        let entity = world.spawn();
        let last = world.next_entity_id();

        stamp_cell_root_range(&mut world, first_root, first, last);
        assert_eq!(world.get::<CellRoot>(entity).map(|c| c.0), Some(first_root));

        stamp_cell_root_range(&mut world, second_root, first, last);
        assert_eq!(
            world.get::<CellRoot>(entity).map(|c| c.0),
            Some(second_root),
            "re-stamping must overwrite, not keep the first claim",
        );
    }

    /// The range is documented as possibly empty — the resumable loader
    /// calls this after a slice that spawned nothing. Neither half may
    /// do anything then.
    #[test]
    fn an_empty_range_stamps_nothing() {
        let mut world = world_with_index();
        let root = world.spawn();
        let boundary = world.next_entity_id();

        stamp_cell_root_range(&mut world, root, boundary, boundary);

        assert!(
            world
                .resource::<CellRootIndex>()
                .map
                .get(&root)
                .is_none_or(Vec::is_empty),
            "an empty range must not add index rows",
        );
    }
}

#[cfg(test)]
mod tests {
    /// #4551 — every route that installs the per-cell animation clip
    /// families must install ALL of them: the `--cell` route carried only
    /// the idle + walk installers, so the P2 combat takes silently no-op
    /// on the route every smoke test drives (the spawn finalize inserts
    /// the `DraugrCombatAnim` marker regardless). Source-level pin in the
    /// established include_str! pattern — driving the installer live
    /// needs real archives; counting the calls needs only the sources.
    ///
    /// #4635 — the count runs over `#[cfg(test)]`-stripped source (the
    /// `material_translate.rs` meta-guard cut): the needle literals used
    /// to self-count inside this test body, so a route deleted whole
    /// (walk + combat together) read as "no regression". Counting the
    /// bare installer names also covers the exterior route
    /// (`scene/world_setup.rs`), whose call sites spell their arguments
    /// differently. The per-file floor is today's real production count:
    /// one interior site in `load.rs` — `InteriorCellApplyJob::finish`, which
    /// the synchronous `load_cell_with_masters` now shares (#5589) — and one in
    /// world setup.
    #[test]
    fn every_animation_route_installs_the_combat_family_too() {
        // #5824 — the strong shared cut (asserts the test module shape).
        let production = |name: &str, src: &'static str| -> &'static str {
            src.split_once("\n#[cfg(test)]\nmod ")
                .map(|(prod, _)| prod)
                .unwrap_or_else(|| {
                    panic!("{name} lost its `#[cfg(test)]` cut point — the meta-guard needs it")
                })
        };
        for (name, whole_file, floor) in [
            ("cell_loader/load.rs", include_str!("load.rs"), 1),
            (
                "scene/world_setup.rs",
                include_str!("../scene/world_setup.rs"),
                1,
            ),
        ] {
            let source = production(name, whole_file);
            let walk = source.matches("populate_skyrim_walk_clip").count();
            let combat = source.matches("populate_draugr_combat_clips").count();
            assert_eq!(
                walk, combat,
                "{name}: each populate_skyrim_walk_clip install site must have the \
                 populate_draugr_combat_clips sibling (#4551) — a route that \
                 spawns the DraugrCombatAnim marker without the clip resource \
                 makes the P2 combat takes silently no-op"
            );
            assert!(
                walk >= floor,
                "{name}: floor of {floor} install site(s) broken — a route \
                 deleted whole is the bug this pin exists for (#4635)"
            );
        }
    }

    /// #3671 — the resumable interior path must use the existing reference
    /// cursor and stamp every yielded slice before returning it to App. The
    /// source check keeps this invariant CI-reachable without requiring a
    /// Vulkan device or shipped ESM archives.
    #[test]
    fn interior_apply_job_is_budgeted_and_reclaimable() {
        let source = include_str!("load.rs");
        let job_start = source
            .find("pub(crate) struct InteriorCellApplyJob")
            .expect("the interior apply job must exist");
        let body = &source[job_start..];
        let advance_start = body
            .find("pub(crate) fn advance(")
            .expect("the interior apply job must expose advance");
        let body = &body[advance_start..];
        assert!(
            body.contains("load_references_budgeted("),
            "interior apply must share the resumable reference cursor (#3671)"
        );
        assert!(
            body.contains("stamp_cell_root_range(world, self.cell_root, first_entity"),
            "each interior slice must stamp its newly-created entity range (#3671)"
        );
        assert!(
            source.contains("references.cancel(world)") && source.contains("unload_cell(world, ctx, self.cell_root)"),
            "cancelling an interior job must release both reference continuations and its cell root (#3671)"
        );
    }

    /// #3320 — pin the load-order invariant that makes interior water render.
    ///
    /// `spawn_water_plane` reserves a bindless slot via `resolve_texture` and
    /// leaves its descriptor on the fallback checkerboard until a batched
    /// flush; the engine has no per-frame flush, and an interior load's only
    /// flush is the forced one at `load_references`' tail. Spawning the water
    /// plane *after* that call therefore bound the WATR normal map to the
    /// magenta checkerboard for the whole life of the cell.
    ///
    /// The runtime guard for this is the `debug_assert_eq!` on
    /// `pending_dds_upload_count()` at the end of `InteriorCellApplyJob::finish`
    /// (the one interior path, #5589),
    /// but nothing in `cargo test` loads a cell (it needs a Vulkan device and
    /// on-disk game data), so that assertion never executes in CI. This is
    /// the CI-reachable half: it reads the production source and requires
    /// the water spawn (in `begin`) to still precede the reference load (in
    /// `advance`). It is a coarse check by design — it exists because the
    /// alternative is no check at all. Scanned over production text only: the
    /// needles below would otherwise match this test's own body.
    #[test]
    fn interior_water_spawns_before_the_reference_load_flush() {
        let source = byroredux_core::source_scan::production_text(include_str!("load.rs"));
        let water = source
            .find("water::spawn_water_plane(")
            .expect("load.rs must still spawn an interior water plane");
        let references = source
            .find("load_references_budgeted(")
            .expect("load.rs must still load placed references");
        assert!(
            water < references,
            "the interior water plane is spawned after the reference load, whose \
             tail carries the cell load's only texture flush — its WATR normal \
             map will render as the fallback checkerboard (#3320)"
        );
    }

    /// #5589 — the synchronous interior load is `InteriorCellApplyJob` run to
    /// completion, not a second copy of it: plugin chaining, the cell lookup
    /// and its suggestion error, precombined, water, references, lighting,
    /// spawn pose, region music and resolver install each live in ONE place.
    #[test]
    fn the_synchronous_interior_load_runs_the_apply_job() {
        let source = byroredux_core::source_scan::production_text(include_str!("load.rs"));
        let start = source
            .find("pub fn load_cell_with_masters(")
            .expect("load_cell_with_masters must exist");
        let end = source[start..]
            .find("\n}\n")
            .map(|i| start + i)
            .expect("the function body must close");
        let body = &source[start..end];
        assert!(
            body.contains("InteriorCellApplyJob::begin(") && body.contains(".advance("),
            "load_cell_with_masters must drive InteriorCellApplyJob (#5589)"
        );
        for twin in [
            "spawn_precombined_meshes(",
            "spawn_water_plane(",
            "resolve_cell_lighting(",
            "install_form_resolver(",
        ] {
            assert!(
                !body.contains(twin),
                "load_cell_with_masters calls `{twin}` itself again — that step belongs to \
                 InteriorCellApplyJob only (#5589)"
            );
        }
        assert_eq!(
            source.matches("interior cells available").count(),
            1,
            "the cell-not-found diagnostic lives only in `find_interior_cell` (#5589)"
        );
    }

    use super::*;

    /// SAVE-D6-02 — the live-load pre-flight must FAIL (not panic) when the
    /// saved plugin set can't be read, so the drain catches it BEFORE tearing
    /// down the running cell and the player isn't stranded in the void. A
    /// missing ESM is the cheapest reproduction of the "corrupt/missing ESM"
    /// failure mode and needs no game data.
    #[test]
    fn validate_cell_loadable_errors_on_missing_esm() {
        let err = validate_cell_loadable(&[], "/nonexistent/Missing.esm", "AnyCell")
            .expect_err("a missing ESM must fail the pre-flight");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Missing.esm"),
            "error should name the unreadable plugin: {msg}"
        );
    }

    /// #1865 / SCR-D6-NEW-03 — `ensure_globals_resource` must guard on
    /// `is_none()`, mirroring `exterior.rs`'s exterior-streaming guard,
    /// so a runtime `Globals::set` mutation survives a second cell load
    /// (e.g. an interior-to-interior door transition) rather than being
    /// silently reset back to the ESM-parsed default. Pre-fix, `load.rs`'s
    /// interior path called `insert_resource` unconditionally.
    #[test]
    fn ensure_globals_resource_preserves_runtime_mutation_across_reload() {
        use byroredux_plugin::esm::records::global::SettingValue;
        use byroredux_plugin::esm::records::GlobalRecord;
        use byroredux_scripting::globals::Globals;
        use std::collections::HashMap;

        let mut records = HashMap::new();
        records.insert(
            0x1000,
            GlobalRecord {
                form_id: 0x1000,
                editor_id: "GameHour".to_string(),
                value: SettingValue::Float(8.0),
            },
        );

        let mut world = World::new();

        // First "load": no resource present yet, so it's built from records.
        ensure_globals_resource(&mut world, &records);
        assert_eq!(world.resource::<Globals>().get(0x1000), Some(8.0));

        // A Papyrus SetGlobalValue-style runtime write.
        world.resource_mut::<Globals>().set(0x1000, 23.5);
        assert_eq!(world.resource::<Globals>().get(0x1000), Some(23.5));

        // Second "load" (simulates an interior-to-interior transition) with
        // the SAME static records — must NOT clobber the runtime mutation.
        ensure_globals_resource(&mut world, &records);
        assert_eq!(
            world.resource::<Globals>().get(0x1000),
            Some(23.5),
            "a second cell load must preserve the runtime Globals::set mutation, \
             not reset it back to the ESM-parsed default"
        );
    }

    /// Minimal FNV-shape interior lighting (no Skyrim+ / Starfield tail).
    fn interior_lighting() -> esm::cell::CellLighting {
        esm::cell::CellLighting {
            ambient: [0.10, 0.10, 0.12],
            directional_color: [1.0, 0.95, 0.80],
            directional_azimuth: 0.0,
            directional_elevation: 0.0,
            fog_color: [0.50, 0.45, 0.30],
            fog_near: 100.0,
            fog_far: 8000.0,
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
            starfield: None,
        }
    }

    /// Regression for #1340 / D3-04 — the shared interior-lighting apply
    /// helper must install a `CellLightingRes` with `is_interior == true`.
    /// Pre-fix, two of the three interior-load entry points (the door-walk
    /// transition + the `cell.load` debug command) skipped this entirely,
    /// so a runtime-loaded interior kept the *previous* cell's resource —
    /// wrong fog/ambient and the exterior directional sun leaking into a
    /// sealed interior (the gate #1282 added keys on `is_interior`).
    /// Routing all three callers through this one helper is the structural
    /// fix; this pins the helper's `is_interior == true` contract.
    #[test]
    fn apply_interior_cell_lighting_inserts_interior_resource() {
        let mut world = World::new();
        // Fresh world == "no previous cell lighting present".
        assert!(world.try_resource::<CellLightingRes>().is_none());

        apply_interior_cell_lighting(
            &mut world,
            Some(&interior_lighting()),
            false,
            Default::default(),
        );

        let res = world
            .try_resource::<CellLightingRes>()
            .expect("apply_interior_cell_lighting must insert CellLightingRes");
        assert!(
            res.is_interior,
            "interior lighting must set is_interior=true so the directional \
             sun is gated out of the sealed cell (#1282 / #1340)"
        );
        assert_eq!(res.ambient, [0.10, 0.10, 0.12], "ambient must propagate");
        assert_eq!(res.fog_far, 8000.0, "fog_far must propagate");
    }

    #[test]
    fn interior_sky_exposure_is_replaced_on_cell_transition() {
        let mut world = World::new();
        apply_interior_cell_lighting(&mut world, None, true, Default::default());
        assert!(world
            .try_resource::<crate::components::InteriorSkyExposureRes>()
            .unwrap()
            .0);

        apply_interior_cell_lighting(&mut world, None, false, Default::default());
        assert!(!world
            .try_resource::<crate::components::InteriorSkyExposureRes>()
            .unwrap()
            .0);
    }

    /// Regression for FNV-D1-01 — a cell with no `XCLL` and no resolvable
    /// `LTMP` (`resolve_cell_lighting` returns `None`) must still install a
    /// fresh, interior-flagged `CellLightingRes`, not silently leave a
    /// stale one (potentially an exterior cell's, with `is_interior: false`
    /// and a full-strength directional sun) sitting in the world from
    /// whatever loaded before it.
    #[test]
    fn apply_interior_cell_lighting_none_overwrites_stale_exterior_resource() {
        let mut world = World::new();
        // Simulate walking in from an exterior cell: a stale, non-interior
        // resource with a live directional sun is already installed.
        // #5814 — the shared no-tail build.
        world.insert_resource(CellLightingRes::bare(
            [0.5, 0.5, 0.6],
            [1.0, 0.95, 0.8],
            [0.3, -0.8, 0.5],
            false,
            [0.6, 0.6, 0.7],
            1000.0,
            50_000.0,
        ));

        apply_interior_cell_lighting(&mut world, None, false, Default::default());

        let res = world
            .try_resource::<CellLightingRes>()
            .expect("None must still install a CellLightingRes, not leave the stale one untouched");
        assert!(
            res.is_interior,
            "the engine-default interior fallback must set is_interior=true \
             so no scene directional leaks into the sealed cell (FNV-D1-01)"
        );
        assert_eq!(
            res.directional_color,
            [0.0, 0.0, 0.0],
            "engine-default fallback must not carry over the previous cell's directional color"
        );
        assert_ne!(
            res.ambient,
            [0.5, 0.5, 0.6],
            "engine-default fallback must not carry over the previous (exterior) cell's ambient"
        );
    }
}
