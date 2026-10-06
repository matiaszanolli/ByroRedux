//! World cell streaming (M40 Phase 1a).
//!
//! Owns the live (gx, gy) → cell_root map and the streaming control
//! parameters. The App-level driver (`app_step.rs`) reads the active
//! camera position each frame, asks
//! [`compute_streaming_deltas`] which cells need to enter or leave the
//! loaded set, and dispatches to
//! [`crate::cell_loader::load_one_exterior_cell`] / [`crate::cell_loader::unload_cell`].
//!
//! ## Hysteresis
//!
//! Cells load at `radius_load` and unload at `radius_unload`
//! (= `radius_load + 1`). A player walking the boundary doesn't thrash
//! a cell every frame: the cell loads as the player crosses into the
//! load radius, stays loaded for one extra cell of travel, and only
//! unloads once the player is genuinely past the boundary.
//!
//! NIF extraction and parsing run on the worker below. The main thread
//! applies completed payloads under a per-frame spawn budget; exterior
//! bootstrap uses the same request and payload path instead of maintaining
//! a second synchronous loader.

use crate::cell_loader::ExteriorWorldContext;
use pre_parse::{build_stream_parse_pool, cell_pre_parse_worker};

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_core::string::StringPool;
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use crate::asset_provider::{MaterialProvider, TextureProvider};

/// One loaded cell tracked by [`WorldStreamingState`]. The
/// `cell_root` is the `EntityId` returned by
/// `load_one_exterior_cell`; passing it to
/// `crate::cell_loader::unload_cell` tears the cell down (despawn
/// every entity stamped with this `CellRoot`, drop mesh / BLAS /
/// texture refs).
#[derive(Debug, Clone, Copy)]
pub struct LoadedCell {
    pub cell_root: EntityId,
}

/// One distant-terrain LOD block tracked by [`WorldStreamingState`]
/// (#1373), keyed by block-coord. `hole_mask` is the 16-bit per-cell
/// hole pattern — bit `dy * LOD_BLOCK_CELLS + dx` is set when that cell
/// is holed (inside the full-detail radius, or missing landscape). When
/// the player moves and a boundary block's mask changes, the block is
/// regenerated so its hole-out tracks the streamed near terrain instead
/// of staying anchored to the spawn cell. Unloading a block calls
/// `drop_mesh(mesh_handle)` (frees its global-SSBO range on the next
/// rebuild) + `World::despawn(entity)`.
#[derive(Debug, Clone, Copy)]
pub struct LodBlock {
    pub entity: EntityId,
    pub mesh_handle: u32,
    /// Base ground `TextureHandle` acquired via `resolve_texture` at spawn
    /// (refcount bump). `World::despawn` has no GPU side effects, so
    /// `unload_lod_block` must `drop_texture` this explicitly or the
    /// refcount never reaches 0 and the VkImage + bindless slot pin for the
    /// session (#1537). `0` = fallback/placeholder, never per-block
    /// refcounted (skip the drop).
    pub texture_handle: u32,
    /// Per-quad tangent-space normal-map `TextureHandle` for a prebaked
    /// `.btr` block (#2371), or `0` when the block has none — the synth
    /// path, FO4's model-space `_msn` variant, and any quad whose `_n`
    /// sibling is missing. Refcounted and dropped exactly like
    /// `texture_handle`.
    pub normal_texture_handle: u32,
    pub hole_mask: u16,
}

/// Distant, worldspace-wide LOD water mesh (#2449 / EXAL-01; per cell since
/// #5243) — the distant counterpart of a cell's full-detail
/// `spawn_water_plane`, built by `cell_loader::water::spawn_lod_water_plane`
/// as one quad per distant cell at that cell's **effective** water height
/// (explicit XCLW override → that height, absent → the WRLD `NAM3`/`NAM4`
/// default, authored dry sentinel → skipped). Unlike [`LodBlock`], this is a
/// SINGLE entity per worldspace, not a per-ring-block streaming set: spawned
/// once at worldspace entry and reclaimed once on worldspace exit
/// (`streaming_helpers::drain_streaming_state`).
///
/// The mesh's hole is *which cells are skipped*, so it follows the player
/// across grid boundaries by REBUILDING the mesh
/// (`cell_loader::water::rebuild_lod_water_mesh`) rather than by translation
/// — the old single-sheet annulus could translate because its geometry was
/// center-relative, but moving a per-cell mesh would drag every distant
/// quad's world position with it. The rebuild swaps only the mesh handle;
/// the entity, material, and texture handles stay, and an emptied ring
/// (nothing wet within reach) keeps the entity mesh-less until a later
/// crossing finds water again. Like full-detail water, this render-only
/// entity uses the safe per-mesh-buffer upload path (`rt_enabled: false`)
/// and never enters the TLAS.
#[derive(Debug, Clone, Copy)]
pub struct LodWaterPlane {
    pub entity: EntityId,
    /// Live mesh handle; `None` while the ring is dry (the entity persists,
    /// mesh-less, until a rebuild finds water again).
    pub mesh_handle: Option<u32>,
    /// Normal-map `TextureHandle` acquired via `resolve_texture` at spawn,
    /// mirroring `spawn_water_plane`'s `NormalMapHandle` refcount contract
    /// (#1338). `None` when the procedural-fallback normal is used (no
    /// texture acquired, nothing to release).
    pub normal_map_handle: Option<u32>,
    /// NAM2–4 noise-map handles acquired for the LOD material. Zero entries
    /// are the shared fallback and are not refcounted.
    pub noise_map_handles: [u32; 3],
    /// The worldspace `NAM3`/`NAM4` default water height the mesh was
    /// spawned with — what absent-XCLW cells inherit on rebuild.
    pub default_height: f32,
    /// Player grid the mesh was built around; the next grid crossing
    /// triggers the rebuild that re-centers the hole.
    pub center_grid: (i32, i32),
}

