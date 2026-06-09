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
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use rayon::prelude::*;
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
    pub hole_mask: u16,
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
}

// #1171 / CONC-D6-NEW-05 — compile-time guarantee that
// `PartialNifImport: Send`. The cell-stream worker emits these across
// `mpsc::Sender<LoadCellPayload>`, which requires `Send`. If a future
// contributor adds a non-`Send` field to `NifScene` (e.g. an `Rc<…>`
// for some compositional reason) or to any nested type, this fires at
// the struct's declaration site rather than at the distant channel-
// send call deep inside `cell_pre_parse_worker`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<PartialNifImport>();
};

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
    /// Distant-terrain LOD blocks, keyed by block-coord (#1373). Streamed
    /// each cell-boundary crossing alongside the full-detail cells: blocks
    /// entering the LOD radius spawn, blocks leaving unload, and boundary
    /// blocks whose hole mask changed regenerate. The Slice-1 ring spawned
    /// these once and never tracked them — re-entry leaked ~600 blocks and
    /// the hole-out went stale as the player walked.
    pub lod_blocks: HashMap<(i32, i32), LodBlock>,
    /// Distant **object** LOD quads, keyed by the quad's SW-corner cell
    /// (EXAL step 6). Skyrim+/FO4 only — each entry is the baked `.bto`
    /// macro-mesh's spawned sub-meshes (or an empty sentinel for a quad with
    /// no baked LOD). Streamed alongside `lod_blocks` each cell-boundary
    /// crossing; quads load only outside the full-detail ring.
    pub object_lod_blocks: HashMap<(i32, i32), crate::cell_loader::ObjectLodBlock>,
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
            lod_blocks: HashMap::new(),
            object_lod_blocks: HashMap::new(),
            pending: HashMap::new(),
            next_generation: 0,
            radius_load,
            radius_unload: radius_load + 1,
            last_player_grid: None,
            worker: Some(worker),
            request_tx: Some(request_tx),
            payload_rx,
        }
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

/// Sentinel returned by [`join_with_timeout`] when the joined thread
/// outlives the timeout. Body is unit since the caller doesn't need
/// to recover any state from the thread — its purpose is to signal
/// "detach, log, move on."
#[derive(Debug, PartialEq, Eq)]
pub struct JoinTimeout;

/// `JoinHandle::join` with a wall-clock timeout. Poll-based on
/// [`std::thread::JoinHandle::is_finished`] (stabilised in Rust
/// 1.61) — no auxiliary watcher thread, no `Arc`-held-resource leak
/// on the timeout path. The previous `mpsc::channel` + watcher-
/// thread pattern (#1169) leaked one watcher thread per timeout,
/// each holding the joined `JoinHandle` indefinitely; reaped by the
/// OS at process exit but a real leak on any future non-terminal
/// caller.
///
/// On `Ok(())`, the joined thread has terminated and `join()` has
/// been called (consumes the handle). On `Err(JoinTimeout)`, the
/// handle has been dropped — equivalent to detaching the thread,
/// matching the contract of the old API.
///
/// Poll cadence: 10 ms. With a 1 s timeout (the production caller)
/// that's ≤100 wakeups during shutdown — negligible CPU, and the
/// fast path (worker exits within the first poll) is one extra
/// `is_finished` check vs. an unconditional join.
///
/// Unit-testable without a full streaming setup — see the
/// `join_with_timeout_*` tests below.
pub fn join_with_timeout(
    handle: JoinHandle<()>,
    timeout: std::time::Duration,
) -> Result<(), JoinTimeout> {
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if handle.is_finished() {
            // Swallow a panic in the joined thread — the caller's
            // contract is "thread is done," not "thread succeeded."
            // Panics in worker threads are already surfaced by the
            // worker itself (see `pre_parse_cell_panic_safe`).
            let _ = handle.join();
            return Ok(());
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            // Drop the handle here — detaches the thread, which will
            // exit naturally once its current unit completes. Matches
            // the prior contract: caller can move on.
            drop(handle);
            return Err(JoinTimeout);
        }
        // Sleep up to POLL_INTERVAL but never past the deadline so a
        // short remaining window doesn't overshoot.
        let remaining = deadline.saturating_duration_since(now);
        std::thread::sleep(POLL_INTERVAL.min(remaining));
    }
}

