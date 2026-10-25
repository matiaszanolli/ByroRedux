//! #5092 — the streaming pre-parse worker pipeline: the bounded join,
//! the rayon pool, the per-cell pre-parser and its input budget, split
//! out of the streaming driver. Bootstrap uses the same request and payload
//! path instead of maintaining a second synchronous loader.

use crate::cell_loader::{canonical_model_path_key, ExteriorWorldContext};
use super::telemetry::StreamingWorkerTimings;
use super::{LoadCellPayload, LoadCellRequest, PartialNifImport};
use byroredux_core::string::StringPool;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::asset_provider::TextureProvider;

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

/// Build the dedicated rayon pool the cell-stream worker uses for its
/// pipelined parallel parse (#3089). Half the logical cores, floored at 1
/// so single-core CI runners still get a working (if serial-equivalent)
/// pool.
///
/// What this buys is **isolation**, not a core partition (#3378). The
/// ECS scheduler's `Stage::Update` batch dispatches into rayon's global
/// pool; building a second `ThreadPool` creates an independent registry
/// and takes nothing away from it. Nothing in the workspace calls
/// `ThreadPoolBuilder::build_global`, so the global pool keeps all `N`
/// of its `available_parallelism` threads and a fresh-parse burst puts
/// `N + N/2` rayon workers (plus the cell-stream, main, listener and
/// audio threads) on `N` hardware threads, arbitrated by the OS. What
/// the private pool guarantees is that the burst can never *occupy* a
/// global-pool worker, so `par_iter_mut` in the frame's parallel stages
/// is never starved of workers by streaming — the contention #3089
/// closed. The `N/2` size is a deliberate cap on that oversubscription,
/// not a reservation: a real partition would need
/// `ThreadPoolBuilder::new().num_threads(N/2).build_global()` at boot,
/// at the cost of halving the scheduler's own parallelism.
pub(super) fn build_stream_parse_pool() -> rayon::ThreadPool {
    let total = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let worker_threads = (total / 2).max(1);
    rayon::ThreadPoolBuilder::new()
        .num_threads(worker_threads)
        .thread_name(|i| format!("byro-stream-parse-{i}"))
        .build()
        .expect("failed to build dedicated cell-stream rayon pool")
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
pub(super) fn cell_pre_parse_worker(
    request_rx: mpsc::Receiver<LoadCellRequest>,
    payload_tx: mpsc::Sender<LoadCellPayload>,
    stream_pool: Arc<rayon::ThreadPool>,
    cancel: Arc<AtomicBool>,
) {
    log::info!("cell-stream worker thread started");
    // #3089 — CONC-2026-08-16-01. Built once for the streaming state's
    // life (and shared with the main thread's texture prefetch), not per
    // request: `pre_parse_cell`'s fan-out runs inside
    // `stream_pool.in_place_scope_fifo(..)` instead of rayon's *global*
    // pool, which the ECS scheduler's `Stage::Update` parallel batch
    // (`scheduler.rs`) also dispatches into. Without a dedicated pool the
    // two competed for the same workers the moment a cell crossed
    // `PRE_PARSE_RAYON_MIN` fresh NIFs, defeating the whole point of
    // running cell parsing on its own thread in the first place.
    // A dispatch queues several cells synchronously, so the receiver's
    // backlog is the natural batch boundary. Keep the memo only while that
    // backlog remains non-empty; once recv_next_batch_request observes an
    // empty queue it clears the keys before blocking for the next dispatch.
    // This bounds the set by one dispatch and prevents a later, independent
    // crossing from losing a needed payload to an old memo entry. The
    // premise — a memoed key is resident when a later apply needs it —
    // holds because the main thread salvages a stale payload's imports
    // into the registry instead of dropping them (#5512); before that,
    // a key emitted only in a dropped payload never landed and the later
    // apply fell back to a main-thread parse inside the frame budget.
    let mut batch_keys = HashSet::new();
    // `Geometry.csg` handles for precombine decode, opened once per plugin
    // for the thread's life (M49).
    let mut csg_handles = crate::cell_loader::precombined::CsgHandleCache::default();
    while let Some(req) = recv_next_live_request(&request_rx, &mut batch_keys, &cancel) {
        let LoadCellRequest {
            gx,
            gy,
            generation,
            queued_at,
            wctx,
            tex_provider,
            cached_keys,
        } = req;
        let worker_started = Instant::now();
        let mut payload = pre_parse_cell_panic_safe(gx, gy, generation, || {
            pre_parse_cell(
                gx,
                gy,
                generation,
                &wctx,
                &tex_provider,
                &cached_keys,
                &mut batch_keys,
                &stream_pool,
                &mut csg_handles,
            )
        });
        payload.timings.queue_wait = worker_started.saturating_duration_since(queued_at);
        payload.timings.worker = worker_started.elapsed();
        if payload_tx.send(payload).is_err() {
            // Receiver dropped — main thread is shutting down; exit cleanly.
            break;
        }
    }
    log::info!("cell-stream worker thread exiting");
}

/// [`recv_next_batch_request`], honouring the shutdown cancel flag (#5491).
///
/// Dropping the request sender does NOT stop a worker with a backlog:
/// `mpsc::Receiver::recv` keeps returning every message sent before the
/// disconnect and reports `Disconnected` only once the queue is empty, and
/// the payload receiver lives on the state being shut down, so the worker's
/// `send` keeps succeeding through the whole join timeout. `shutdown` sets
/// `cancel` first; a request received after that is discarded unparsed and
/// the loop ends, bounding the join to the cell already in flight.
pub(crate) fn recv_next_live_request<T>(
    request_rx: &mpsc::Receiver<T>,
    batch_keys: &mut HashSet<String>,
    cancel: &AtomicBool,
) -> Option<T> {
    let req = recv_next_batch_request(request_rx, batch_keys)?;
    if cancel.load(Ordering::Acquire) {
        batch_keys.clear();
        return None;
    }
    Some(req)
}

/// Receive the next request while preserving a worker batch memo only across
/// an already-queued run of requests. The try_recv probe is important: a
/// plain blocking recv cannot tell whether the queue was empty between two
/// dispatches, so a memo would otherwise survive for the worker's whole life.
pub(crate) fn recv_next_batch_request<T>(
    request_rx: &mpsc::Receiver<T>,
    batch_keys: &mut HashSet<String>,
) -> Option<T> {
    if batch_keys.is_empty() {
        return request_rx.recv().ok();
    }

    match request_rx.try_recv() {
        Ok(req) => Some(req),
        Err(mpsc::TryRecvError::Disconnected) => {
            batch_keys.clear();
            None
        }
        Err(mpsc::TryRecvError::Empty) => {
            batch_keys.clear();
            request_rx.recv().ok()
        }
    }
}

/// Run `f` (the cell pre-parse) inside a panic guard. If `f` panics,
/// log and return an empty payload tagged with the request's
/// coordinates and generation. The drain step still observes the
/// (empty) payload, clears the pending entry, and the streaming loop
/// stays live for the next cell crossing — unlike the pre-#854
/// behaviour where the worker thread died and every subsequent send
/// failed.
pub(crate) fn pre_parse_cell_panic_safe<F>(gx: i32, gy: i32, generation: u64, f: F) -> LoadCellPayload
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
            timings: StreamingWorkerTimings::default(),
            parsed: HashMap::new(),
        }
    })
}

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
pub(crate) fn parse_one_nif(
    path: String,
    bytes: Option<Vec<u8>>,
    precombine: bool,
    ctx: &PreParseContext<'_>,
) -> ParsedNifResult {
    let mesh_resolver = ctx.mesh_resolver;
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
        // #6115 — the sync REFR loader warned on a truncated scene; this worker,
        // which parses the exterior statics, cached it silently.
        crate::parsed_nif_cache::note_scene_health(&path, &scene);
        // M49 — a shared-variant precombine carries its geometry in the
        // companion `.csg`; the walk-based import below would produce zero
        // meshes for it. Same decision the main-thread `PrecombinedSpawnJob`
        // makes: CSG decode when it yields meshes, else the ordinary import.
        if precombine {
            let mut worker_pool = StringPool::new();
            if let Some((meshes, geometry_dedup)) =
                crate::cell_loader::precombined::decode_precombine_csg(
                    &scene,
                    ctx.csg_blobs,
                    &mut worker_pool,
                )
            {
                return Some(PartialNifImport {
                    scene,
                    meshes,
                    collisions: Vec::new(),
                    worker_pool,
                    bsx: 0,
                    root_flags: 0,
                    lights: Vec::new(),
                    particle_emitters: Vec::new(),
                    embedded_clip: None,
                    precombine_geometry: Some(geometry_dedup),
                });
            }
        }
        let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
        let root_flags = byroredux_nif::import::extract_root_flags(&scene);
        let lights = byroredux_nif::import::import_nif_lights(&scene);
        let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
        // Standalone controllers only; the sequences ride the scene
        // satellites the main thread extracts (see `references::import`,
        // #3602 / #6253).
        let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
        let mut worker_pool = StringPool::new();
        let (meshes, collisions) =
            byroredux_nif::import::import_nif_with_collision_and_resolver(
                &scene,
                &mut worker_pool,
                Some(mesh_resolver),
            );
        Some(PartialNifImport {
            scene,
            meshes,
            collisions,
            worker_pool,
            bsx,
            root_flags,
            lights,
            particle_emitters,
            embedded_clip,
            precombine_geometry: None,
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

pub(crate) const PRE_PARSE_RAYON_MIN: usize = 8;

pub(crate) type ParsedNifResult = (String, Option<PartialNifImport>);

/// One fresh key the worker pre-parses.
pub(crate) struct PreParseInput {
    key: String,
    /// An FO4 precombine `_oc.nif`: try the `Geometry.csg` decode first.
    precombine: bool,
    /// Decoded size the archive index declares for `key` (0 when no archive
    /// has it) — what the input budget admits the task against before the
    /// task extracts.
    declared_bytes: usize,
}

/// One task's measurements, summed into [`ParsePipelineStats`].
pub(crate) struct PreParseTaskTimings {
    extract: Duration,
    parse: Duration,
    input_bytes: usize,
}

/// Run an archive extraction inside a panic guard (#6119). `parse_one_nif`
/// already turns a parser panic into a negative result for its NIF, but the
/// extraction feeding it ran outside that guard: a reader panic unwound the
/// whole cell's pre-parse, discarded every sibling NIF it had parsed, recorded
/// nothing as failed, and left the main thread to replay the same extraction
/// unguarded. A panic here reads as "not in the archives" instead - the same
/// negative entry the parse step produces.
pub(crate) fn extract_guarded(key: &str, extract: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(extract)).unwrap_or_else(|_| {
        log::error!("[stream-worker] panic extracting NIF '{key}' — treated as missing (#6119)");
        None
    })
}

/// Extract `input`'s bytes and parse them. Runs inside the pool task, so
/// archive reads and inflates proceed in parallel instead of one at a time
/// on the coordinator (the readers are lock-free — positional reads).
pub(crate) fn pre_parse_one(
    input: PreParseInput,
    ctx: &PreParseContext<'_>,
) -> (ParsedNifResult, PreParseTaskTimings) {
    let started = Instant::now();
    let bytes = extract_guarded(&input.key, || ctx.mesh_resolver.extract_mesh(&input.key));
    let extract = started.elapsed();
    let input_bytes = bytes.as_ref().map_or(0, Vec::capacity);
    let parse_started = Instant::now();
    let result = parse_one_nif(input.key, bytes, input.precombine, ctx);
    (
        result,
        PreParseTaskTimings {
            extract,
            parse: parse_started.elapsed(),
            input_bytes,
        },
    )
}

/// What every parse task of one cell borrows.
pub(crate) struct PreParseContext<'a> {
    mesh_resolver: &'a TextureProvider,
    /// `BSPackedGeomObject::filename_hash` → the blob that answers to it,
    /// for the cell's precombine inputs. Empty when the cell has none.
    csg_blobs: &'a HashMap<u32, Arc<byroredux_bsa::CsgArchive>>,
}

/// Bounds decoded input buffers held by queued/running parse tasks. A task is
/// admitted against the size its archive index declares
/// ([`TextureProvider::mesh_declared_size`]) before it extracts, so nothing is
/// allocated ahead of admission. Parsed output, parser scratch, and Starfield
/// external meshes are outside this budget.
pub(crate) const STREAM_PARSE_INPUT_BYTES: usize = 64 * 1024 * 1024;

pub(crate) const STREAM_PARSE_MAX_TASKS: usize = 32;

#[derive(Default)]
pub(crate) struct ParseInputUsage {
    bytes: usize,
    tasks: usize,
    peak_bytes: usize,
    peak_tasks: usize,
}

pub(crate) struct ParseInputBudget {
    usage: Mutex<ParseInputUsage>,
    released: Condvar,
    max_tasks: usize,
}

impl ParseInputBudget {
    fn acquire(&self, bytes: usize) -> ParseInputPermit<'_> {
        let mut usage = self.usage.lock().unwrap_or_else(|e| e.into_inner());
        // An oversized input is admitted alone, so a valid large asset cannot
        // wait forever for a byte limit it can never satisfy.
        while usage.tasks >= self.max_tasks
            || (usage.tasks > 0
                && (usage.bytes > STREAM_PARSE_INPUT_BYTES
                    || bytes > STREAM_PARSE_INPUT_BYTES.saturating_sub(usage.bytes)))
        {
            usage = self.released.wait(usage).unwrap_or_else(|e| e.into_inner());
        }
        usage.bytes += bytes;
        usage.tasks += 1;
        usage.peak_bytes = usage.peak_bytes.max(usage.bytes);
        usage.peak_tasks = usage.peak_tasks.max(usage.tasks);
        ParseInputPermit {
            budget: self,
            bytes,
        }
    }
}

