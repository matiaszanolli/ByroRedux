//! Sky-aperture selection for the composite pass (VLM-INT-005): which marked openings reach the GPU
//! when a cell carries more than the composite UBO holds, and the once-per-cell log when some do not.
//! Split out of `frame_params.rs` (#5089's file budget) with its tests.
use super::super::composite::CompositeSkyAperture;

/// VLM-INT-005: keep the apertures nearest the camera, as many as `out` holds, in nearest-first order,
/// and count how many were dropped. Each [`Self::push`] carries the aperture's squared distance to the
/// camera; ties keep their submission order, so which one survives a tie is stable from frame to frame,
/// and `total_cmp` orders NaN last, so a degenerate distance is the first thing dropped.
///
/// The survivors are insertion-sorted straight into `out` with their distances beside them on the
/// stack: no per-frame candidate vector (#6841; #6753 removed the second one).
pub(super) struct NearestApertures<'a> {
    out: &'a mut [CompositeSkyAperture],
    distances: [f32; super::super::composite::MAX_SKY_APERTURES],
    kept: usize,
    dropped: usize,
}

impl<'a> NearestApertures<'a> {
    /// The cap is `out.len()`, at most `MAX_SKY_APERTURES`.
    pub(super) fn new(out: &'a mut [CompositeSkyAperture]) -> Self {
        let cap = out.len().min(super::super::composite::MAX_SKY_APERTURES);
        Self { out: &mut out[..cap], distances: [0.0; super::super::composite::MAX_SKY_APERTURES], kept: 0, dropped: 0 }
    }

    pub(super) fn push(&mut self, distance_sq: f32, aperture: CompositeSkyAperture) {
        let cap = self.out.len();
        // After every kept aperture at or nearer this distance: equal distances keep submission order.
        let at = self.distances[..self.kept].partition_point(|kept| kept.total_cmp(&distance_sq).is_le());
        if at >= cap {
            self.dropped += 1;
            return;
        }
        if self.kept == cap {
            // Full: the farthest survivor makes room.
            self.dropped += 1;
        } else {
            self.kept += 1;
        }
        self.distances.copy_within(at..self.kept - 1, at + 1);
        self.out.copy_within(at..self.kept - 1, at + 1);
        self.distances[at] = distance_sq;
        self.out[at] = aperture;
    }

    /// `(kept, dropped)`; the first `kept` entries of `out` are the survivors, nearest first.
    pub(super) fn finish(self) -> (usize, usize) {
        (self.kept, self.dropped)
    }
}

/// Per-renderer state keyed by the current cell root, not the number dropped.
/// A root changes on a cell load; `None` also separates interior visits across
/// exterior or loading frames. Count changes within one cell do not re-log.
#[derive(Default)]
pub(super) struct ApertureOverflowLog {
    cell: Option<u32>,
    reported: bool,
}

impl ApertureOverflowLog {
    fn should_report(&mut self, cell: Option<u32>, dropped: usize) -> bool {
        if self.cell != cell {
            self.cell = cell;
            self.reported = false;
        }
        if dropped == 0 || self.reported {
            return false;
        }
        self.reported = true;
        true
    }

    pub(super) fn note(&mut self, cell: Option<u32>, dropped: usize, kept: usize) {
        if !self.should_report(cell, dropped) {
            return;
        }
        log::warn!(
            "sky apertures over capacity: {dropped} marked opening(s) dropped, kept the {kept} nearest \
             the camera (MAX_COMPOSITE_SKY_APERTURES, VLM-INT-005)"
        );
    }
}

#[cfg(test)]
mod sky_aperture_overflow_tests {
        use super::*;

    /// An aperture tagged by `id` in a field the selection never touches.
    fn tagged(distance_sq: f32, id: f32) -> (f32, CompositeSkyAperture) {
        (distance_sq, CompositeSkyAperture { half_extents: [id, 0.0, 0.0, 0.0], ..Default::default() })
    }

