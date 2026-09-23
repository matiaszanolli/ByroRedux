//! Current-frame ownership is independent of the LRU clock. Several BLAS
//! batches may advance that clock before the upcoming TLAS has been built.

use rustc_hash::FxHashSet;

#[derive(Default)]
pub(super) struct StaticBlasWorkingSet {
    handles: FxHashSet<u32>,
}

impl StaticBlasWorkingSet {
    pub fn clear(&mut self) {
        self.handles.clear();
    }

    pub fn insert(&mut self, handle: u32) {
        self.handles.insert(handle);
    }

    pub fn replace(&mut self, handles: &[u32]) {
        self.clear();
        self.handles.extend(handles.iter().copied());
    }

    pub fn can_evict(&self, handle: u32, last_used: u64, current: u64, min_idle: u64) -> bool {
        !self.handles.contains(&handle) && current.saturating_sub(last_used) >= min_idle
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