/// The permit outlives the owned input, including during unwinding. Releasing
/// capacity must never depend on the result channel or on parser success.
pub(crate) struct ParseInputPermit<'a> {
    budget: &'a ParseInputBudget,
    bytes: usize,
}

impl Drop for ParseInputPermit<'_> {
    fn drop(&mut self) {
        let mut usage = self.budget.usage.lock().unwrap_or_else(|e| e.into_inner());
        usage.bytes -= self.bytes;
        usage.tasks -= 1;
        self.budget.released.notify_one();
    }
}

#[derive(Default)]
pub(crate) struct ParsePipelineStats {
    /// Sum of task parse durations; overlapping tasks make this different
    /// from wall time and it includes any external-mesh archive waits.
    parse_task_time: Duration,
    /// Sum of task extraction (archive read + inflate) durations; overlaps
    /// like `parse_task_time`.
    extract_task_time: Duration,
    backpressure: Duration,
    peak_input_bytes: usize,
    peak_tasks: usize,
    largest_input: usize,
}

impl ParsePipelineStats {
    fn record_task(&mut self, timings: &PreParseTaskTimings) {
        self.extract_task_time += timings.extract;
        self.parse_task_time += timings.parse;
        self.largest_input = self.largest_input.max(timings.input_bytes);
    }
}

