//! End-of-frame CPU scratch shrink — extracted from `draw.rs` (#4767 /
//! TD1-2026-09-22-01) to keep `draw_frame` under its size budget. Runs after
//! present, once every per-frame scratch Vec has been restored to the
//! context; the TLAS shrink that sizes itself from the returned instance
//! working set stays in `draw_frame` (see `docs/engine/memory-budget.md`).

use super::VulkanContext;

impl VulkanContext {
    /// Shrink the four per-frame scratch Vecs back toward this frame's
    /// working set and return the instance working set, which the
    /// end-of-frame TLAS shrink sizes to.
    pub(super) fn shrink_frame_scratch(&mut self) -> usize {
        // Restore the scratch buffers to the context so their capacity
        // amortizes across frames (#243), then shrink them back toward
        // the working set after a past peak frame. Same policy as the
        // `tlas_instances_scratch` in #504 — scratch Vecs behave as
        // "grow fast, shrink on pressure": working-set × 2 keeps a
        // slack band against frame-to-frame variance, and the 512
        // floor avoids reallocations on common-case small scenes.
        // #3837 — all four scratch Vecs are restored above, each right after
        // its last use, so none of them is vacated across the error paths
        // between here and there. The shrink policy below is unchanged; it
        // just reads its working-set lengths from the fields now.
        let working_instances = self.scratch.gpu_instances_scratch.len();
        let working_lights = self.scratch.frame_lights_scratch.len();
        let working_previous = self.scratch.previous_models_scratch.len();
        let working_batches = self.scratch.batches_scratch.len();
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.gpu_instances_scratch,
            working_instances,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.frame_lights_scratch,
            working_lights,
            128,
        );
        // #2486 / D5-01 — `previous_models_scratch` was restored here but
        // never shrunk, so it pinned its peak (~16 MB at `MAX_INSTANCES`) for
        // the session. It grows one entry per instance, so its own `len()` is
        // the working set.
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.previous_models_scratch,
            working_previous,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.batches_scratch,
            working_batches,
            512,
        );
        working_instances
    }
}
