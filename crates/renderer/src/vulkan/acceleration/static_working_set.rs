//! Current-frame ownership is independent of the LRU clock. Several BLAS
//! batches may advance that clock before the upcoming TLAS has been built.

pub(super) struct StaticBlasWorkingSet {
    /// Current generation for each dense mesh-handle slot. Handles are
    /// registry slot IDs, so direct indexing replaces per-frame hashing;
    /// zero means the slot has not been marked in the current generation.
    stamps: Vec<u32>,
    generation: u32,
}

impl Default for StaticBlasWorkingSet {
    fn default() -> Self {
        Self {
            stamps: Vec::new(),
            generation: 1,
        }
    }
}

impl StaticBlasWorkingSet {
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamps.fill(0);
            self.generation = 1;
        }
    }

    pub fn insert(&mut self, handle: u32) {
        let index = handle as usize;
        if self.stamps.len() <= index {
            self.stamps.resize(index + 1, 0);
        }
        self.stamps[index] = self.generation;
    }

    pub fn replace(&mut self, handles: &[u32]) {
        self.clear();
        for &handle in handles {
            self.insert(handle);
        }
    }

    pub fn can_evict(&self, handle: u32, last_used: u64, current: u64, min_idle: u64) -> bool {
        self.stamps.get(handle as usize) != Some(&self.generation)
            && current.saturating_sub(last_used) >= min_idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_batches_cannot_age_current_casters_into_eviction() {
        let mut working = StaticBlasWorkingSet::default();
        // Handle 12 can be missing at collection time; it is protected as
        // soon as a later recovery batch creates its BLAS too.
        working.replace(&[11, 12, 13]);
        for batch_clock in 101..110 {
            for handle in [11, 12, 13] {
                assert!(
                    !working.can_evict(handle, 100, batch_clock, 3),
                    "current caster {handle} became idle during batch clock {batch_clock}"
                );
            }
        }
        assert!(
            working.can_evict(99, 100, 109, 3),
            "unused cache must remain reclaimable"
        );
    }

    #[test]
    fn next_draw_set_releases_old_casters_and_protects_new_ones() {
        let mut working = StaticBlasWorkingSet::default();
        working.replace(&[11, 12]);
        working.replace(&[12, 13]);
        assert!(working.can_evict(11, 100, 109, 3));
        assert!(!working.can_evict(12, 100, 109, 3));
        assert!(!working.can_evict(13, 0, 109, 3));
        working.replace(&[]);
        assert!(working.can_evict(12, 100, 109, 3));
    }

    #[test]
    fn unused_cache_still_obeys_the_idle_window() {
        let working = StaticBlasWorkingSet::default();
        assert!(!working.can_evict(1, 100, 102, 3));
        assert!(working.can_evict(1, 100, 103, 3));
        assert!(!working.can_evict(1, 100, 99, 3));
    }

    #[test]
    fn tlas_collection_refreshes_membership_without_relying_on_recovery() {
        let mut working = StaticBlasWorkingSet::default();
        working.replace(&[1]);
        // The between-frame restore saw last frame's draws. The actual TLAS
        // now sees a newly visible caster, including one not built yet.
        working.clear();
        working.insert(2);
        working.insert(2);
        assert!(working.can_evict(1, 100, 109, 3));
        assert!(!working.can_evict(2, 0, 109, 3));
        working.clear();
        assert!(working.can_evict(2, 0, 109, 3));
    }
}
