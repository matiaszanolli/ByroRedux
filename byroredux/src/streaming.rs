//! World cell streaming (M40 Phase 1a).
//!
//! Owns the live (gx, gy) → cell_root map and the streaming control
//! parameters. The App-level driver (`main.rs`) reads the active
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
//! ## Phase 1a vs 1b
//!
//! Phase 1a (this module): sync load on the main thread via the
//! per-cell loader factored in commit `2e3f73e`. NIF parse + DDS
//! extract happen synchronously when each cell crosses the load
//! boundary — expected stutter ~50-100 ms per cell crossing on FNV
//! WastelandNV. The control loop (diff + dispatch + stale-load
//! cancellation) is the load-bearing piece; all of it works against a
//! `&mut World + &mut VulkanContext` driver.
//!
//! Phase 1b (next commit): replaces the sync load call with an
//! `mpsc::Sender<LoadCellRequest>` send and a payload drain step.
//! Worker thread does the parse off main thread. The diff logic and
//! state shape on this struct stay the same.

use byroredux_core::ecs::storage::EntityId;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::cell_loader::ExteriorWorldContext;

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

/// Worker request — main thread asks the worker to pre-parse a cell.
/// Carries everything the worker needs to extract NIF bytes from BSA
/// and run the pool-free portion of the import pipeline.
pub struct LoadCellRequest {
    pub gx: i32,
    pub gy: i32,
    /// Generation counter snapshot at request time. The drain step
    /// compares against the current generation for `(gx, gy)` and drops
    /// stale payloads — the player may have moved out of range and back
    /// while the worker was busy.
    pub generation: u64,
    pub wctx: Arc<ExteriorWorldContext>,
    pub tex_provider: Arc<TextureProvider>,
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
    /// `Some(scene)` = parsed cleanly. `None` = extraction or parse
    /// failed; the entry is still emitted so the cache records the
    /// negative result and a future placement of the same model
    /// doesn't re-attempt the parse.
    pub parsed: HashMap<String, Option<PartialNifImport>>,
}

/// Pool-free portion of NIF import — everything the worker can do
/// off-thread. The main-thread drain step takes a `PartialNifImport`,
/// runs `import_nif_with_collision` (string interning, needs the
/// world's `StringPool`) and `merge_bgsm_into_mesh` (needs the
/// `MaterialProvider`), and assembles the full
/// `cell_loader::CachedNifImport`.
pub struct PartialNifImport {
    /// Parsed scene — needed by the main-thread import step
    /// (`import_nif_with_collision` walks this).
    pub scene: byroredux_nif::scene::NifScene,
    /// BSXFlags bit-set extracted from the scene root. The drain step
    /// honours the `0x20` editor-marker bit (skip insertion).
    pub bsx: u32,
    /// Lights — pool-free import path.
    pub lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters — pool-free import path.
    pub particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Embedded animation clip — pool-free import path.
    pub embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
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
    /// `BsaArchive` / `Ba2Archive` already serialise their inner `File`
    /// access via `Mutex`, so concurrent extracts are safe.
    pub tex_provider: Arc<TextureProvider>,
    /// Long-lived BGSM material provider. Stays main-thread only —
    /// `merge_bgsm_into_mesh` needs `&mut MaterialProvider` (writes to
    /// `bgsm_cache` / `bgem_cache` / `failed_paths`), and serialising
    /// every drain-step BGSM resolve through a Mutex would put the
    /// main thread on the slow path. Worker doesn't touch BGSM.
    pub mat_provider: MaterialProvider,
    /// Currently-loaded cells.
    pub loaded: HashMap<(i32, i32), LoadedCell>,
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
    /// Worker thread handle. Held so the thread isn't detached; on
    /// shutdown the worker observes the `request_tx` drop, exits its
    /// recv loop, and the `JoinHandle` lets a future graceful-shutdown
    /// path wait on it. Kept inside `Option` so `WorldStreamingState`
    /// can be moved out of the App on shutdown without forcing a join.
    /// `dead_code` allow: nothing currently calls `.take().join()` —
    /// holding the handle is the point (prevents the OS thread from
    /// being treated as a detached/leaked allocation).
    #[allow(dead_code)]
    pub worker: Option<JoinHandle<()>>,
    /// mpsc channel sending requests to the worker. Dropped on
    /// `WorldStreamingState` Drop so the worker exits cleanly.
    pub request_tx: mpsc::Sender<LoadCellRequest>,
    /// mpsc receiver for completed payloads. Drained each frame by the
    /// App driver; non-blocking via `try_recv`.
    pub payload_rx: mpsc::Receiver<LoadCellPayload>,
}