/// Cell pre-parse worker loop. Pulls requests off the channel, does
/// the off-thread work for every NIF the cell references, and emits a
/// single `LoadCellPayload` per request.
///
/// Exits when `request_rx` returns `Err` (sender dropped on
/// `WorldStreamingState` shutdown). Panics inside `pre_parse_cell`
/// are caught and converted into an empty payload — without this
/// guard a single parser-level panic would tear down the worker
/// thread, drop `request_rx`, and silently disable exterior streaming
/// for the rest of the session (#854).
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
            cached_keys,
        } = req;
        let payload = pre_parse_cell_panic_safe(gx, gy, generation, || {
            pre_parse_cell(gx, gy, generation, &wctx, &tex_provider, &cached_keys)
        });
        if payload_tx.send(payload).is_err() {
            // Receiver dropped — main thread is shutting down; exit cleanly.
            break;
        }
    }
    log::info!("cell-stream worker thread exiting");
}

/// Run `f` (the cell pre-parse) inside a panic guard. If `f` panics,
/// log and return an empty payload tagged with the request's
/// coordinates and generation. The drain step still observes the
/// (empty) payload, clears the pending entry, and the streaming loop
/// stays live for the next cell crossing — unlike the pre-#854
/// behaviour where the worker thread died and every subsequent send
/// failed.
fn pre_parse_cell_panic_safe<F>(gx: i32, gy: i32, generation: u64, f: F) -> LoadCellPayload
where
    F: FnOnce() -> LoadCellPayload,
{
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::error!(
            "[stream-worker] panic in pre_parse_cell({}, {}) gen={} — recovered with empty payload (#854)",
            gx,
            gy,
            generation
        );
        LoadCellPayload {
            gx,
            gy,
            generation,
            parsed: HashMap::new(),
        }
    })
}

/// Per-cell pre-parse: walk references, resolve unique model paths,
/// extract NIF bytes from the texture provider's mesh archives, and
/// run the pool-free portion of the NIF import pipeline.
///
/// `cached_keys` is the main-thread snapshot of
/// [`crate::cell_loader::NifImportRegistry`] at request-build time;
/// any model path it contains is skipped here — the drain step's
/// `load_one_exterior_cell` will spawn the cell's REFRs against the
/// cached entries directly, no re-parse needed. See #862.
///
/// Returns a populated [`LoadCellPayload`] (which may have an empty
/// Parse + import a single (path, Option<bytes>) pair. Shared between
/// the serial and parallel branches of `pre_parse_cell` so both paths
/// stay byte-identical — no logic drift between code paths.
///
/// Per-NIF panic guard — converts a parser-level panic into the same
/// `None` failure marker used by the regular `Err` path. Without this,
/// a panic would propagate through rayon's `collect()` and tear down
/// the worker thread (#854). Preserved verbatim across the #877
/// refactor; extracted in #1262 (NIF-D5-NEW-02) to avoid duplicating
/// the closure between the serial / parallel branches.
fn parse_one_nif((path, bytes): (String, Option<Vec<u8>>)) -> (String, Option<PartialNifImport>) {
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(bytes) = bytes else {
            log::debug!("[stream-worker] NIF not in BSA: '{}'", path);
            return None;
        };
        let scene = match byroredux_nif::parse_nif(&bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[stream-worker] NIF parse failed '{}': {}", path, e);
                return None;
            }
        };
        let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
        let root_flags = byroredux_nif::import::extract_root_flags(&scene);
        let lights = byroredux_nif::import::import_nif_lights(&scene);
        let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
        let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
        Some(PartialNifImport {
            scene,
            bsx,
            root_flags,
            lights,
            particle_emitters,
            embedded_clip,
        })
    }))
    .unwrap_or_else(|_| {
        log::error!(
            "[stream-worker] panic parsing NIF '{}' — recording None (#854)",
            path
        );
        None
    });
    (path, parsed)
}