/// Admit each input against the decoded-input budget on the cell coordinator
/// and hand it to the private pool, where the task extracts and parses it.
/// `in_place_scope_fifo` keeps the coordinator off the pool: waiting on its
/// budget cannot occupy the sole worker on small CPUs. The scope joins all
/// tasks before returning or propagating a task panic.
pub(crate) fn parse_nif_pipeline(
    inputs: impl ExactSizeIterator<Item = PreParseInput>,
    stream_pool: &rayon::ThreadPool,
    ctx: &PreParseContext<'_>,
) -> (Vec<ParsedNifResult>, Vec<String>, ParsePipelineStats) {
    let count = inputs.len();
    let mut stats = ParsePipelineStats::default();
    if count < PRE_PARSE_RAYON_MIN {
        let results = inputs
            .map(|input| {
                let (result, timings) = pre_parse_one(input, ctx);
                stats.record_task(&timings);
                stats.peak_input_bytes = stats.largest_input;
                stats.peak_tasks = 1;
                result
            })
            .collect();
        return (results, Vec::new(), stats);
    }

    let budget = ParseInputBudget {
        usage: Mutex::new(ParseInputUsage::default()),
        released: Condvar::new(),
        max_tasks: stream_pool
            .current_num_threads()
            .saturating_mul(2)
            .clamp(1, STREAM_PARSE_MAX_TASKS),
    };
    // Results already constitute the whole cell payload; this channel adds no
    // extra input retention. Indices preserve the serial iterator's output order.
    let (tx, rx) = mpsc::channel();
    stream_pool.in_place_scope_fifo(|scope| {
        for (index, input) in inputs.enumerate() {
            let waiting = Instant::now();
            let permit = budget.acquire(input.declared_bytes);
            stats.backpressure += waiting.elapsed();
            let tx = tx.clone();
            scope.spawn_fifo(move |_| {
                let thread_name = std::thread::current().name().map(str::to_string);
                let (result, timings) = pre_parse_one(input, ctx);
                drop(permit);
                let _ = tx.send((index, result, thread_name, timings));
            });
        }
    });
    drop(tx);
    let mut observed: Vec<_> = rx.into_iter().collect();
    observed.sort_unstable_by_key(|(index, _, _, _)| *index);
    let mut results = Vec::with_capacity(count);
    let mut names = Vec::new();
    for (_, result, name, timings) in observed {
        results.push(result);
        names.extend(name);
        stats.record_task(&timings);
    }
    names.sort_unstable();
    names.dedup();
    let usage = budget.usage.lock().unwrap_or_else(|e| e.into_inner());
    stats.peak_input_bytes = usage.peak_bytes;
    stats.peak_tasks = usage.peak_tasks;
    (results, names, stats)
}