impl WorldStreamingState {
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
        let (request_tx, request_rx) = mpsc::channel::<LoadCellRequest>();
        let (payload_tx, payload_rx) = mpsc::channel::<LoadCellPayload>();
        let worker = std::thread::Builder::new()
            .name("byro-cell-stream".into())
            .spawn(move || cell_pre_parse_worker(request_rx, payload_tx))
            .expect("failed to spawn cell-stream worker thread");
        Self {
            wctx: Arc::new(wctx),
            tex_provider: Arc::new(tex_provider),
            mat_provider,
            loaded: HashMap::new(),
            pending: HashMap::new(),
            next_generation: 0,
            radius_load,
            radius_unload: radius_load + 1,
            last_player_grid: None,
            worker: Some(worker),
            request_tx,
            payload_rx,
        }
    }
}

/// Cell pre-parse worker loop. Pulls requests off the channel, does
/// the off-thread work for every NIF the cell references, and emits a
/// single `LoadCellPayload` per request.
///
/// Exits when `request_rx` returns `Err` (sender dropped on
/// `WorldStreamingState` shutdown).
fn cell_pre_parse_worker(
    request_rx: mpsc::Receiver<LoadCellRequest>,
    payload_tx: mpsc::Sender<LoadCellPayload>,
) {
    log::info!("cell-stream worker thread started");
    while let Ok(req) = request_rx.recv() {
        let LoadCellRequest {
            gx,
            gy,
            generation,
            wctx,
            tex_provider,
        } = req;
        let payload = pre_parse_cell(gx, gy, generation, &wctx, &tex_provider);
        if payload_tx.send(payload).is_err() {
            // Receiver dropped — main thread is shutting down; exit cleanly.
            break;
        }
    }
    log::info!("cell-stream worker thread exiting");
}

/// Per-cell pre-parse: walk references, resolve unique model paths,
/// extract NIF bytes from the texture provider's mesh archives, and
/// run the pool-free portion of the NIF import pipeline.
///
/// Returns a populated [`LoadCellPayload`] (which may have an empty
/// `parsed` map if the cell doesn't exist or has no references — the
/// main-thread drain still applies the empty payload so the pending
/// entry is cleared).
fn pre_parse_cell(
    gx: i32,
    gy: i32,
    generation: u64,
    wctx: &ExteriorWorldContext,
    tex_provider: &TextureProvider,
) -> LoadCellPayload {
    let mut parsed: HashMap<String, Option<PartialNifImport>> = HashMap::new();
    let cells_map = match wctx.record_index.cells.exterior_cells.get(&wctx.worldspace_key) {
        Some(m) => m,
        None => return LoadCellPayload { gx, gy, generation, parsed },
    };
    let Some(cell) = cells_map.get(&(gx, gy)) else {
        return LoadCellPayload { gx, gy, generation, parsed };
    };

    // Unique lowercased model paths in this cell. Reuse across
    // duplicate placements — chairs, lanterns, rocks all share one
    // model path each.
    let mut model_paths: HashSet<String> = HashSet::new();
    for refr in &cell.references {
        let Some(model_path) = wctx
            .record_index
            .cells
            .statics
            .get(&refr.base_form_id)
            .map(|s| s.model_path.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        model_paths.insert(model_path.to_ascii_lowercase());
    }

    // Extract + parse each unique NIF off-thread. Errors are recorded
    // as `None` entries so the drain step caches the negative result
    // and downstream placements skip silently.
    for path in model_paths {
        let Some(bytes) = tex_provider.extract_mesh(&path) else {
            log::debug!("[stream-worker] NIF not in BSA: '{}'", path);
            parsed.insert(path, None);
            continue;
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[stream-worker] NIF parse failed '{}': {}", path, e);
                parsed.insert(path, None);
                continue;
            }
        };
        let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
        let lights = byroredux_nif::import::import_nif_lights(&scene);
        let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
        let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
        parsed.insert(
            path,
            Some(PartialNifImport {
                scene,
                bsx,
                lights,
                particle_emitters,
                embedded_clip,
            }),
        );
    }

    LoadCellPayload {
        gx,
        gy,
        generation,
        parsed,
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

/// Convert a Y-up world-space translation into Bethesda exterior grid
/// coords. 4096 units per cell. The engine's Z-up→Y-up flip negates
/// the source-Y axis when populating world Z, so an exterior placed at
/// source `(2048, 2048, 0)` lands at world `(2048, 0, -2048)` and
/// resolves to grid `(0, 0)`.
pub fn world_pos_to_grid(world_x: f32, world_z: f32) -> (i32, i32) {
    const CELL: f32 = 4096.0;
    let gx = (world_x / CELL).floor() as i32;
    let gy = (-world_z / CELL).floor() as i32;
    (gx, gy)
}

/// Generation-counter decision for an incoming worker payload.
///
/// The drain step (in `main.rs::consume_streaming_payload` and
/// `scene::stream_initial_radius`) compares the payload's generation
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

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
