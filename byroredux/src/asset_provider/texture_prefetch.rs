//! Parallel texture prefetch for the texture-resolve miss path.
//!
//! A texture the registry hasn't loaded is read + inflated on the main thread
//! inside `resolve_texture_view_with_clamp` — measured at 41% of streaming
//! apply time on FO4. Once a streamed NIF's import is finished (external
//! materials merged), its slot paths are final, several budgeted apply slices
//! before the spawn that resolves them. [`prefetch_textures`] queues those
//! paths on the stream pool; the resolve then takes the staged bytes instead
//! of reading the archive.
//!
//! The store is a cache of `TextureProvider::extract(key)` results, keyed by
//! the same canonical key the resolve extracts with, so it cannot change what
//! a resolve returns — only who paid for the read. A key nobody resolves costs
//! wasted pool work; a resolve nobody predicted falls back to the synchronous
//! read. Resolves never wait behind the pool's queue: a still-queued key is
//! withdrawn and read inline, and only a read already running is waited for.

use super::TextureProvider;
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Ceiling on staged-but-unconsumed DDS bytes. Past it, new keys are not
/// queued and finished reads are dropped (their resolve reads inline), so a
/// cell full of never-resolved paths cannot pin unbounded memory. The bytes a
/// consumed entry held move into the registry's pending-upload queue, which
/// already holds them until the cell's batched flush.
const STAGED_BYTE_CAP: usize = 256 << 20;

enum Slot {
    /// Waiting in the pool's queue. A resolve that finds it here withdraws
    /// it and reads inline rather than waiting behind the queue.
    Queued,
    /// A pool thread is reading it; a resolve waits for the result.
    Running,
    Ready(Vec<u8>),
    /// The read finished and the archives do not have the file.
    Missing,
}

#[derive(Default)]
struct StoreState {
    /// Bumped by [`PrefetchStore::clear`]; a task from an older epoch
    /// neither starts nor stores its result.
    epoch: u64,
    slots: HashMap<String, Slot>,
    ready_bytes: usize,
    peak_ready_bytes: usize,
    dropped_over_cap: u64,
    withdrawn: u64,
    unused_at_clear: u64,
}

/// Staged prefetch results, owned by [`TextureProvider`].
#[derive(Default)]
pub(crate) struct PrefetchStore {
    state: Mutex<StoreState>,
    finished: Condvar,
}

/// What [`PrefetchStore::take`] found for a key.
pub(crate) enum Staged {
    Bytes(Vec<u8>),
    Missing,
    /// No prefetch for this key (never queued, withdrawn, dropped over the
    /// cap, or cleared): the caller reads it itself.
    NotStaged,
}

/// Counters for the bench line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PrefetchStats {
    pub peak_ready_bytes: usize,
    pub dropped_over_cap: u64,
    /// Resolves that found their key still queued and read it inline.
    pub withdrawn: u64,
    /// Finished reads no resolve took before their apply ended — keys the
    /// prediction got wrong (or a cancelled cell's).
    pub unused_at_clear: u64,
}

impl PrefetchStore {
    /// Poison-tolerant: every mutation below leaves the map consistent at
    /// each statement, so a panic elsewhere cannot leave it half-updated.
    fn lock(&self) -> MutexGuard<'_, StoreState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queue `key` for a prefetch. Returns the epoch the task runs under,
    /// or `None` when the key is already staged or the store is at its cap.
    fn reserve(&self, key: &str) -> Option<u64> {
        let mut st = self.lock();
        if st.ready_bytes >= STAGED_BYTE_CAP || st.slots.contains_key(key) {
            return None;
        }
        st.slots.insert(key.to_owned(), Slot::Queued);
        Some(st.epoch)
    }

    /// Claim a queued key for reading. `false` when it was withdrawn or
    /// cleared while it sat in the queue.
    fn start(&self, key: &str, epoch: u64) -> bool {
        let mut st = self.lock();
        if st.epoch != epoch {
            return false;
        }
        match st.slots.get_mut(key) {
            Some(slot @ Slot::Queued) => {
                *slot = Slot::Running;
                true
            }
            _ => false,
        }
    }

    fn finish(&self, key: &str, epoch: u64, bytes: Option<Vec<u8>>) {
        let mut st = self.lock();
        if st.epoch == epoch && matches!(st.slots.get(key), Some(Slot::Running)) {
            match bytes {
                Some(bytes) if st.ready_bytes + bytes.len() > STAGED_BYTE_CAP => {
                    st.slots.remove(key);
                    st.dropped_over_cap += 1;
                }
                Some(bytes) => {
                    st.ready_bytes += bytes.len();
                    st.peak_ready_bytes = st.peak_ready_bytes.max(st.ready_bytes);
                    st.slots.insert(key.to_owned(), Slot::Ready(bytes));
                }
                None => {
                    st.slots.insert(key.to_owned(), Slot::Missing);
                }
            }
        }
        drop(st);
        self.finished.notify_all();
    }

