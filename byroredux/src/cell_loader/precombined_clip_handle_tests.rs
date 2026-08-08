//! Regression test for #2524 / PERF-D3-NEW-01 — the precombined-mesh
//! commit-to-registry step must forward `NifImportRegistry::insert`'s
//! `#[must_use]` `Vec<u32>` return to `AnimationClipRegistry::release`,
//! mirroring the other four production call sites (`partial.rs`,
//! `streaming_helpers.rs`, `references/mod.rs`).
//!
//! `precombined::advance` pulls in a full `VulkanContext` + CSG/BSA
//! archive set and can't run in a unit test — same constraint as
//! `unload_cell` (see `sky_params_cleanup_tests.rs`'s source-scan
//! precedent for the identical problem). Pinned at the source level
//! instead, from a SEPARATE file: binding to `_freed` (the bare-
//! discard-with-an-underscore-prefix shape) satisfies BOTH the
//! `must_use` and `unused_variables` lints simultaneously, so the
//! compiler gives no warning on a silent drop — exactly the shape this
//! bug had. Scanning `precombined.rs`'s source from a sibling file
//! (rather than from a `#[cfg(test)] mod` inside `precombined.rs`
//! itself) avoids the self-reference trap where the test's own
//! assertion string would trivially match its own `include_str!`.

#[test]
fn precombined_insert_forwards_freed_clip_handles_to_release() {
    let src = include_str!("precombined.rs");
    assert!(
        !src.contains("let _freed = reg.insert("),
        "the LRU-eviction return from NifImportRegistry::insert must be \
         bound to a real variable and forwarded to \
         AnimationClipRegistry::release, not silently discarded (#2524)"
    );
    assert!(
        src.contains("clip_reg.release(h)"),
        "advance() must release every LRU-evicted clip handle its own \
         NifImportRegistry::insert call may produce (#2524)"
    );
}