// The dedicated-pool regression fixture's entry point: plain model keys,
// extracted through `mesh_resolver` inside the tasks exactly as in production.
#[cfg(test)]
pub(crate) fn parse_model_keys(
    keys: Vec<String>,
    stream_pool: &rayon::ThreadPool,
    mesh_resolver: &TextureProvider,
) -> (Vec<ParsedNifResult>, Vec<String>) {
    let no_csgs = HashMap::new();
    let ctx = PreParseContext {
        mesh_resolver,
        csg_blobs: &no_csgs,
    };
    let inputs: Vec<PreParseInput> = keys
        .into_iter()
        .map(|key| PreParseInput {
            declared_bytes: mesh_resolver.mesh_declared_size(&key).unwrap_or(0),
            key,
            precombine: false,
        })
        .collect();
    let inputs = inputs.into_iter();
    let (results, names, _) = parse_nif_pipeline(inputs, stream_pool, &ctx);
    (results, names)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreParseModelSkip {
    Cached,
    BatchDuplicate,
}

/// #4207 — per-cell memo of the per-REFR preflight decision, keyed on the
/// raw model-path `&str` (stable: the strings live in the record index).
/// The canonical key is computed at most once per DISTINCT path instead of
/// per REFR — the loop's own comments document ~95% cache hits and heavy
/// per-cell path duplication, so the old shape re-lowercased and
/// re-formatted 2-3 throwaway strings for every duplicate reference.
pub(crate) enum PreflightDecision {
    /// SpeedTree `.spt` — never a NIF; skip before any key work (#3735).
    SkipSpt,
    /// Already cached / already batched — no new key needed.
    Skip(PreParseModelSkip),
    /// First sighting this cell — insert this canonical key.
    Insert(String),
}

/// Whether a MODL path names a SpeedTree `.spt` binary.
///
/// SPT-2026-09-29-D3-02 — this used to slice `path[path.len() - 4..]` by
/// byte index, which panics when the 4th-from-last byte falls inside a
/// multi-byte character: `read_zstring`'s `from_utf8_lossy` turns a cp1252
/// byte into 3-byte U+FFFD, and mod content can author UTF-8 paths. Test the
/// raw bytes instead — a `&[u8]` has no char boundaries.
pub(crate) fn is_spt_model_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 4 && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".spt")
}

/// Apply the two cache layers in the same order as the production
/// pre-parse filter. The request snapshot wins when a key is present in both
/// sets; otherwise the worker memo suppresses a result already emitted by an
/// earlier request in this dispatch batch (#3670).
pub(crate) fn pre_parse_model_skip_reason(
    key: &str,
    cached_keys: &HashSet<String>,
    batch_keys: &HashSet<String>,
) -> Option<PreParseModelSkip> {
    if cached_keys.contains(key) {
        Some(PreParseModelSkip::Cached)
    } else if batch_keys.contains(key) {
        Some(PreParseModelSkip::BatchDuplicate)
    } else {
        None
    }
}

/// The refs a cell's loaded bakes absorb, as the main thread's reference pass
/// will see them (#6097).
///
/// `hashes` / `keys` are the cell's precombine hashes and their `_oc.nif`
/// archive keys, in the same order ([`precombine_oc_nif_paths`]); a bake counts
/// when `bake_exists` finds its `_oc.nif`, so a missing one un-skips exactly its
/// own refs, as [`effective_absorbed_refs`] does for a bake that did not load.
/// A bake that is present but fails to decode is not seen here: its refs lose
/// the prefetch and take the main thread's synchronous parse instead.
///
/// [`precombine_oc_nif_paths`]: crate::cell_loader::precombined::precombine_oc_nif_paths
/// [`effective_absorbed_refs`]: crate::cell_loader::precombined::effective_absorbed_refs
fn bake_absorbed_refs(
    bakes: &[(u32, u32)],
    hashes: &[u32],
    keys: &[String],
    bake_exists: impl Fn(&str) -> bool,
) -> HashSet<u32> {
    let present: Vec<u32> = hashes
        .iter()
        .zip(keys)
        .filter(|(_, key)| bake_exists(key))
        .map(|(&hash, _)| hash)
        .collect();
    crate::cell_loader::precombined::effective_absorbed_refs(bakes, &present)
}

/// Pre-parse one exterior cell's unique, uncached static NIFs for the
/// main-thread streaming drain.
///
/// The coordinator resolves the CELL, canonicalizes and deduplicates its
/// model paths, skips the caller's cache snapshot (#862), and admits each
/// input against the decoded-input budget; each pool task then extracts
/// (archive read + inflate) and parses its own input (#3659).
/// `load_one_exterior_cell` can therefore spawn cached REFR assets directly
/// while consuming this payload only for cache misses.
///
/// Returns a [`LoadCellPayload`] with an empty `parsed` map when the cell is
/// absent, has no references, or every model was cached. The main-thread drain
/// still consumes that empty payload so its pending entry is cleared.
///
/// #6342 — a ref a *present* bake would absorb is parsed after all when its bake did not draw: the
/// first pass parses the bakes and every other model, and the models of the absorbed refs wait for
/// the bakes' results ([`drawn_absorbed_refs`]), a second pass that is empty for a healthy install.
/// What the worker knows of one precombine bake once the first pass has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BakeVerdict {
    /// Not in the archives: draws nothing.
    Absent,
    /// Parsed with at least one mesh: the main thread will spawn it (`count > 0`).
    Drew,
    /// Parsed, but with no mesh (a zero-mesh `_oc.nif`, a CSG-deferred placeholder) or failed to
    /// parse: the main thread's spawn comes back with `count == 0` and its refs render per-REFR.
    DrewNothing,
    /// Not parsed by this request (already cached, or batched from another cell): the worker has no
    /// mesh count, so it assumes the bake draws, as the pre-#6342 predicate did.
    Unknown,
}