/// Worker request — main thread asks the worker to pre-parse a cell.
/// Carries everything the worker needs to extract NIF bytes from BSA
/// and run the worker-safe parse/import portion of the pipeline.
pub struct LoadCellRequest {
    pub gx: i32,
    pub gy: i32,
    /// Generation counter snapshot at request time. The drain step
    /// compares against the current generation for `(gx, gy)` and drops
    /// stale payloads — the player may have moved out of range and back
    /// while the worker was busy.
    pub generation: u64,
    /// Monotonic enqueue stamp used to separate worker queueing from actual
    /// extract/parse service time in the boundary benchmark.
    pub queued_at: Instant,
    pub wctx: Arc<ExteriorWorldContext>,
    pub tex_provider: Arc<TextureProvider>,
    /// Snapshot of `NifImportRegistry`'s cached keys at request-build
    /// time. The worker skips BSA-extract + parse for any model path
    /// already in this set — main-thread cache will spawn it through
    /// [`crate::cell_loader::load_one_exterior_cell`] without needing
    /// the worker to re-produce the import. See #862. Includes
    /// negative-cache entries so known-failed parses aren't re-tried.
    /// May lag the registry by a few ms (more cache entries can land
    /// between snapshot and worker dispatch); that's harmless — at
    /// worst the worker over-extracts, never under-skips.
    pub cached_keys: Arc<std::collections::HashSet<String>>,
}

/// Worker output — pre-parsed scenes for every NIF the cell references.
/// `parsed` keys are lowercased model paths (matching the
/// `NifImportRegistry` key shape). The main-thread drain step finishes
/// the import (string interning + BGSM merge) and inserts into the
/// process-lifetime cache before calling `load_one_exterior_cell`.
pub struct LoadCellPayload {
    pub gx: i32,
    pub gy: i32,
    pub generation: u64,
    pub timings: StreamingWorkerTimings,
    /// `Some(scene)` = parsed cleanly. `None` = extraction or parse
    /// failed; the entry is still emitted so the cache records the
    /// negative result and a future placement of the same model
    /// doesn't re-attempt the parse.
    pub parsed: HashMap<String, Option<PartialNifImport>>,
}

/// Main-thread continuation for one worker payload.
///
/// Only one is active at a time. Keeping the original generation in the job
/// lets boundary-crossing cancellation reuse the same `pending` generation
/// gate as payload arrival.
pub(crate) struct StreamingCellApplyJob {
    pub(crate) coord: (i32, i32),
    pub(crate) generation: u64,
    pub(crate) phase: StreamingCellApplyPhase,
}

// One instance at most (the single `active_apply` slot), so the size gap
// between phases costs nothing worth a per-transition box.
#[allow(clippy::large_enum_variant)]
pub(crate) enum StreamingCellApplyPhase {
    /// Finish pool/material-dependent import work one NIF at a time.
    FinishImports(std::collections::hash_map::IntoIter<String, Option<PartialNifImport>>),
    /// All worker imports are resident; the exterior cell has not begun.
    BeginExterior,
    /// Terrain/root are resident and the placed-reference walk is resumable.
    Spawn(crate::cell_loader::ExteriorCellApplyJob),
}

impl StreamingCellApplyJob {
    pub(crate) fn from_payload(payload: LoadCellPayload) -> Self {
        Self {
            coord: (payload.gx, payload.gy),
            generation: payload.generation,
            phase: StreamingCellApplyPhase::FinishImports(payload.parsed.into_iter()),
        }
    }
}

