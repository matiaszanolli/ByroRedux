//! End-of-frame CPU scratch shrink — extracted from `draw.rs` (#4767 /
//! TD1-2026-09-22-01) to keep `draw_frame` under its size budget. Runs after
//! present, once every per-frame scratch Vec has been restored to the
//! context; the TLAS shrink that sizes itself from the returned instance
//! working set stays in `draw_frame` (see `docs/engine/memory-budget.md`).

use super::VulkanContext;

impl VulkanContext {
    /// Shrink the per-frame scratch Vecs back toward this frame's working set
    /// and return the instance working set, which the end-of-frame TLAS
    /// shrink sizes to. Every `Vec` field of `ScratchBuffers` is either shrunk
    /// here or exempted, with its bound, in
    /// `every_draw_count_sized_scratch_vec_is_shrunk` (#4897).
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
        // #5055 — the identity vec is one entry per light (same working set
        // as the lights it parallels) and the resort scratch one decorated
        // tuple per point light.
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.frame_light_ids_scratch,
            working_lights,
            128,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.light_resort_scratch,
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
        // #4897 — both reserve to a per-frame draw-derived count and were
        // never shrunk, so one large exterior frame pinned their peak for the
        // session. The instance map holds one entry per draw command (its own
        // `len()`); the indirect list holds one command per batch, and is only
        // refilled on frames that have batches, so its working set is the
        // batch count rather than its possibly-stale `len()`.
        let working_draws = self.scratch.instance_map_scratch.len();
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.instance_map_scratch,
            working_draws,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.scratch.indirect_draws_scratch,
            working_batches,
            512,
        );
        working_instances
    }
}

#[cfg(test)]
mod tests {
    /// #4897 — every `Vec` field of `ScratchBuffers` must either be shrunk by
    /// `shrink_frame_scratch` or be listed here with the bound that makes a
    /// shrink unnecessary. `instance_map_scratch` (draw-count sized) shipped
    /// with neither, and so did `indirect_draws_scratch` (batch-count sized).
    #[test]
    fn every_draw_count_sized_scratch_vec_is_shrunk() {
        const BOUNDED: &[(&str, &str)] = &[
            (
                "palette_plan_scratch",
                "at most one run per dirty skin slot, capped by the skin slot pool",
            ),
            (
                "skin_dispatches_scratch",
                "one entry per skinned entity dispatched, capped by the skin slot pool",
            ),
            (
                "skin_first_sight_builds_scratch",
                "first-sight skinned BLAS builds, capped by the skin slot pool",
            ),
            (
                "terrain_tile_scratch",
                "at most MAX_TERRAIN_TILES (1024) x 176 B",
            ),
        ];
        let module = include_str!("mod.rs");
        let start = module
            .find("\nstruct ScratchBuffers {")
            .expect("ScratchBuffers is declared in context/mod.rs");
        let body = &module[start..];
        let body = &body[..body.find("\n}\n").expect("ScratchBuffers closes")];
        let vec_fields: Vec<&str> = body
            .lines()
            .filter_map(|line| {
                let (name, ty) = line.trim_start().split_once(": ")?;
                (name.ends_with("_scratch") && ty.starts_with("Vec<")).then_some(name)
            })
            .collect();
        assert!(
            vec_fields.contains(&"instance_map_scratch") && vec_fields.len() >= 8,
            "the ScratchBuffers field scan broke (found {vec_fields:?})"
        );
        let shrink = crate::source_scan::production_text(include_str!("shrink_frame_scratch.rs"));
        for field in vec_fields {
            let shrunk = shrink.contains(&format!("&mut self.scratch.{field},"));
            let exempt = BOUNDED.iter().any(|(name, _)| *name == field);
            assert!(
                shrunk ^ exempt,
                "`{field}`: every ScratchBuffers Vec is either shrunk in \
                 shrink_frame_scratch or exempted with its bound — shrunk={shrunk}, \
                 exempt={exempt} (#4897)"
            );
        }
    }
}