/// The refs a cell's bakes *absorb*, as the main thread's `baked_hashes` will see them (#6342): a
/// bake counts when it drew, or when the worker cannot tell ([`BakeVerdict::Unknown`]). Strictly
/// narrower than [`bake_absorbed_refs`], which only asks whether the `_oc.nif` exists.
fn drawn_absorbed_refs(
    bakes: &[(u32, u32)],
    hashes: &[u32],
    keys: &[String],
    verdict: impl Fn(&str) -> BakeVerdict,
) -> HashSet<u32> {
    let drawn: Vec<u32> = hashes
        .iter()
        .zip(keys)
        .filter(|(_, key)| matches!(verdict(key), BakeVerdict::Drew | BakeVerdict::Unknown))
        .map(|(&hash, _)| hash)
        .collect();
    crate::cell_loader::precombined::effective_absorbed_refs(bakes, &drawn)
}

/// The per-model preflight shared by the first pass and the deferred one: a model already cached
/// or already batched is not parsed again, a SpeedTree binary is not a NIF, and anything else is
/// admitted under its canonical key. Memoised per raw path in `decisions` (#4207).
fn admit_model<'a>(
    model_path: &'a str,
    decisions: &mut HashMap<&'a str, PreflightDecision>,
    cached_keys: &HashSet<String>,
    batch_keys: &HashSet<String>,
    model_paths: &mut HashSet<String>,
    skipped_cached: &mut usize,
    skipped_batch_duplicates: &mut usize,
) {
    if !decisions.contains_key(model_path) {
        let d = if is_spt_model_path(model_path) {
            // #3735 — SpeedTree `.spt` binaries are not NIFs; prefetching them
            // was actively harmful (`extract_mesh` missed and wrote a NEGATIVE
            // `NifImportRegistry` entry under the shared cache key, which the
            // sync loader's own resolver then never got past).
            PreflightDecision::SkipSpt
        } else {
            // #3038 — must match the sync REFR loader's key exactly
            // (`references/synth_child.rs`), or the same asset ends up cached
            // under two different `NifImportRegistry` keys and parsed twice.
            let key = canonical_model_path_key(model_path);
            match pre_parse_model_skip_reason(&key, cached_keys, batch_keys) {
                Some(skip) => PreflightDecision::Skip(skip),
                None => PreflightDecision::Insert(key),
            }
        };
        decisions.insert(model_path, d);
    }
    match decisions.get(model_path) {
        Some(PreflightDecision::SkipSpt) => {}
        Some(PreflightDecision::Skip(PreParseModelSkip::Cached)) => *skipped_cached += 1,
        Some(PreflightDecision::Skip(PreParseModelSkip::BatchDuplicate)) => *skipped_batch_duplicates += 1,
        Some(PreflightDecision::Insert(key)) => {
            model_paths.insert(key.clone());
        }
        None => unreachable!("just inserted"),
    }
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(
    name = "pre_parse_cell",
    skip_all,
    fields(gx = gx, gy = gy, generation = generation, cached_count = cached_keys.len()),
)]
pub(crate) fn pre_parse_cell(
    gx: i32,
    gy: i32,
    generation: u64,
    wctx: &ExteriorWorldContext,
    tex_provider: &TextureProvider,
    cached_keys: &HashSet<String>,
    batch_keys: &mut HashSet<String>,
    stream_pool: &rayon::ThreadPool,
    csg_handles: &mut crate::cell_loader::precombined::CsgHandleCache,
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
                timings: StreamingWorkerTimings::default(),
                parsed,
            }
        }
    };
    let Some(cell) = cells_map.get(&(gx, gy)) else {
        return LoadCellPayload {
            gx,
            gy,
            generation,
            timings: StreamingWorkerTimings::default(),
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
    let mut skipped_batch_duplicates = 0usize;
    // M49 — the cell's FO4 precombine `_oc.nif`s, keyed exactly as
    // `PrecombinedSpawnJob` looks them up (the path is already canonical).
    let load_order_paths: Vec<&str> = wctx.plugin_paths.iter().map(String::as_str).collect();
    let precombine_keys = if cell.precombined_mesh_hashes.is_empty() {
        Vec::new()
    } else {
        crate::cell_loader::precombined::precombine_oc_nif_paths(
            cell,
            &wctx.plugin_path,
            &wctx.load_order,
            &load_order_paths,
        )
    };
    // #6097 — the refs whose geometry a bake present in the archives absorbs.
    // The main thread's reference pass drops them before any model lookup, so
    // their models are never spawned; parsing them here (95% of an FO4
    // exterior's STAT/SCOL placements) is pure worker and drain cost.
    let absorbed_refs = bake_absorbed_refs(
        &cell.absorbed_ref_bakes,
        &cell.precombined_mesh_hashes,
        &precombine_keys,
        |key| tex_provider.has_mesh(key),
    );
    // Refs a present bake may absorb, held back until the first pass shows whether it drew (#6342).
    let mut deferred_absorbed: Vec<(u32, &str)> = Vec::new();
    let mut decisions: std::collections::HashMap<&str, PreflightDecision> = HashMap::new();
    for refr in &cell.references {
        let Some(object) = wctx.record_index.cells.statics.get(&refr.base_form_id) else {
            continue;
        };
        let model_path = object.model_path.as_str();
        if model_path.is_empty() {
            continue;
        }
        if absorbed_refs.contains(&refr.form_id)
            && crate::cell_loader::references::precombine_can_replace_record(Some(object.record_type))
        {
            // A present bake probably absorbs this ref, but only a bake that draws does: wait for
            // the bakes' results before deciding whether its model is needed (#6342).
            deferred_absorbed.push((refr.form_id, model_path));
            continue;
        }
        // #3735 / #4207 / #3038 — see `admit_model`.
        admit_model(
            model_path,
            &mut decisions,
            cached_keys,
            batch_keys,
            &mut model_paths,
            &mut skipped_cached,
            &mut skipped_batch_duplicates,
        );
    }
    // Pre-parsing the precombines moves their read, parse and CSG decode off the
    // main thread; the drain's precombine merge and the job's cache hit do
    // the rest. They go first: CSG decode is the heaviest task per input.
    let mut precombine_paths: Vec<String> = Vec::new();
    let mut csg_blobs = HashMap::new();
    // Bakes this request does not parse (cached, or batched from another cell) have no mesh count
    // here; the first pass's verdict treats them as drawing.
    let unparsed_bakes: HashSet<String> = precombine_keys
        .iter()
        .filter(|key| pre_parse_model_skip_reason(key, cached_keys, batch_keys).is_some())
        .cloned()
        .collect();
    let bake_keys = precombine_keys.clone();
    if !precombine_keys.is_empty() {
        let mut seen = HashSet::new();
        for key in precombine_keys {
            match pre_parse_model_skip_reason(&key, cached_keys, batch_keys) {
                Some(PreParseModelSkip::Cached) => skipped_cached += 1,
                Some(PreParseModelSkip::BatchDuplicate) => skipped_batch_duplicates += 1,
                None if seen.insert(key.clone()) => precombine_paths.push(key),
                None => {}
            }
        }
        if !precombine_paths.is_empty() {
            csg_blobs =
                csg_handles.blobs_for_cell(
                    cell.form_id,
                    &wctx.plugin_path,
                    &wctx.load_order,
                    &load_order_paths,
                );
        }
    }
    if skipped_cached > 0 || skipped_batch_duplicates > 0 || !deferred_absorbed.is_empty() {
        log::debug!(
            "[stream-worker] cell ({},{}): {} cached models skipped, {} batch duplicates skipped, \
             {} refs held for their bakes, {} unique to parse",
            gx,
            gy,
            skipped_cached,
            skipped_batch_duplicates,
            deferred_absorbed.len(),
            model_paths.len() + precombine_paths.len(),
        );
    }

    // The coordinator only admits inputs against the decoded-input budget;
    // each pool task extracts (archive read + inflate) and parses its own
    // input. BSA and BA2 both extract through positional reads with no
    // per-archive lock (#3659), so inflates run in parallel (and reads too on
    // Unix — Windows serialises reads on one handle, #4999), and
    // parse starts as soon as a NIF is available instead of behind an
    // extract-all barrier that retained the whole cell's decoded input.
    // Archive lookup precedence is the provider's (`extract_mesh` /
    // `mesh_declared_size` agree on it). Cells with fewer than eight fresh
    // inputs keep the serial fast path.
    let precombine_count = precombine_paths.len();
    let inputs: Vec<PreParseInput> = precombine_paths
        .into_iter()
        .map(|key| (key, true))
        .chain(model_paths.into_iter().map(|key| (key, false)))
        .map(|(key, precombine)| PreParseInput {
            declared_bytes: tex_provider.mesh_declared_size(&key).unwrap_or(0),
            key,
            precombine,
        })
        .collect();
    let input_count = inputs.len();
    let pipeline_started = Instant::now();
    let ctx = PreParseContext {
        mesh_resolver: tex_provider,
        csg_blobs: &csg_blobs,
    };
    let (results, parallel_parse_threads, pipeline) =
        parse_nif_pipeline(inputs.into_iter(), stream_pool, &ctx);
    if input_count > 0 {
        log::debug!(
            "[stream-worker] cell ({gx},{gy}) pipeline: inputs={input_count} \
             precombines={precombine_count} wall_ms={:.3} \
             extract_task_sum_ms={:.3} parse_task_sum_ms={:.3} backpressure_ms={:.3} \
             peak_task_input_bytes={} largest_input_bytes={} peak_tasks={}",
            pipeline_started.elapsed().as_secs_f64() * 1000.0,
            pipeline.extract_task_time.as_secs_f64() * 1000.0,
            pipeline.parse_task_time.as_secs_f64() * 1000.0,
            pipeline.backpressure.as_secs_f64() * 1000.0,
            pipeline.peak_input_bytes,
            pipeline.largest_input,
            pipeline.peak_tasks,
        );
    }
    // Record only keys for which this request emitted a result, including a
    // negative result. A parser panic caught by the outer request guard
    // therefore does not poison the rest of the batch with keys it never
    // produced.
    batch_keys.extend(results.iter().map(|(key, _)| key.clone()));
    parsed.extend(results);

    // #6342 — the second pass. A ref's model is skipped only if its bake actually drew, which is
    // what the main thread's `baked_hashes` records (`count > 0`); a bake that is in the archives
    // but drew nothing (a zero-mesh `_oc.nif`, a CSG-deferred placeholder, a parse failure) leaves
    // its refs to render per-REFR, and without their models parsed here each unique one takes an
    // inline extract + parse on the main thread, the cost this worker exists to remove.
    let mut parallel_parse_threads = parallel_parse_threads;
    if !deferred_absorbed.is_empty() {
        let drawn = drawn_absorbed_refs(&cell.absorbed_ref_bakes, &cell.precombined_mesh_hashes, &bake_keys, |key| {
            if !tex_provider.has_mesh(key) {
                BakeVerdict::Absent
            } else if unparsed_bakes.contains(key) {
                BakeVerdict::Unknown
            } else {
                match parsed.get(key) {
                    Some(Some(import)) if !import.meshes.is_empty() => BakeVerdict::Drew,
                    _ => BakeVerdict::DrewNothing,
                }
            }
        });
        let mut late_paths: HashSet<String> = HashSet::new();
        let (mut late_cached, mut late_batched) = (0usize, 0usize);
        for &(form_id, model_path) in &deferred_absorbed {
            if !drawn.contains(&form_id) {
                admit_model(
                    model_path,
                    &mut decisions,
                    cached_keys,
                    batch_keys,
                    &mut late_paths,
                    &mut late_cached,
                    &mut late_batched,
                );
            }
        }
        skipped_batch_duplicates += late_batched;
        if !late_paths.is_empty() {
            log::debug!(
                "[stream-worker] cell ({gx},{gy}): {} models of refs whose bake drew nothing parsed after all",
                late_paths.len()
            );
            let inputs: Vec<PreParseInput> = late_paths
                .into_iter()
                .map(|key| PreParseInput {
                    declared_bytes: tex_provider.mesh_declared_size(&key).unwrap_or(0),
                    key,
                    precombine: false,
                })
                .collect();
            let (late_results, late_threads, _stats) = parse_nif_pipeline(inputs.into_iter(), stream_pool, &ctx);
            batch_keys.extend(late_results.iter().map(|(key, _)| key.clone()));
            parsed.extend(late_results);
            parallel_parse_threads.extend(late_threads);
        }
    }

    LoadCellPayload {
        gx,
        gy,
        generation,
        timings: StreamingWorkerTimings {
            parallel_parse_threads,
            batch_duplicate_skips: skipped_batch_duplicates,
            ..StreamingWorkerTimings::default()
        },
        parsed,
    }
}

#[cfg(test)]
mod extraction_guard_tests {
    use super::extract_guarded;

    /// #6119 - a reader panic during extraction is contained to its own NIF
    /// (reads as missing), not propagated to take the whole cell with it.
    #[test]
    fn a_panicking_extraction_reads_as_missing() {
        assert_eq!(extract_guarded("a.nif", || Some(vec![1, 2, 3])), Some(vec![1, 2, 3]));
        assert_eq!(extract_guarded("b.nif", || None), None);
        let panicked = extract_guarded("c.nif", || -> Option<Vec<u8>> { panic!("reader bug") });
        assert_eq!(panicked, None);
    }
}

/// #6104 - the admission control is a `Mutex` + `Condvar` pair whose failure
/// shape (the coordinator parked forever, so streaming silently stops) no other
/// guard can see, and the one test that crosses the rayon threshold feeds it
/// zero-byte inputs, so no limit ever binds. These drive `ParseInputBudget`
/// alone: pure CPU, no archives.
#[cfg(test)]
mod parse_input_budget_tests {
    use super::*;
    use std::time::Duration;

    fn budget(max_tasks: usize) -> ParseInputBudget {
        ParseInputBudget {
            usage: Mutex::new(ParseInputUsage::default()),
            released: Condvar::new(),
            max_tasks,
        }
    }

    fn usage(budget: &ParseInputBudget) -> (usize, usize) {
        let usage = budget.usage.lock().unwrap();
        (usage.tasks, usage.bytes)
    }

    /// `acquire(bytes)` on another thread, admitted straight away? A request
    /// that cannot be admitted would park its thread - and the scope joining
    /// it - for good, so a `false` answer is only available through
    /// [`blocks_until`], which frees it.
    fn admitted_within(budget: &ParseInputBudget, bytes: usize, wait: Duration) -> bool {
        std::thread::scope(|scope| {
            let (done, admitted) = mpsc::channel();
            scope.spawn(move || {
                let permit = budget.acquire(bytes);
                let _ = done.send(());
                drop(permit);
            });
            admitted.recv_timeout(wait).is_ok()
        })
    }

    /// `acquire(bytes)` must wait: it is not admitted within [`NOT_ADMITTED`];
    /// then `release` frees capacity and it must be admitted promptly.
    fn blocks_until(budget: &ParseInputBudget, bytes: usize, release: impl FnOnce()) {
        std::thread::scope(|scope| {
            let (done, admitted) = mpsc::channel();
            scope.spawn(move || {
                let permit = budget.acquire(bytes);
                let _ = done.send(());
                drop(permit);
            });
            assert!(admitted.recv_timeout(NOT_ADMITTED).is_err(), "{bytes} bytes must wait");
            release();
            assert!(admitted.recv_timeout(ADMITTED).is_ok(), "and be admitted once capacity returns");
        });
    }

    const NOT_ADMITTED: Duration = Duration::from_millis(150);
    const ADMITTED: Duration = Duration::from_secs(10);

    #[test]
    fn the_task_limit_blocks_until_a_permit_drops() {
        let budget = budget(2);
        let first = budget.acquire(1);
        let second = budget.acquire(1);
        assert_eq!(usage(&budget), (2, 2));
        blocks_until(&budget, 1, || drop(first));
        drop(second);
        assert_eq!(usage(&budget), (0, 0));
    }

    #[test]
    fn the_byte_cap_blocks_an_input_that_does_not_fit() {
        let budget = budget(8);
        let half = STREAM_PARSE_INPUT_BYTES / 2 + 1;
        let held = budget.acquire(half);
        // A small one still fits beside it ...
        assert!(admitted_within(&budget, 1024, ADMITTED));
        // ... a second half-cap input does not, until the first drops.
        blocks_until(&budget, half, || drop(held));
        assert_eq!(usage(&budget), (0, 0));
    }

    /// A single asset bigger than the whole cap must still load: admitted alone,
    /// and nothing else is admitted beside it.
    #[test]
    fn an_oversized_input_is_admitted_only_alone_and_blocks_the_next() {
        let budget = budget(8);
        let huge = STREAM_PARSE_INPUT_BYTES + 4096;
        let holding = budget.acquire(huge);
        assert_eq!(usage(&budget).0, 1, "admitted with nothing else in flight");
        // Even a tiny input waits behind it ...
        blocks_until(&budget, 1, || drop(holding));

        // ... and it does not squeeze in beside a running task either.
        let small = budget.acquire(16);
        blocks_until(&budget, huge, || drop(small));
        assert_eq!(usage(&budget), (0, 0));
    }

    /// The permit lives in the task closure, so a parser panic unwinds through
    /// its drop and capacity is returned.
    #[test]
    fn a_permit_moved_into_a_panicking_task_is_released() {
        let budget = budget(1);
        let permit = budget.acquire(4096);
        assert_eq!(usage(&budget), (1, 4096));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _permit = permit;
            panic!("parser bug");
        }));
        assert!(outcome.is_err());
        assert_eq!(usage(&budget), (0, 0), "the unwind released the permit");
        assert!(admitted_within(&budget, 1, ADMITTED), "so the next task is admitted");
    }
}