/// Worker portion of NIF import. Meshes and collisions are imported against
/// `worker_pool` before the payload crosses to the main thread. Their
/// `FixedString` handles are valid only with that pool; the drain re-interns
/// the material paths into the world's pool before caching the result.
/// External Starfield `.mesh` data is resolved through the immutable
/// `TextureProvider` while the worker owns the NIF import.
pub struct PartialNifImport {
    /// Parsed scene — still needed by the drain for pool-free metadata such
    /// as collision authoring, flame markers, and furniture markers.
    pub scene: byroredux_nif::scene::NifScene,
    /// Meshes produced by the worker-local import walk. Texture/material
    /// handles in these meshes refer to `worker_pool` until the drain's
    /// re-intern boundary runs.
    pub meshes: Vec<byroredux_nif::import::ImportedMesh>,
    /// Collision geometry produced alongside `meshes` on the worker.
    pub collisions: Vec<byroredux_nif::import::ImportedCollision>,
    /// Interner that owns the `FixedString` symbols embedded in `meshes`.
    pub worker_pool: StringPool,
    /// BSXFlags bit-set extracted from the scene root. Bit 5 indicates
    /// marker children on classic content; the NIF walker filters those
    /// children individually while preserving sibling geometry (#3036).
    pub bsx: u32,
    /// Root NiNode `NiAVObject.flags` (SELECTIVE_UPDATE / DISABLE_SORTING
    /// / DISPLAY_OBJECT / IS_NODE / …) for placement-root SceneFlags
    /// parity with the loose-NIF loader. See #1235 / LC-D1-NEW-01.
    pub root_flags: u32,
    /// Lights — pool-free import path.
    pub lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters — pool-free import path.
    pub particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Embedded animation clip — pool-free import path.
    pub embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
    /// `Some(geometry_dedup)` for an FO4 precombine `_oc.nif` whose geometry
    /// the worker decoded from its `Geometry.csg` (M49): `meshes` are the
    /// placed instances, and the drain builds the same geometry-only cache
    /// entry `PrecombinedSpawnJob` would, with the precombine material merge.
    pub precombine_geometry: Option<Vec<u32>>,
}