    /// `NearestApertures` against a `cap`-long output, as the Vec-returning shape the cases read.
    fn nearest(candidates: Vec<(f32, CompositeSkyAperture)>, cap: usize) -> (Vec<CompositeSkyAperture>, usize) {
        let mut out = vec![CompositeSkyAperture::default(); cap];
        let mut nearest = NearestApertures::new(&mut out);
        for (distance_sq, aperture) in candidates {
            nearest.push(distance_sq, aperture);
        }
        let (kept, dropped) = nearest.finish();
        out.truncate(kept);
        (out, dropped)
    }

    fn ids(apertures: &[CompositeSkyAperture]) -> Vec<f32> {
        apertures.iter().map(|a| a.half_extents[0]).collect()
    }

    /// VLM-INT-005: over the cap, the apertures nearest the camera survive, not the ones that came
    /// last, and the count dropped is reported.
    #[test]
    fn the_nearest_apertures_survive_the_cap() {
        // Submission order is deliberately the reverse of distance: the old loop would have kept the
        // farthest three.
        let candidates = (0..6).map(|i| tagged(100.0 - i as f32 * 10.0, i as f32)).collect();
        let (kept, dropped) = nearest(candidates, 3);
        assert_eq!(dropped, 3);
        assert_eq!(ids(&kept), [5.0, 4.0, 3.0], "nearest first");
    }

    #[test]
    fn under_the_cap_nothing_is_dropped_and_the_order_is_nearest_first() {
        let (kept, dropped) = nearest(vec![tagged(9.0, 0.0), tagged(1.0, 1.0), tagged(4.0, 2.0)], 8);
        assert_eq!((dropped, ids(&kept)), (0, vec![1.0, 2.0, 0.0]));
        let (none, dropped) = nearest(Vec::new(), 8);
        assert_eq!((none.len(), dropped), (0, 0));
    }

    /// Equal distances keep their submission order, so the survivor of a tie cannot flicker, and a
    /// non-finite distance is the first thing dropped.
    #[test]
    fn ties_are_stable_and_a_degenerate_distance_goes_first() {
        let candidates = vec![tagged(5.0, 0.0), tagged(f32::NAN, 1.0), tagged(5.0, 2.0), tagged(5.0, 3.0)];
        let (kept, dropped) = nearest(candidates, 3);
        assert_eq!(dropped, 1);
        assert_eq!(ids(&kept), [0.0, 2.0, 3.0]);
    }

    /// The selection matches a full stable sort for any submission order, including evictions from a
    /// full output and ties across the cap boundary.
    #[test]
    fn the_selection_matches_a_stable_sort() {
        let distances = [7.0, 3.0, 3.0, 9.0, 1.0, 3.0, 8.0, 0.5, 3.0, 2.0, 7.0];
        for cap in 0..=distances.len() + 1 {
            let candidates: Vec<_> = distances.iter().enumerate().map(|(i, d)| tagged(*d, i as f32)).collect();
            let mut sorted = candidates.clone();
            sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
            let want: Vec<f32> = sorted.iter().take(cap).map(|(_, a)| a.half_extents[0]).collect();
            let (kept, dropped) = nearest(candidates, cap);
            assert_eq!(ids(&kept), want, "cap {cap}");
            assert_eq!(dropped, distances.len().saturating_sub(cap));
        }
    }

    /// A new cell with the same overflow logs, while changes inside one cell do not.
    #[test]
    fn an_overflow_logs_once_per_cell_even_when_counts_match() {
        let mut log = ApertureOverflowLog::default();
        assert!(!log.should_report(Some(1), 0));
        assert!(log.should_report(Some(1), 7));
        assert!(!log.should_report(Some(1), 9));
        assert!(!log.should_report(Some(1), 0));
        assert!(!log.should_report(Some(1), 7));
        assert!(log.should_report(Some(2), 7));
        assert!(!log.should_report(None, 0));
        assert!(log.should_report(Some(2), 7));
    }
}