/// `parsed` map if the cell doesn't exist, has no references, or
/// every model path was already cached — the main-thread drain still
/// applies the empty payload so the pending entry is cleared).
#[tracing::instrument(
    name = "pre_parse_cell",
    skip_all,
    fields(gx = gx, gy = gy, generation = generation, cached_count = cached_keys.len()),
)]
fn pre_parse_cell(
    gx: i32,
    gy: i32,
    generation: u64,
    wctx: &ExteriorWorldContext,
    tex_provider: &TextureProvider,
    cached_keys: &HashSet<String>,
) -> LoadCellPayload {
    let mut parsed: HashMap<String, Option<PartialNifImport>> = HashMap::new();
    let cells_map = match wctx
        .record_index
        .cells
        .exterior_cells
        .get(&wctx.worldspace_key)
    {
        Some(m) => m,
        None => {
            return LoadCellPayload {
                gx,
                gy,
                generation,
                parsed,
            }
        }
    };
    let Some(cell) = cells_map.get(&(gx, gy)) else {
        return LoadCellPayload {
            gx,
            gy,
            generation,
            parsed,
        };
    };

    // Unique lowercased model paths in this cell. Reuse across
    // duplicate placements — chairs, lanterns, rocks all share one
    // model path each. Filter out paths already in the main-thread
    // cache snapshot — the drain's `load_one_exterior_cell` spawns
    // them directly from cache without needing the worker to
    // re-produce the import (#862). 7×7 grid traversal in WastelandNV
    // typically sees ~95% cache hits on shared statics, so this slash
    // is dominant for the steady-state workload.
    let mut model_paths: HashSet<String> = HashSet::new();
    let mut skipped_cached = 0usize;
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
        let key = model_path.to_ascii_lowercase();
        if cached_keys.contains(&key) {
            skipped_cached += 1;
            continue;
        }
        model_paths.insert(key);
    }
    if skipped_cached > 0 {
        log::debug!(
            "[stream-worker] cell ({},{}): {} cached models skipped, {} unique to parse",
            gx,
            gy,
            skipped_cached,
            model_paths.len(),
        );
    }

    // Two-phase pre-parse (#877 / NIF-PERF-13):
    //   Phase 1 — SERIAL BSA extract on one thread. The BSA / BA2
    //     readers wrap `File` in `Mutex<File>` (`bsa/archive.rs:119`,
    //     `bsa/ba2.rs:78`), so concurrent `extract_mesh` calls would
    //     queue on the mutex and pay both the lock-acquire overhead
    //     and a context switch per worker — the worst case shape for
    //     a short-blob hot path. Doing the I/O serially on one thread
    //     pays zero lock contention.
    //   Phase 2 — PARALLEL parse + import on the `(path, bytes)` pairs.
    //     The CPU-bound parse / import work fans out cleanly across
    //     rayon workers without any shared-mutex bottleneck.
    //
    // Pre-#877 the entire pipeline ran inside the rayon closure,
    // including the BSA mutex acquire — workers spent most of their
    // wall-clock queued on the mutex on small-NIF-heavy interior
    // cells. Original #830 / NIF-PERF-06 closeout already shipped the
    // ~6-7× single-core → multi-core speedup; this lift on top is the
    // remaining ~10-20% the mutex was eating.
    //
    // Errors are recorded as `None` entries so the drain step caches
    // the negative result and downstream placements skip silently.
    let model_paths: Vec<String> = model_paths.into_iter().collect();

    // Phase 1: serial extract. One BSA mutex acquire per NIF, no
    // contention. `None` for paths the BSA doesn't carry (skipped
    // silently — same semantics as the pre-#877 inline check).
    let extracted: Vec<(String, Option<Vec<u8>>)> = model_paths
        .into_iter()
        .map(|p| {
            let bytes = tex_provider.extract_mesh(&p);
            (p, bytes)
        })
        .collect();

    // Phase 2: parse + import. Each worker owns its `Vec<u8>` for the
    // whole closure — no shared mutex on the hot path.
    //
    // #1262 / NIF-D5-NEW-02 — rayon's worker-wake + join overhead
    // (~50-200 µs typical) dominates at small N. Post-#862 the NIF
    // import cache absorbs most cell-load work and the typical fresh-
    // parse count is 0-6 per cell (the Riverwood log confirms "6 new
    // unique meshes parsed, NIF cache hits/misses 156/6 this cell").
    // Drop to serial iteration below the threshold; keep rayon for
    // session-start fresh-cell bursts where N is genuinely large.
    //
    // Threshold: 8. Empirically chosen against the steady-state
    // streaming pattern — at N≤7 the parallel dispatch is net-loss
    // or break-even; N≥8 the parallel speedup outpaces wake-overhead.
    const PRE_PARSE_RAYON_MIN: usize = 8;
    let results: Vec<(String, Option<PartialNifImport>)> = if extracted.len() < PRE_PARSE_RAYON_MIN
    {
        extracted.into_iter().map(parse_one_nif).collect()
    } else {
        extracted.into_par_iter().map(parse_one_nif).collect()
    };
    parsed.extend(results);

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
    let gx = (world_x / EXTERIOR_CELL_UNITS).floor() as i32;
    let gy = (-world_z / EXTERIOR_CELL_UNITS).floor() as i32;
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