/// World-streaming state. Owned by `App` (not an ECS resource — needs
/// to coexist on the same struct as `VulkanContext` and the texture /
/// material providers, all of which the streaming driver borrows
/// mutably each frame).
pub struct WorldStreamingState {
    /// Once-per-session parsed plugin snapshot + chosen worldspace +
    /// resolved climate / default weather. Cheap to clone the `Arc`
    /// into the worker thread per request.
    pub wctx: Arc<ExteriorWorldContext>,
    /// Long-lived texture archive provider (BSA / BA2 readers). Behind
    /// `Arc` so the worker thread can extract NIF bytes off-thread —
    /// `BsaArchive` / `Ba2Archive` read through positional reads with no
    /// shared cursor, so concurrent extracts are safe without a lock (on
    /// Windows the kernel still serialises reads on one handle; see
    /// `crates/bsa/src/read_at.rs`, #4999).
    pub tex_provider: Arc<TextureProvider>,
    /// The dedicated stream pool (#3089): the worker's parallel NIF parse
    /// and the main thread's texture prefetch
    /// ([`crate::asset_provider::prefetch_textures`]) both run here, never
    /// on the global pool the frame's parallel stages use.
    pub(crate) stream_pool: Arc<rayon::ThreadPool>,
    /// Long-lived BGSM material provider. Stays main-thread only —
    /// `merge_external_material` needs `&mut MaterialProvider` (writes to
    /// `bgsm_cache` / `bgem_cache` / `failed_paths`), and serialising
    /// every drain-step BGSM resolve through a Mutex would put the
    /// main thread on the slow path. Worker doesn't touch BGSM.
    pub mat_provider: MaterialProvider,
    /// Currently-loaded cells.
    pub loaded: HashMap<(i32, i32), LoadedCell>,
    /// Root owning the active worldspace's persistent CELL. Unlike `loaded`,
    /// this is not keyed by a grid coordinate and never participates in
    /// radius eviction; it is reclaimed only when the worldspace drains.
    pub persistent_root: Option<EntityId>,
    /// Resumable persistent-CELL spawn. It shares the main-thread apply
    /// deadline with ordinary exterior tiles and is cleared on completion.
    pub(crate) persistent_apply: Option<crate::cell_loader::PersistentCellApplyJob>,
    /// Distant-terrain LOD blocks, keyed by block-coord (#1373). Streamed
    /// each cell-boundary crossing alongside the full-detail cells: blocks
    /// entering the LOD radius spawn, blocks leaving unload, and boundary
    /// blocks whose hole mask changed regenerate. The Slice-1 ring spawned
    /// these once and never tracked them — re-entry leaked ~600 blocks and
    /// the hole-out went stale as the player walked.
    pub lod_blocks: HashMap<(i32, i32, i32), LodBlock>,
    /// Terrain-LOD coordinates that were reconciled but produced no mesh,
    /// keyed to the hole mask that was attempted. This is the terrain
    /// equivalent of the empty sentinels stored directly in the object and
    /// placement maps: incremental initialization must not retry the same
    /// absent asset every frame and starve all later coordinates.
    pub lod_missing_blocks: HashMap<(i32, i32, i32), u16>,
    /// Distant **object** LOD quads, keyed by `(level, qx, qy)` — the quad's
    /// LOD band plus its SW-corner cell (EXAL step 6). Skyrim+/FO4 only —
    /// each entry is the baked `.bto` macro-mesh's spawned sub-meshes (or an
    /// empty sentinel for a quad with no baked LOD). Reconciled progressively
    /// alongside `lod_blocks`; quads load only outside the full-detail ring.
    ///
    /// The `level` is part of the key because the same ground is covered by a
    /// different quad in every band (#2371): a band switch is an unload of
    /// the old `(level, …)` entry plus a load of the new one, which is what
    /// keeps two levels from ever double-drawing it.
    pub object_lod_blocks: HashMap<(i32, i32, i32), crate::cell_loader::ObjectLodBlock>,
    /// Memoised "does the game ship a baked LOD asset for this quad?"
    /// (#3385). Keyed `(level, qx, qy)`; separate maps because the terrain
    /// and baked-object rings probe different archives for the same key.
    ///
    /// The answer is a pure function of the worldspace, the quad, and the
    /// opened archive set — none of which change while a
    /// `WorldStreamingState` lives (`tex_provider` is an `Arc` cloned into
    /// the worker, never rebuilt in place). The band descent nonetheless
    /// re-derived it from scratch on every reconcile frame, and each miss
    /// costs several `String` allocations plus one hashed lookup per open
    /// archive — work that scales with the ring while the reconcile is
    /// allowed to do only `MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME`
    /// loads, on the main thread inside `STREAMING_APPLY_BUDGET`.
    ///
    /// `FxHashMap` per the hot-path hashing rule — integer-tuple keyspace,
    /// per-frame, not DoS-facing. Cleared with the rings in
    /// `drain_streaming_state`.
    pub lod_terrain_available: rustc_hash::FxHashMap<(i32, i32, i32), bool>,
    /// Baked-object half of [`Self::lod_terrain_available`] (#3385).
    pub lod_object_available: rustc_hash::FxHashMap<(i32, i32, i32), bool>,
    /// Real load/unload/reload churn on `lod_blocks`' keys, independent of
    /// `telemetry.superseded_lod` (which only catches one in-flight load
    /// cancelled by the *next* boundary; this catches a settled key
    /// flapping across several) — EX-10/11 / #2371.
    pub(crate) terrain_lod_churn: crate::cell_loader::ChurnTracker,
    /// Same as `terrain_lod_churn`, for `object_lod_blocks`.
    pub(crate) object_lod_churn: crate::cell_loader::ChurnTracker,
    /// Distant **object** LOD cells for Oblivion's placement scheme, keyed
    /// by cell `(x, y)`. Each entry is the cell's
    /// `DistantLOD\*.lod` instanced `_far.nif` meshes (or an empty sentinel
    /// for a cell with no `.lod`). Streamed alongside `object_lod_blocks`;
    /// only one of the two ever populates per game (the gate is by
    /// `GameKind`). Cells load only outside the full-detail ring.
    pub placement_lod_blocks: HashMap<(i32, i32), crate::cell_loader::PlacementLodBlock>,
    /// Distant worldspace-wide LOD water quad (`NAM3`/`NAM4`, #2449 /
    /// EXAL-01). `None` when the worldspace authors no LOD water, or the
    /// mesh upload failed at spawn. Set once at worldspace entry (see
    /// [`LodWaterPlane`]'s doc for why this isn't reconciled per-block like
    /// `lod_blocks`), reclaimed once in `drain_streaming_state`.
    pub lod_water: Option<crate::streaming::LodWaterPlane>,
    /// #4413 — pseudo cell root owning the authored ground-cover templates
    /// (one hidden instance of each `GRAS` model). Spawned once at worldspace
    /// entry, reclaimed with the other roots in `drain_streaming_state`.
    pub authored_cover_root: Option<EntityId>,
    /// Cells whose load request is in flight on the worker. Maps
    /// `(gx, gy)` to the generation of the outstanding request.
    /// Drain compares the payload's generation against this map's
    /// entry — mismatch ⇒ payload is stale, drop it.
    pub pending: HashMap<(i32, i32), u64>,
    /// Generation counter — bumped per request so a "load → unload →
    /// reload" sequence on the same `(gx, gy)` cell can distinguish
    /// the outstanding payload from the new one. Drains never apply
    /// payloads whose generation doesn't match `pending[(gx, gy)]`.
    pub next_generation: u64,
    /// Load radius — cells within this Chebyshev distance of the player
    /// are loaded. `1` = 3×3 grid, `2` = 5×5, `3` = 7×7.
    pub radius_load: i32,
    /// Unload radius — cells outside this Chebyshev distance are
    /// unloaded. Must be `>= radius_load + 1` to avoid load-unload
    /// thrash at the boundary.
    pub radius_unload: i32,
    /// Last (gx, gy) the player was in. Used by the App driver to
    /// suppress no-op streaming work when the player hasn't crossed a
    /// cell boundary.
    pub last_player_grid: Option<(i32, i32)>,
    /// CLMT FormID currently driving the installed sky / weather
    /// resources — the worldspace climate at bootstrap, or a cell's own
    /// `XCCM` override once the player walks into a cell that authors
    /// one. `None` means no climate resolved (procedural fallback sky).
    ///
    /// Owned here rather than recomputed because it records what was
    /// *applied*, which is the only thing a boundary crossing can compare
    /// against to know whether re-applying is needed. See
    /// `scene::apply_cell_climate_override` (#2451 / EXAL-03).
    pub applied_climate_form: Option<u32>,
    /// `(player_grid, resolved RegionAmbientRes)` the region ambient
    /// directive was last resolved for (#3679). `wctx` — and therefore the
    /// REGN/XCLR data `RegionAmbientRes::resolve` reads — never changes
    /// across a `WorldStreamingState`'s lifetime (it's a once-per-session
    /// `Arc` set at construction, see its own doc), so `player_grid` alone
    /// is a sufficient cache key: the same grid cell always resolves to the
    /// same ambient directive for as long as this state lives.
    /// `apply_cell_region_ambient` (`scene/world_setup.rs`) skips the
    /// `resolve()` call — which allocates and sorts a `Vec` — when the
    /// player is still in the cached grid cell.
    pub(crate) applied_region_ambient: Option<((i32, i32), crate::components::RegionAmbientRes)>,
    /// Whether the three distant-LOD rings still have deferred reconcile
    /// work. Foreground-first bootstrap and cell-boundary movement set this;
    /// idle frames clear it progressively through the shared LOD budget.
    pub lod_reconcile_pending: bool,
    /// Set whenever `loaded` gains or loses an entry since the diagnostics
    /// in `reconcile_lod_rings` last sampled it — a boundary-crossing unload
    /// (`app_step.rs`) or a full-detail cell finishing its apply
    /// (`advance_streaming_apply`'s `loaded.insert` sites). `reconcile_lod_rings`
    /// consumes and clears it each call; `update_terrain_seam_stats` only
    /// needs recomputing when this is set, since its only input is
    /// `loaded`'s key set (#3688).
    pub(crate) loaded_residency_changed: bool,
    /// Boundary-to-ready and apply-slice aggregates. Read by the benchmark
    /// summary; otherwise passive, bounded runtime diagnostics.
    pub telemetry: StreamingTelemetry,
    /// Worker thread handle. Held so the thread isn't detached. On
    /// graceful shutdown [`WorldStreamingState::shutdown`] drops
    /// `request_tx` (so the worker's recv loop exits) and joins this
    /// handle with a bounded timeout (#856). Kept inside `Option` so
    /// `shutdown` can move the handle out of `self` by destructure
    /// without `JoinHandle: Default`. The [`Drop`] impl on
    /// `WorldStreamingState` (#1167) mirrors that shutdown handshake
    /// for any exit path that bypasses the explicit call.
    pub worker: Option<JoinHandle<()>>,
    /// mpsc channel sending requests to the worker. Wrapped in
    /// `Option` so [`Drop`] (#1167) can `take()` it and drop the
    /// sender BEFORE the worker `JoinHandle` is dropped — Rust's
    /// declaration-order field-drop would otherwise drop the worker
    /// (= detach) before the channel close, defeating the join. Send
    /// sites go through [`WorldStreamingState::send_request`].
    pub request_tx: Option<mpsc::Sender<LoadCellRequest>>,
    /// mpsc receiver for completed payloads. Drained each frame by the
    /// App driver; non-blocking via `try_recv`.
    pub payload_rx: mpsc::Receiver<LoadCellPayload>,
    /// Current resumable main-thread apply. The matching `pending` entry stays
    /// live until this completes, suppressing duplicate requests and making a
    /// boundary-crossing removal an immediate cancellation signal.
    pub(crate) active_apply: Option<StreamingCellApplyJob>,
}