    /// Take the staged result for `key`, waiting only while its read is
    /// already running.
    pub(crate) fn take(&self, key: &str) -> Staged {
        let key = fold_key(key);
        let key = key.as_str();
        let mut st = self.lock();
        while matches!(st.slots.get(key), Some(Slot::Running)) {
            st = self.finished.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        match st.slots.remove(key) {
            Some(Slot::Ready(bytes)) => {
                st.ready_bytes -= bytes.len();
                Staged::Bytes(bytes)
            }
            Some(Slot::Missing) => Staged::Missing,
            // Withdrawn: `start` now finds no slot and the task skips it.
            Some(Slot::Queued) => {
                st.withdrawn += 1;
                Staged::NotStaged
            }
            None => Staged::NotStaged,
            Some(Slot::Running) => unreachable!("waited above"),
        }
    }

    /// Drop every staged result and orphan in-flight tasks. Called when the
    /// apply that queued them completes or is cancelled.
    pub(crate) fn clear(&self) {
        let mut st = self.lock();
        st.epoch = st.epoch.wrapping_add(1);
        let unused = st
            .slots
            .values()
            .filter(|slot| matches!(slot, Slot::Ready(_) | Slot::Missing))
            .count();
        st.unused_at_clear += unused as u64;
        st.slots.clear();
        st.ready_bytes = 0;
        drop(st);
        // A waiter can only be the thread that clears (both run on the main
        // thread), but wake defensively so no wait outlives its epoch.
        self.finished.notify_all();
    }

    pub(crate) fn stats(&self) -> PrefetchStats {
        let st = self.lock();
        PrefetchStats {
            peak_ready_bytes: st.peak_ready_bytes,
            dropped_over_cap: st.dropped_over_cap,
            withdrawn: st.withdrawn,
            unused_at_clear: st.unused_at_clear,
        }
    }
}

/// The store's map key: lowercase, backslash-separated. Canonical texture
/// keys keep whatever separators the source authored after the `textures\`
/// root (a NIF slot and its BGSM can spell one file `a\b.dds` and `a/b.dds`),
/// and the archives and texture registry both fold those spellings together,
/// so the store must too or a prefetch misses its own resolve.
fn fold_key(key: &str) -> String {
    key.to_ascii_lowercase().replace('/', "\\")
}

/// Queue each canonical texture key for a background read on `pool`.
/// Returns how many were newly queued.
pub(crate) fn prefetch_textures(
    provider: &Arc<TextureProvider>,
    pool: &rayon::ThreadPool,
    keys: impl IntoIterator<Item = String>,
) -> usize {
    let mut queued = 0;
    for key in keys {
        // The archives fold case and separators the same way, so the folded
        // key extracts the same file.
        let key = fold_key(&key);
        let Some(epoch) = provider.prefetch.reserve(&key) else {
            continue;
        };
        queued += 1;
        let provider = Arc::clone(provider);
        pool.spawn(move || {
            if !provider.prefetch.start(&key, epoch) {
                return;
            }
            // A panicking read must still finish the slot, or a resolve
            // waiting on it would never wake.
            let bytes =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| provider.extract(&key)))
                    .ok()
                    .flatten();
            provider.prefetch.finish(&key, epoch, bytes);
        });
    }
    queued
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queued_key_is_withdrawn_not_waited_for() {
        let store = PrefetchStore::default();
        let epoch = store.reserve("a.dds").unwrap();
        assert!(matches!(store.take("a.dds"), Staged::NotStaged));
        // The task that later dequeues it must skip the read.
        assert!(!store.start("a.dds", epoch));
    }

    #[test]
    fn a_running_read_is_waited_for_and_its_bytes_taken_once() {
        let store = Arc::new(PrefetchStore::default());
        let epoch = store.reserve("a.dds").unwrap();
        assert!(store.start("a.dds", epoch));
        let reader = {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                store.finish("a.dds", epoch, Some(vec![7; 4]));
            })
        };
        assert!(matches!(store.take("a.dds"), Staged::Bytes(b) if b == vec![7; 4]));
        reader.join().unwrap();
        assert!(matches!(store.take("a.dds"), Staged::NotStaged));
        assert_eq!(store.lock().ready_bytes, 0);
    }

    #[test]
    fn a_missing_file_is_staged_as_missing() {
        let store = PrefetchStore::default();
        let epoch = store.reserve("gone.dds").unwrap();
        assert!(store.start("gone.dds", epoch));
        store.finish("gone.dds", epoch, None);
        assert!(matches!(store.take("gone.dds"), Staged::Missing));
    }

    #[test]
    fn clear_discards_results_from_the_old_epoch() {
        let store = PrefetchStore::default();
        let epoch = store.reserve("a.dds").unwrap();
        assert!(store.start("a.dds", epoch));
        store.clear();
        store.finish("a.dds", epoch, Some(vec![1; 8]));
        assert!(matches!(store.take("a.dds"), Staged::NotStaged));
        assert_eq!(store.lock().ready_bytes, 0);
    }

    #[test]
    fn keys_differing_only_in_separators_or_case_share_a_slot() {
        let store = PrefetchStore::default();
        let epoch = store.reserve(&fold_key("textures\\a\\b.dds")).unwrap();
        assert!(store.start("textures\\a\\b.dds", epoch));
        store.finish("textures\\a\\b.dds", epoch, Some(vec![1; 2]));
        assert!(matches!(store.take("Textures\\A/b.DDS"), Staged::Bytes(_)));
    }

    #[test]
    fn a_duplicate_key_is_not_queued_twice() {
        let store = PrefetchStore::default();
        assert!(store.reserve("a.dds").is_some());
        assert!(store.reserve("a.dds").is_none());
    }

    #[test]
    fn reads_past_the_byte_cap_are_dropped_to_the_inline_path() {
        let store = PrefetchStore::default();
        store.lock().ready_bytes = STAGED_BYTE_CAP - 4;
        let epoch = store.reserve("big.dds").unwrap();
        assert!(store.start("big.dds", epoch));
        store.finish("big.dds", epoch, Some(vec![0; 8]));
        assert!(matches!(store.take("big.dds"), Staged::NotStaged));
        assert_eq!(store.stats().dropped_over_cap, 1);
        // At the cap, nothing new is queued.
        store.lock().ready_bytes = STAGED_BYTE_CAP;
        assert!(store.reserve("next.dds").is_none());
    }
}
