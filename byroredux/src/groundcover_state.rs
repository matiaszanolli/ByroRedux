//! Ground-cover state carried by [`crate::App`] (#6694).
//!
//! Per-frame scatter scratch, the model tier's records, the species table and
//! its cache key, the residency ring, and the `--groundcover-*` switches. One
//! `groundcover` field on `App` replaces seventeen `groundcover_*` fields
//! plus `cover_template_draws`. `Default` is the construction contract: every
//! field starts empty, `None`, `0` or `false`.

use byroredux_renderer::vulkan::context::DrawCommand;

/// Ground-cover state. See the module docs.
#[derive(Default)]
pub(crate) struct GroundCoverState {
    /// #4413 — this frame's authored ground-cover template draws (record,
    /// built draw), for the ground-cover model tier. Retained like
    /// `draw_commands`.
    pub(crate) cover_template_draws: Vec<(u32, DrawCommand)>,
    /// #4413 — the model tier's records and table. #4922 — derived once per
    /// installed `AuthoredCover`, not per frame; `model_records_key`
    /// says which one they were derived from.
    pub(crate) model_records:
        Vec<byroredux_renderer::vulkan::groundcover_models::GpuGroundCoverModelRecord>,
    pub(crate) model_selection: Vec<u32>,
    pub(crate) model_records_key: Option<crate::render::groundcover::GroundCoverModelRecordsKey>,
    /// EXAL ground cover (#4054). Per-frame scatter input, kept as App-owned
    /// scratch so the allocations persist across frames — same pattern as
    /// `draw_commands` and the light buffers.
    pub(crate) cells: Vec<byroredux_renderer::vulkan::groundcover::GpuGroundCoverCell>,
    pub(crate) chunks: Vec<byroredux_renderer::vulkan::groundcover::GpuGroundCoverChunk>,
    /// Camera-centred fixed-slab ownership for ground-cover chunks.  Kept on
    /// the app, not rebuilt in the render collector, so a chunk retains its
    /// blade-arena slot until it genuinely leaves the residency ring.
    pub(crate) residency: crate::render::groundcover::GroundCoverResidency,
    pub(crate) collect_scratch: crate::render::groundcover::GroundCoverCollectScratch,
    /// One-shot warning latch for an actual ground-cover capacity fault. Ring
    /// fill-in is normal and is never logged as truncation.
    pub(crate) truncation_logged: bool,
    /// Bindless generated Tier-3 atlas and the palette signature currently in
    /// it. The handle is updated in place on a worldspace palette change, so
    /// it never grows the registry during travel.
    pub(crate) detail_atlas: Option<(u32, u64)>,
    pub(crate) species: Vec<byroredux_renderer::vulkan::groundcover::GpuGroundCoverSpecies>,
    /// The scatter's species selection table (§7), rebuilt alongside
    /// `species` each frame from the palette's climate weights.
    /// #4609 — rebuilt only when `species_table_signature`
    /// moves (palette or climate change), not every frame.
    pub(crate) species_table: Vec<u32>,
    /// #4609 — the O(species) cache key the species-table rebuild is gated
    /// on.
    pub(crate) species_table_signature: u64,
    /// §12.4's per-frame disturber list (#4058). Caller-owned scratch, so the
    /// allocation persists across frames like the chunk and cell vectors.
    pub(crate) disturbers: Vec<byroredux_renderer::vulkan::groundcover::GpuGroundCoverDisturber>,
    /// Render the accepted candidate points instead of blades. §9's Phase 1
    /// view: "this is where the distribution is judged, before any blade
    /// exists". Set by `--groundcover-debug-points`.
    pub(crate) debug_points: bool,
    /// Skip ground cover entirely (`--groundcover-off`). The A/B switch for a
    /// feature that changes every exterior frame.
    pub(crate) off: bool,
    /// EXAL ground-cover §11.1 terrain-attribute sampling bench (#4052).
    /// `Some` under `--bench-groundcover-sampling`, carrying the two sample
    /// counts the flag can tune. The renderer-side harness is created on the
    /// first frame that has a device, and the per-frame terrain publication
    /// is skipped entirely while this is `None`.
    pub(crate) bench: Option<crate::bench::GroundcoverBenchConfig>,
    /// Latched once `enable_groundcover_bench` has been attempted, so a
    /// failure is reported once rather than every frame.
    pub(crate) bench_started: bool,
}

#[cfg(test)]
mod tests {
    /// #6694 — ground-cover state has one home. A new `groundcover_*` field
    /// (or a stray `cover_template_draws`) on `App` would rebuild the sprawl
    /// this struct replaced.
    #[test]
    fn app_declares_no_ground_cover_fields_of_its_own() {
        let main_src = include_str!("main.rs");
        let header = "\nstruct App {";
        let start = main_src.find(header).expect("`struct App` not found") + header.len();
        let body = &main_src[start..];
        let body = &body[..body.find("\n}\n").expect("`struct App` is never closed")];
        assert!(
            body.contains("\n    groundcover: crate::groundcover_state::GroundCoverState,"),
            "App must hold its ground-cover state in the `groundcover` field"
        );
        let strays: Vec<&str> = body
            .lines()
            .map(str::trim_start)
            .filter(|l| {
                !l.starts_with("//")
                    && (l.starts_with("groundcover_") || l.starts_with("cover_template_draws"))
            })
            .collect();
        assert!(
            strays.is_empty(),
            "ground-cover fields belong on GroundCoverState, not App: {strays:?}"
        );
    }
}