impl WorldStreamingState {
    /// New scene geometry is appended throughout a resumable cell/LOD load.
    /// Rebuilding the whole global SSBO after every atomic REFR would turn a
    /// large FO4 crossing into dozens of 600–900 MiB copies. The frame driver
    /// defers that rebuild while this returns true and masks appended ranges
    /// from raster/TLAS through `MeshRegistry::is_geometry_resident`.
    pub fn geometry_batch_in_progress(&self) -> bool {
        !self.pending.is_empty()
            || self.active_apply.is_some()
            || self.persistent_apply.is_some()
            || self.lod_reconcile_pending
    }

    /// Construct from an already-resolved [`ExteriorWorldContext`] and
    /// the long-lived providers. Spawns the cell-pre-parse worker
    /// thread; first request can be sent immediately.
    pub fn new(
        wctx: ExteriorWorldContext,
        tex_provider: TextureProvider,
        mat_provider: MaterialProvider,
        radius_load: i32,
    ) -> Self {
        // Hysteresis: unload at load + 1. Pre-fix any value would
        // accept; clamping here means a future caller passing
        // `radius_unload = radius_load` doesn't cause boundary thrash.
        let radius_load = radius_load.max(0);
        // Seed the applied-climate tracker from the worldspace resolve
        // (#2451): bootstrap installs that climate's sky, so the first
        // boundary crossing must compare against it, not against `None`
        // — otherwise every crossing in a worldspace with no XCCM cells
        // would read as a change and re-apply the same environment.
        let wctx_climate_form = wctx.climate.as_ref().map(|c| c.form_id);
        let (request_tx, request_rx) = mpsc::channel::<LoadCellRequest>();
        let (payload_tx, payload_rx) = mpsc::channel::<LoadCellPayload>();
        let stream_pool = Arc::new(build_stream_parse_pool());
        let worker_pool = Arc::clone(&stream_pool);
        let worker = std::thread::Builder::new()
            .name("byro-cell-stream".into())
            .spawn(move || cell_pre_parse_worker(request_rx, payload_tx, worker_pool))
            .expect("failed to spawn cell-stream worker thread");
        Self {
            wctx: Arc::new(wctx),
            tex_provider: Arc::new(tex_provider),
            stream_pool,
            mat_provider,
            loaded: HashMap::new(),
            persistent_root: None,
            persistent_apply: None,
            lod_blocks: HashMap::new(),
            lod_missing_blocks: HashMap::new(),
            object_lod_blocks: HashMap::new(),
            lod_terrain_available: rustc_hash::FxHashMap::default(),
            lod_object_available: rustc_hash::FxHashMap::default(),
            terrain_lod_churn: crate::cell_loader::ChurnTracker::default(),
            object_lod_churn: crate::cell_loader::ChurnTracker::default(),
            placement_lod_blocks: HashMap::new(),
            lod_water: None,
            authored_cover_root: None,
            pending: HashMap::new(),
            next_generation: 0,
            radius_load,
            radius_unload: radius_load + 1,
            last_player_grid: None,
            applied_climate_form: wctx_climate_form,
            applied_region_ambient: None,
            lod_reconcile_pending: false,
            loaded_residency_changed: false,
            telemetry: StreamingTelemetry::default(),
            worker: Some(worker),
            request_tx: Some(request_tx),
            payload_rx,
            active_apply: None,
        }
    }

