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

    // #7385: the guard is the episode latch over the unclamped length and the clamped count,
    // so the warn fires once per overflow episode rather than on every frame.
    assert!(
        lights_fn_body.contains("let count = lights.len().min(MAX_LIGHTS);")
            && lights_fn_body.contains("overflow_episode_starts(&mut self.overflow_episodes.lights, lights.len(), count)"),
        "upload_lights must guard its overflow warning on the unclamped length against MAX_LIGHTS \
         (PERF-D4-NEW-02 / #1808), through the per-episode latch (#7385)"
    );
    // The warn must describe the real drop policy. collect_lights sorts
    // the point-light suffix by descending gi_priority_score (#1800)
    // before this clamp, so the dropped tail is the lowest-priority one —
    // the old "storage-iteration order / no proximity priority" wording
    // described the pre-sort code and sent operators reading ECS
    // iteration order for a cause that no longer exists (W2.11, light &
    // shadow campaign).
    assert!(
        lights_fn_body.contains("lowest gi_priority_score"),
        "the overflow warn must state that the dropped tail is the lowest \
         gi_priority_score after collect_lights' priority sort"
    );
    assert!(
        !lights_fn_body.contains("storage-iteration order"),
        "the stale storage-order wording is back — the truncation is \
         priority-ordered, not storage-ordered"
    );
    let warn_idx = lights_fn_body
        .find("log::warn!")
        .expect("upload_lights must emit a log::warn! on overflow (PERF-D4-NEW-02 / #1808)");
    let guard_idx = lights_fn_body.find("overflow_episode_starts(&mut self.overflow_episodes.lights").unwrap();
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

/// #7385 — every SSBO upload that runs each frame and clamps logs its overflow once per
/// episode. A scene held over a cap (a heavy modded interior, many combustion field lights)
/// logged one line a frame from each of these, ~60 lines/s, while `rt.integrity` already
/// reported the live counts. The terrain-tile upload is dirty-gated, not per frame, so it
/// keeps its plain warn.
#[test]
fn every_per_frame_clamped_upload_warns_once_per_overflow_episode() {
    // A whole-file scan, like the guard above: `upload.rs` keeps a test module near its top, so
    // `production_text` (which cuts at the first `#[cfg(test)]`) would drop every upload function.
    // The needles are production spellings (`&mut self.<latch>`) the test module does not contain.
    let src = include_str!("upload.rs");
    for (function, latch, message) in [
        ("pub fn upload_lights(", "overflow_episodes.lights", "Light SSBO overflow"),
        ("pub fn upload_instances(", "overflow_episodes.instances", "Instance SSBO overflow"),
        ("pub fn upload_previous_models(", "overflow_episodes.previous_models", "Previous-model SSBO overflow"),
        ("pub fn upload_indirect_draws(", "overflow_episodes.indirect_draws", "Indirect draw overflow"),
    ] {
        let start = src.find(function).unwrap_or_else(|| panic!("{function} must exist"));
        let body = &src[start..];
        let body = &body[..body[1..].find("\n    pub fn ").map_or(body.len(), |at| at + 1)];
        let warn = body.find(message).unwrap_or_else(|| panic!("{function} keeps its `{message}` warn"));
        let guard = body
            .find(&format!("overflow_episode_starts(&mut self.{latch}"))
            .unwrap_or_else(|| panic!("{function} must gate its warn on the `{latch}` episode latch (#7385)"));
        assert!(guard < warn, "{function}: the latch must guard the warn (#7385)");
    }
}