/// #6097 - which refs the worker may leave unparsed.
#[cfg(test)]
mod absorbed_ref_tests {
    use super::{bake_absorbed_refs, drawn_absorbed_refs, BakeVerdict};

    fn keys(hashes: &[u32]) -> Vec<String> {
        hashes.iter().map(|hash| format!("meshes/precombined/{hash:08x}_oc.nif")).collect()
    }

    #[test]
    fn a_present_bake_absorbs_exactly_its_own_refs() {
        let bakes = [(1, 0xA), (2, 0xA), (3, 0xB)];
        let absorbed = bake_absorbed_refs(&bakes, &[0xA, 0xB], &keys(&[0xA, 0xB]), |_| true);
        assert_eq!(absorbed, [1, 2, 3].into_iter().collect());
    }

    /// A bake whose `_oc.nif` is not in the archives drew nothing, so its refs
    /// render per-REFR and their models must still be parsed.
    #[test]
    fn a_missing_bake_leaves_its_refs_to_their_models() {
        let bakes = [(1, 0xA), (2, 0xB)];
        let missing_a = keys(&[0xA]).remove(0);
        let absorbed =
            bake_absorbed_refs(&bakes, &[0xA, 0xB], &keys(&[0xA, 0xB]), |key| key != missing_a);
        assert_eq!(absorbed, [2].into_iter().collect());
    }