    /// #4413 — spawn the worldspace's authored ground-cover templates, when
    /// `install_ground_cover` resolved an [`AuthoredCover`] for it. The
    /// renderer instances them; see `cell_loader::spawn::authored_cover`.
    ///
    /// [`AuthoredCover`]: byroredux_core::ecs::components::groundcover::AuthoredCover
    pub fn spawn_authored_cover(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let Some(cover) = world
            .try_resource::<byroredux_core::ecs::components::groundcover::AuthoredCover>()
            .map(|cover| cover.clone())
        else {
            return;
        };
        self.authored_cover_root =
            crate::cell_loader::spawn::authored_cover::spawn_authored_cover_templates(
                world,
                ctx,
                &self.tex_provider,
                &mut self.mat_provider,
                &cover,
            );
    }

    /// Spawn the worldspace-wide distant LOD water quad for this streaming
    /// state's worldspace, if it authors one (`NAM3`/`NAM4`) and a player
    /// grid position has already been set (#2449 / EXAL-01). Leaves
    /// `lod_water` untouched (stays `None`) if either precondition isn't
    /// met, so callers can invoke this unconditionally right after
    /// `last_player_grid` is set, mirroring how `apply_worldspace_weather`
    /// is called unconditionally at every worldspace-entry call site.
    pub fn spawn_lod_water(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let Some(player_grid) = self.last_player_grid else {
            return;
        };
        let (Some(height), lod_water_form) = crate::env_translate::translate_lod_water(
            &self.wctx.record_index.cells.worldspaces,
            &self.wctx.worldspace_key,
        ) else {
            return;
        };
        // The worldspace's own cell table drives the per-cell distant mesh
        // (#5243): explicit XCLW overrides at their height, absent XCLW at
        // the worldspace default, authored dry sentinels skipped.
        let empty = HashMap::new();
        let cells = self
            .wctx
            .record_index
            .cells
            .exterior_cells
            .get(&self.wctx.worldspace_key)
            .unwrap_or(&empty);
        self.lod_water = crate::cell_loader::spawn_lod_water_plane(
            world,
            ctx,
            &self.tex_provider,
            &self.wctx.record_index.waters,
            height,
            lod_water_form,
            cells,
            player_grid,
            self.radius_unload,
            self.wctx.record_index.game,
        );
    }

    /// Keep the distant-water mesh's hole centered on the current
    /// full-detail streaming area. The hole is *which cells are skipped*, so
    /// a grid crossing rebuilds the mesh (`rebuild_lod_water_mesh`) — only
    /// the mesh handle swaps; the entity, material, and textures persist.
    pub fn recenter_lod_water(
        &mut self,
        world: &mut World,
        ctx: &mut VulkanContext,
        player_grid: (i32, i32),
    ) {
        let Some(plane) = self.lod_water.as_mut() else {
            return;
        };
        if plane.center_grid == player_grid {
            return;
        }
        let default_height = plane.default_height;
        let empty = HashMap::new();
        let cells = self
            .wctx
            .record_index
            .cells
            .exterior_cells
            .get(&self.wctx.worldspace_key)
            .unwrap_or(&empty);
        crate::cell_loader::rebuild_lod_water_mesh(
            world,
            ctx,
            plane,
            cells,
            default_height,
            self.wctx.record_index.game,
            player_grid,
            self.radius_unload,
        );
    }

    /// Send a load request to the worker. Returns `Err` if the worker
    /// channel has already been closed (Drop / shutdown). Hides the
    /// `Option<Sender>` field shape introduced for the #1167 Drop fix.
    pub fn send_request(
        &self,
        req: LoadCellRequest,
    ) -> Result<(), mpsc::SendError<LoadCellRequest>> {
        match self.request_tx.as_ref() {
            Some(tx) => tx.send(req),
            None => Err(mpsc::SendError(req)),
        }
    }

