//! Size pins for the Vulkan construction and recording drivers that grew past
//! 500 lines. (#6143, regression of #5096 — whose budget covered only
//! `draw_frame`.)
//!
//! Pins are in lines of CODE — blank and comment-only lines are not counted
//! (#6720), so documenting a function never breaks its pin. Each pin sits just
//! above the function's code-line count at the time it was set. A
//! function that was split by #6143 (`build_pipelines_and_finish`,
//! `build_and_upload_instances`), #6708 (`record_skinned_blas_refit`), #6709
//! (`build_blas_batched`) or #6710 (`record_geometry_pass`) is pinned at its
//! post-split size, and the largest phase each of those last three left behind
//! is pinned beside it. Their phase order is pinned next to the code.
//!
//! A new addition that grows one of these must move the work into a helper or
//! a `context/<phase>.rs` sibling and say so here — not raise the pin.

use byroredux_core::source_scan::function_line_count;

#[test]
fn construction_and_recording_drivers_stay_within_their_size_pins() {
    for (source, declaration, pin) in [
        // 1373 before #6143; the per-pass construction is `init_stages.rs`.
        (include_str!("init.rs"), "fn build_pipelines_and_finish(", 395),
        // 1339 before #6143; the flag word, skin/morph fields, batch merge,
        // history gates, blend prewarm and the upload / parameter blocks are
        // helpers in the same file.
        (
            include_str!("build_and_upload_instances.rs"),
            "fn build_and_upload_instances(",
            281,
        ),
        // 971 before #6708; the phases are `SkinChain` methods in the same file.
        (include_str!("skinned_blas_refit.rs"), "fn record_skinned_blas_refit(", 103),
        // 751 before #6710; the phases are `unsafe fn`s beside it.
        (include_str!("geometry_pass.rs"), "fn record_geometry_pass(", 28),
        (include_str!("geometry_pass.rs"), "fn record_main_batches(", 120),
        // 966 before #6709; the phases are methods beside it, in phase order.
        (
            include_str!("../acceleration/blas_static.rs"),
            "fn build_blas_batched(",
            148,
        ),
        (
            include_str!("../acceleration/blas_static.rs"),
            "fn prepare_static_blas_batch(",
            154,
        ),
    ] {
        let measured = function_line_count(source, declaration);
        assert!(
            measured <= pin,
            "`{declaration}` carries {measured} code lines (pin {pin}) — move the new work into a \
             helper or a context/<phase>.rs sibling rather than raising the pin (#6143)"
        );
    }
}
