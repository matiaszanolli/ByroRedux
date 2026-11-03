//! M39 — the host half of texture promotion (docs/engine/texture-streaming.md
//! §4, §10): the renderer's registry asks for a streamed texture's missing
//! levels (`TextureRegistry::take_promotion_requests`), this reads them from
//! the archives on its own small pool, and hands the bytes back
//! (`TextureRegistry::queue_promotion_bytes`), which queues the promotion
//! the per-frame flush uploads.
//!
//! It owns its provider, built from the same archive arguments as every
//! other one, so promotions work in interiors (whose cell-load provider is
//! dropped after the load) as well as exteriors. A BA2 re-read inflates only
//! the chunks the promotion needs (`Ba2Archive::extract_dx10_mips`); a BSA
//! or loose re-read is whole, the cost decision Q2 (b) accepted.

use std::collections::HashSet;
use std::sync::{mpsc, Arc};

use byroredux_renderer::texture_registry::{TextureHandle, TextureRegistry};

use crate::asset_provider::TextureProvider;

/// Reads in flight at once. The pool has two threads, so two queued reads
/// per thread keep it busy without letting a burst of promotion requests
/// stage an unbounded amount of inflated bytes. Phase 3's per-frame cap
/// (TXS-BUD-006) replaces this interim bound with a measured one.
const MAX_READS_IN_FLIGHT: usize = 4;

struct FinishedRead {
    handle: TextureHandle,
    target: u32,
    bytes: Option<Vec<u8>>,
}

pub(crate) struct TextureStreamer {
    provider: Arc<TextureProvider>,
    pool: rayon::ThreadPool,
    in_flight: HashSet<TextureHandle>,
    done_tx: mpsc::Sender<FinishedRead>,
    done_rx: mpsc::Receiver<FinishedRead>,
}

impl TextureStreamer {
    pub(crate) fn new(provider: TextureProvider) -> Option<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|i| format!("texture-stream-{i}"))
            .build()
            .map_err(|e| log::warn!("texture streamer pool: {e}"))
            .ok()?;
        let (done_tx, done_rx) = mpsc::channel();
        Some(Self {
            provider: Arc::new(provider),
            pool,
            in_flight: HashSet::new(),
            done_tx,
            done_rx,
        })
    }

    /// Land the reads that finished since the last frame, then start new
    /// ones for the registry's queued requests, up to the in-flight bound.
    /// Returns (landed, started).
    pub(crate) fn step(&mut self, registry: &mut TextureRegistry) -> (usize, usize) {
        let mut landed = 0;
        while let Ok(read) = self.done_rx.try_recv() {
            self.in_flight.remove(&read.handle);
            let Some(bytes) = read.bytes else {
                log::warn!(
                    "texture promotion read for handle {} found no archive entry",
                    read.handle
                );
                continue;
            };
            if registry.queue_promotion_bytes(read.handle, read.target, bytes) {
                landed += 1;
            } else {
                log::debug!(
                    "texture promotion for handle {} to level {} was not queued (released or already there)",
                    read.handle,
                    read.target,
                );
            }
        }
        let room = MAX_READS_IN_FLIGHT.saturating_sub(self.in_flight.len());
        let mut started = 0;
        for request in registry.take_promotion_requests(room) {
            if !self.in_flight.insert(request.handle) {
                continue;
            }
            let provider = Arc::clone(&self.provider);
            let done = self.done_tx.clone();
            self.pool.spawn(move || {
                let bytes = provider.extract_texture_from(&request.path, request.target);
                // A closed channel means the streamer is gone; nothing to land.
                let _ = done.send(FinishedRead {
                    handle: request.handle,
                    target: request.target,
                    bytes,
                });
            });
            started += 1;
        }
        (landed, started)
    }
}