    /// Queue cells through the canonical exterior worker path.
    ///
    /// Generation allocation, duplicate suppression, pending bookkeeping,
    /// request construction, and closed-channel rollback live here so the
    /// initial bootstrap and steady-state boundary crossing cannot drift.
    /// Returns the number of requests successfully queued.
    pub fn queue_loads(
        &mut self,
        coords: impl IntoIterator<Item = (i32, i32)>,
        cached_keys: Arc<HashSet<String>>,
    ) -> usize {
        let mut queued = 0usize;
        for (gx, gy) in coords {
            let coord = (gx, gy);
            if self.loaded.contains_key(&coord) || self.pending.contains_key(&coord) {
                continue;
            }

            let generation = self.next_generation;
            self.next_generation = self.next_generation.wrapping_add(1);
            self.pending.insert(coord, generation);
            let req = LoadCellRequest {
                gx,
                gy,
                generation,
                queued_at: Instant::now(),
                wctx: self.wctx.clone(),
                tex_provider: self.tex_provider.clone(),
                cached_keys: cached_keys.clone(),
            };
            if self.send_request(req).is_err() {
                log::error!("Streaming worker channel closed; cell ({gx},{gy}) cannot be loaded");
                self.pending.remove(&coord);
            } else {
                queued += 1;
            }
        }
        queued
    }

    /// Graceful shutdown — close the request channel so the worker's
    /// recv loop exits, then join the worker with a bounded timeout.
    /// On timeout the worker is detached (matches the pre-#856
    /// unconditional-detach behaviour as a fallback). Replaces the
    /// previous `self.streaming.take()` pattern at the
    /// `WindowEvent::CloseRequested` handler in `main.rs`.
    ///
    /// The bound is necessary because the worker may be mid-
    /// `BsaArchive::extract()` (~100–300 ms typical, much longer on
    /// network filesystems or contended spinning disks); a slow
    /// extract should not block process teardown indefinitely. See
    /// AUDIT_CONCURRENCY_2026-05-05.md / C6-NEW-03.
    ///
    /// Takes `&mut self` (#1167) — the [`Drop`] safety-net calls into
    /// this same method, so both paths share one implementation. After
    /// `shutdown` returns, subsequent calls (including the eventual
    /// `Drop`) observe `worker: None` and short-circuit, so the join
    /// runs exactly once.
    pub fn shutdown(&mut self, timeout: std::time::Duration) {
        // Take the worker handle so the eventual `Drop` skips the
        // detaching path; the join below is the only place we wait on
        // this thread.
        let Some(handle) = self.worker.take() else {
            return;
        };
        // Close the request channel BEFORE the join. The worker's
        // `request_rx.recv()` returns Err on its next loop iteration
        // and the thread exits. The matching `payload_rx` will be
        // dropped automatically when `self` is dropped — if the worker
        // is currently inside `payload_tx.send(payload)` it observes
        // the closed receiver and bails via the existing post-#854
        // break path.
        let _ = self.request_tx.take();
        match join_with_timeout(handle, timeout) {
            Ok(()) => log::info!("cell-stream worker joined cleanly on shutdown"),
            Err(JoinTimeout) => log::warn!(
                "cell-stream worker did not exit within {:?} — detaching (#856). \
                 The worker thread will exit shortly after `request_tx` drop, but the \
                 process teardown won't block on it.",
                timeout
            ),
        }
    }
}

/// Safety-net teardown for every exit path that doesn't go through the
/// explicit [`WorldStreamingState::shutdown`] handshake (e.g. the
/// `--bench-frames` natural exit at `main.rs` and the panic / error
/// exits that call `event_loop.exit()` without first taking the
/// streaming state out of `App`). See #1167 / CONC-D6-NEW-01.
///
/// Delegates to `shutdown` with a fixed 1 s timeout. If `shutdown` was
/// already called explicitly, the take()'s inside it have set
/// `worker = None` / `request_tx = None`, so this re-entry observes
/// the short-circuit and is a no-op — the join runs exactly once.
impl Drop for WorldStreamingState {
    fn drop(&mut self) {
        self.shutdown(std::time::Duration::from_secs(1));
    }
}

/// Diff result computed by [`compute_streaming_deltas`]. Pure
/// data — no Vulkan, no World access — so it's testable in isolation
/// of the engine's runtime.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StreamingDeltas {
    /// Cells inside the load radius that aren't yet loaded. Sorted so
    /// the App driver loads cells in a deterministic order (closer to
    /// the player first, ties broken on (gx, gy) lexically). The
    /// closest-first ordering means the visible cell-of-arrival is
    /// always loaded before peripheral cells.
    pub to_load: Vec<(i32, i32)>,
    /// Cells outside the unload radius that are currently loaded. No
    /// inherent ordering required (the App driver unloads each via
    /// `unload_cell` independently). Sorted by (gx, gy) for
    /// deterministic output so the regression tests are stable.
    pub to_unload: Vec<(i32, i32)>,
}

