//! Ground-cover scatter telemetry (extracted from the pipeline module,
//! #4568): the per-frame [`GroundCoverStats`] harvest plus the
//! counter-buffer layout constants the record pass and the shader
//! zero/seed fills share.

use crate::shader_constants::{GROUNDCOVER_HISTOGRAM_BUCKETS, GROUNDCOVER_MAX_CHUNKS};

/// Per-frame scatter telemetry, harvested one pipelined cycle late.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct GroundCoverStats {
    pub(crate) chunks_dispatched: u32,
    /// Chunks lost to an explicit host cell-table capacity fault. A residency
    /// ring filling over several frames is deliberately not counted: the
    /// pending chunks remain queued and will be placed, rather than being
    /// discarded as the old per-frame cap did (#4338).
    pub(crate) chunks_truncated: u32,
    pub(crate) blades_accepted: u32,
    /// Candidates dropped because their chunk's slice was already full.
    /// Non-zero is not a bug — §4 designs for it — but a large fraction means
    /// the cap is below what the density field is asking for.
    pub(crate) blades_overflowed: u32,
    /// Accepted candidates dropped because placed geometry — a road, a
    /// flagstone path, a rock base — covers their root. Zero on open ground;
    /// a large share of `blades` on open ground would mean the test is
    /// hitting the terrain it is meant to stand above.
    pub(crate) blades_covered: u32,
    /// §11.3's `d_ground` histogram over every candidate the field was
    /// evaluated at, accepted or not.
    pub(crate) histogram: [u32; GROUNDCOVER_HISTOGRAM_BUCKETS as usize],
    /// Range of `d_ground` over the frame's candidates.
    pub(crate) d_ground_min: f32,
    pub(crate) d_ground_max: f32,
    /// Range of the view distance the fade was evaluated at. Reported because
    /// an all-bucket-0 histogram with an empty blade count has two completely
    /// different causes — a field that is genuinely near zero, or a field that
    /// is fine with every candidate past `GROUNDCOVER_DRAW_DISTANCE` — and
    /// these two numbers are what tell them apart.
    pub(crate) view_dist_min: f32,
    pub(crate) view_dist_max: f32,
    /// Largest value each of the five §3 factors reached this frame, in
    /// [`GROUNDCOVER_FACTOR_NAMES`] order. A zero here names the term that
    /// annihilated the product — which is the only way a pure product fails,
    /// and the one thing a histogram of the product cannot tell you.
    pub(crate) factor_max: [f32; 5],
}

impl GroundCoverStats {
    /// `groundcover:` summary row, in the same `key=value` shape as the rest
    /// of the bench output.
    pub(crate) fn bench_line(&self) -> String {
        let hist = self
            .histogram
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "groundcover: chunks={} blades={} overflow={} d_ground={:.4}..{:.4} \
             view_dist={:.0}..{:.0} factor_max={} d_ground_hist={} covered={} \
             truncated={}",
            self.chunks_dispatched,
            self.blades_accepted,
            self.blades_overflowed,
            self.d_ground_min,
            self.d_ground_max,
            self.view_dist_min,
            self.view_dist_max,
            GROUNDCOVER_FACTOR_NAMES
                .iter()
                .zip(self.factor_max)
                .map(|(name, value)| format!("{name}:{value:.3}"))
                .collect::<Vec<_>>()
                .join(","),
            hist,
            self.blades_covered,
            self.chunks_truncated
        )
    }
}

/// Counter-buffer layout. `[0 .. MAX_CHUNKS)` are the per-chunk atomic append
/// cursors; then the histogram buckets; then one overflow tally.
pub(crate) const COUNTER_HIST_BASE: usize = GROUNDCOVER_MAX_CHUNKS as usize;
pub(crate) const COUNTER_OVERFLOW: usize = COUNTER_HIST_BASE + GROUNDCOVER_HISTOGRAM_BUCKETS as usize;
/// Four extrema slots after the overflow tally: `d_ground` min/max (fixed
/// point ×1e6) and view-distance min/max (world units). See the scatter's own
/// comment on why a histogram alone cannot separate "the field is uniformly
/// low" from "the field is fine but everything is past the fade".
pub(crate) const COUNTER_EXTREMA_BASE: usize = COUNTER_OVERFLOW + 1;
/// Per-factor maxima, ×1e6, in `GroundCoverFactors` order.
pub(crate) const COUNTER_FACTOR_BASE: usize = COUNTER_EXTREMA_BASE + 4;
pub const GROUNDCOVER_FACTOR_NAMES: [&str; 5] =
    ["affinity", "slope", "moisture", "shelter", "clump"];
/// Accepted candidates the placed-geometry cover test rejected — the
/// scatter's `SLOT_COVERED`.
pub(crate) const COUNTER_COVERED: usize = COUNTER_FACTOR_BASE + GROUNDCOVER_FACTOR_NAMES.len();
pub(crate) const COUNTER_SLOTS: usize = COUNTER_COVERED + 1;
