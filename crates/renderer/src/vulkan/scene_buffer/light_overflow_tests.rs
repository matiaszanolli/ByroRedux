//! Regression for PERF-D4-NEW-02 / #1808 — `upload_lights` silently
//! clamped to `MAX_LIGHTS` with no overflow telemetry, unlike every
//! sibling SSBO upload (`upload_instances`, `upload_indirect_draws`,
//! `upload_terrain_tiles`, `upload_materials`), which all warn on
//! truncation. `upload_lights` needs a live Vulkan device + mapped GPU
//! buffer to exercise end-to-end, so this pins the guard at the source
//! level — the same pattern already used for `skin_compute.rs`'s
//! Vulkan-untestable dispatch invariants.

/// `upload_lights` must warn when `lights.len() > MAX_LIGHTS`, using the
/// same `log::warn!` shape as `upload_instances`'s `MAX_INSTANCES` guard,
/// and the check must run before the clamp so the warning fires on the
/// unclamped length.
#[test]
fn upload_lights_warns_on_overflow_like_upload_instances() {
    let src = include_str!("upload.rs");

    let lights_fn_start = src
        .find("pub fn upload_lights(")
        .expect("upload_lights must exist in upload.rs");
    let instances_fn_start = src
        .find("pub fn upload_instances(")
        .expect("upload_instances must exist in upload.rs");
    assert!(
        lights_fn_start < instances_fn_start,
        "test assumes upload_lights is declared before upload_instances in upload.rs"
    );
    let lights_fn_body = &src[lights_fn_start..instances_fn_start];

    assert!(
        lights_fn_body.contains("lights.len() > MAX_LIGHTS"),
        "upload_lights must guard an overflow warning on `lights.len() > MAX_LIGHTS` \
         (PERF-D4-NEW-02 / #1808) — mirroring upload_instances's \
         `instances.len() > MAX_INSTANCES` guard"
    );
    let warn_idx = lights_fn_body
        .find("log::warn!")
        .expect("upload_lights must emit a log::warn! on overflow (PERF-D4-NEW-02 / #1808)");
    let guard_idx = lights_fn_body
        .find("lights.len() > MAX_LIGHTS")
        .unwrap();
    assert!(
        guard_idx < warn_idx,
        "the length-vs-MAX_LIGHTS check must precede the log::warn! call"
    );

    // The overflow check must sit immediately alongside the clamp
    // (top of the function), not buried after the copy — so a reader
    // sees "clamp, then warn" as one unit, matching upload_instances.
    let clamp_idx = lights_fn_body
        .find("lights.len().min(MAX_LIGHTS)")
        .expect("upload_lights must still clamp count to MAX_LIGHTS");
    assert!(
        clamp_idx < guard_idx && guard_idx - clamp_idx < 200,
        "overflow check should sit immediately alongside the clamp, not buried deep in the function"
    );
}