/// Compute streaming deltas — which cells to load, which to unload —
/// given the player's current grid coords, the currently-loaded set,
/// and the load / unload radii.
///
/// Pure function with no I/O. The App driver consumes the deltas and
/// dispatches to the cell loader.
pub fn compute_streaming_deltas(
    loaded: &HashMap<(i32, i32), LoadedCell>,
    player_grid: (i32, i32),
    radius_load: i32,
    radius_unload: i32,
) -> StreamingDeltas {
    debug_assert!(
        radius_unload >= radius_load,
        "radius_unload ({radius_unload}) < radius_load ({radius_load}) — boundary thrash"
    );

    let (px, py) = player_grid;

    // Desired set: every cell inside the load radius (Chebyshev).
    let mut desired: HashSet<(i32, i32)> = HashSet::new();
    for dx in -radius_load..=radius_load {
        for dy in -radius_load..=radius_load {
            desired.insert((px + dx, py + dy));
        }
    }

    // Cells to load: in `desired`, not in `loaded`.
    let mut to_load: Vec<(i32, i32)> = desired
        .iter()
        .copied()
        .filter(|coord| !loaded.contains_key(coord))
        .collect();
    // Closest-first ordering by Chebyshev distance, ties on (gx, gy).
    to_load.sort_by_key(|(gx, gy)| {
        let d = (gx - px).abs().max((gy - py).abs());
        (d, *gx, *gy)
    });

    // Cells to unload: in `loaded`, outside the unload radius.
    let mut to_unload: Vec<(i32, i32)> = loaded
        .keys()
        .copied()
        .filter(|(gx, gy)| {
            let d = (gx - px).abs().max((gy - py).abs());
            d > radius_unload
        })
        .collect();
    to_unload.sort();

    StreamingDeltas { to_load, to_unload }
}

/// Cells with an in-flight worker request that have left the unload
/// radius around the player's current grid position — #2113 / D7-01.
///
/// `compute_streaming_deltas` only diffs `loaded` against the desired
/// set, so a cell dispatched to the worker but not yet spawned (still
/// only in `pending`) is invisible to it: if the player leaves before
/// the request completes, the payload would otherwise still classify
/// as [`PayloadDecision::Apply`] and pay a full main-thread spawn just
/// before the next boundary crossing unloads it again. The caller
/// removes each returned coord from `pending`, so `classify_payload`
/// sees no entry for it and returns `StaleNoPending` — the payload is
/// discarded before spawn.
pub fn stale_pending_coords(
    pending: &HashMap<(i32, i32), u64>,
    player_grid: (i32, i32),
    radius_unload: i32,
) -> Vec<(i32, i32)> {
    let (px, py) = player_grid;
    let mut stale: Vec<(i32, i32)> = pending
        .keys()
        .copied()
        .filter(|(gx, gy)| (gx - px).abs().max((gy - py).abs()) > radius_unload)
        .collect();
    stale.sort();
    stale
}

/// Convert a Y-up world-space translation into Bethesda exterior grid
/// coords. 4096 units per cell. The engine's Z-up→Y-up flip negates
/// the source-Y axis when populating world Z, so an exterior placed at
/// source `(2048, 2048, 0)` lands at world `(2048, 0, -2048)` and
/// resolves to grid `(0, 0)`.
pub fn world_pos_to_grid(world_x: f32, world_z: f32) -> (i32, i32) {
    let gx = (world_x / EXTERIOR_CELL_UNITS).floor() as i32;
    let gy = (-world_z / EXTERIOR_CELL_UNITS).floor() as i32;
    (gx, gy)
}

/// Generation-counter decision for an incoming worker payload.
///
/// The shared drain step in `streaming_helpers::consume_streaming_payload`
/// compares the payload's generation
/// against `WorldStreamingState.pending[(gx, gy)]`. A mismatch means
/// either:
///   * The cell was unloaded since the request was sent — `pending`
///     has no entry for the coord (`StaleNoPending`).
///   * The cell was unloaded and re-requested at a higher generation
///     — `pending` holds the new generation, payload's is older
///     (`StaleNewerPending`).
///
/// Both cases result in the payload being dropped without spawning;
/// the worker's pre-parse work is wasted but the world stays
/// consistent. This pure helper makes that invariant testable
/// without standing up the worker thread.
#[derive(Debug, PartialEq, Eq)]
pub enum PayloadDecision {
    /// Apply the payload — it matches the pending request for the
    /// cell.
    Apply,
    /// Drop — no pending entry for `(gx, gy)`. Cell was unloaded
    /// (or never loaded) while the payload was in flight.
    StaleNoPending,
    /// Drop — pending entry exists but at a different generation.
    /// Cell was unloaded and re-requested while the older payload was
    /// in flight.
    StaleNewerPending {
        pending_generation: u64,
        payload_generation: u64,
    },
}

/// Classify an incoming worker payload against the streaming state's
/// pending map. Returns the action the caller should take.
pub fn classify_payload(
    pending: &HashMap<(i32, i32), u64>,
    coord: (i32, i32),
    payload_generation: u64,
) -> PayloadDecision {
    match pending.get(&coord) {
        Some(&g) if g == payload_generation => PayloadDecision::Apply,
        Some(&pending_generation) => PayloadDecision::StaleNewerPending {
            pending_generation,
            payload_generation,
        },
        None => PayloadDecision::StaleNoPending,
    }
}

// #5092 — telemetry and the pre-parse worker pipeline split into
// submodules (this file crossed 2069 prod_loc). Glob re-exports keep every
// historical `crate::streaming::*` path — `streaming_tests.rs`'s import
// list and the App driver both consume them unchanged.
mod pre_parse;
mod telemetry;

pub use pre_parse::*;
pub use telemetry::*;

#[cfg(test)]
#[path = "../streaming_tests.rs"]
mod tests;