    /// An XCRI ref naming a hash the cell does not list has no bake to absorb it.
    #[test]
    fn a_cell_without_precombines_absorbs_nothing() {
        assert!(bake_absorbed_refs(&[(1, 0xA)], &[], &[], |_| true).is_empty());
        assert!(bake_absorbed_refs(&[(1, 0xC)], &[0xA], &keys(&[0xA]), |_| true).is_empty());
    }

    /// #6342 - a bake in the archives that draws nothing (a zero-mesh `_oc.nif`, a CSG-deferred
    /// placeholder, a parse failure) is not a carrier of its refs' geometry: the main thread
    /// renders them per-REFR, so the worker must still have their models. `bake_absorbed_refs`
    /// (presence only) says they are absorbed; `drawn_absorbed_refs` (what the main thread's
    /// `count > 0` records) says otherwise.
    #[test]
    fn a_bake_that_exists_but_draws_nothing_does_not_absorb_its_refs() {
        let bakes = [(1, 0xA), (2, 0xA), (3, 0xB), (4, 0xC)];
        let hashes = [0xA, 0xB, 0xC];
        let paths = keys(&hashes);
        let by_hash = |verdicts: [BakeVerdict; 3]| {
            let paths = paths.clone();
            move |key: &str| verdicts[paths.iter().position(|p| p == key).expect("a listed bake")]
        };

        // All three are in the archives: the presence predicate absorbs every ref ...
        assert_eq!(bake_absorbed_refs(&bakes, &hashes, &paths, |_| true).len(), 4);
        // ... but bake A parsed to no mesh (CSG-deferred) and bake B failed to parse.
        let drawn = drawn_absorbed_refs(
            &bakes,
            &hashes,
            &paths,
            by_hash([BakeVerdict::DrewNothing, BakeVerdict::DrewNothing, BakeVerdict::Drew]),
        );
        assert_eq!(drawn, [4].into_iter().collect(), "only the bake that drew absorbs its refs");
        assert!(!drawn.contains(&1) && !drawn.contains(&2), "A's refs keep their models");
        assert!(!drawn.contains(&3), "B's ref keeps its model");
    }

