//! Size pins for the Vulkan construction and recording drivers that grew past
//! 500 lines. (#6143, regression of #5096 — whose budget covered only
//! `draw_frame`.)
//!
//! Each pin sits just above the function's size at the time it was set. A
//! function that was split by #6143 (`build_pipelines_and_finish`,
//! `build_and_upload_instances`) is pinned at its post-split size; the three
//! that were *not* split — `record_skinned_blas_refit`, `build_blas_batched`
//! and `record_geometry_pass` — are pinned where they stand, so growth is a
//! deliberate edit. Their extraction records barrier and bracket order and
//! failure-path cleanup that only `BYRO_VALIDATION=1` on a device can verify,
//! so it is left to a session that can run one rather than done blind.
//!
//! A new addition that grows one of these must move the work into a helper or
//! a `context/<phase>.rs` sibling and say so here — not raise the pin.

use byroredux_core::source_scan::function_line_count;

#[test]
fn construction_and_recording_drivers_stay_within_their_size_pins() {
    for (source, declaration, pin) in [
        // 1373 before #6143; the per-pass construction is `init_stages.rs`.
        (include_str!("init.rs"), "fn build_pipelines_and_finish(", 550),
        // 1339 before #6143; the flag word, skin/morph fields, batch merge,
        // history gates, blend prewarm and the upload / parameter blocks are
        // helpers in the same file.
        (
            include_str!("build_and_upload_instances.rs"),
            "fn build_and_upload_instances(",
            555,
        ),
        // Not split (see the module docs).
        (include_str!("skinned_blas_refit.rs"), "fn record_skinned_blas_refit(", 985),
        (include_str!("geometry_pass.rs"), "fn record_geometry_pass(", 770),
        (
            include_str!("../acceleration/blas_static.rs"),
            "fn build_blas_batched(",
            985,
        ),
    ] {
        let measured = function_line_count(source, declaration);
        assert!(
            measured <= pin,
            "`{declaration}` carries {measured} lines (pin {pin}) — move the new work into a \
             helper or a context/<phase>.rs sibling rather than raising the pin (#6143)"
        );
    }
}