    /// A bake this request did not parse (cached, or batched from another cell) has no mesh count
    /// here: it keeps absorbing, as before #6342; an absent one never does.
    #[test]
    fn an_unparsed_bake_is_assumed_to_draw_and_an_absent_one_does_not() {
        let bakes = [(1, 0xA), (2, 0xB)];
        let paths = keys(&[0xA, 0xB]);
        let absent_b = paths[1].clone();
        let drawn = drawn_absorbed_refs(&bakes, &[0xA, 0xB], &paths, |key| {
            if key == absent_b {
                BakeVerdict::Absent
            } else {
                BakeVerdict::Unknown
            }
        });
        assert_eq!(drawn, [1].into_iter().collect());
    }

    /// The loop applies the main thread's type gate, not the absorbed set alone:
    /// an interactive record in an XCRI pair keeps its runtime identity there, so
    /// the worker must keep its model.
    #[test]
    fn the_worker_uses_the_main_threads_type_gate() {
        use byroredux_plugin::RecordType;
        use crate::cell_loader::references::precombine_can_replace_record as gate;
        assert!(gate(Some(RecordType::STAT)) && gate(Some(RecordType::SCOL)));
        assert!(!gate(Some(RecordType::FURN)) && !gate(None));
        let source = byroredux_core::source_scan::strip_test_modules(include_str!("pre_parse.rs"));
        let loop_body = source.split_once("for refr in &cell.references").expect("the preflight loop").1;
        assert!(
            loop_body.contains("deferred_absorbed.push(")
                && source.contains("drawn_absorbed_refs(&cell.absorbed_ref_bakes"),
            "an absorbed ref is deferred to the post-bake pass, not dropped outright (#6342)"
        );
        assert!(
            loop_body.contains("absorbed_refs.contains(&refr.form_id)")
                && loop_body.contains("precombine_can_replace_record"),
            "the preflight loop must hold back a STAT/SCOL ref the bakes absorb (#6097 / #6342)"
        );
    }
}
