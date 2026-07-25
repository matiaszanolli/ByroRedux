# 2171: PERF-D9-NEW-02: Origin-crossing diagnostic trace logs render_origin_delta after the state it measures was already overwritten

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2171
**Labels**: bug, medium, performance

---

## Severity
MEDIUM

## Dimension
Telemetry (Dim 9) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/draw.rs:1165-1187`

## Description
The trace added for the open ghosting investigation computes `render_origin.{x,y,z} - self.prev_render_origin[..]`, but `self.prev_render_origin` was assigned the *current* origin 16 lines earlier. The subtraction is therefore always exactly zero.

## Evidence
```rust
self.prev_view_proj = *vp;                                                    // :1165
self.prev_camera_position = camera_pos;                                       // :1166
self.prev_render_origin = [render_origin.x, render_origin.y, render_origin.z]; // :1167
// ...
log::trace!("... render_origin_delta=({:.3},{:.3},{:.3}) ...",
    render_origin.x - self.prev_render_origin[0],   // :1183 — always 0.0
    render_origin.y - self.prev_render_origin[1],
    render_origin.z - self.prev_render_origin[2], ...);
```

## Impact
The single diagnostic added specifically to identify origin-crossing frames in a live repro reports a constant zero, actively arguing "no crossing happened" on precisely the frames under investigation — including the frames the sibling PERF-D9-NEW-01 issue corrupts. (`vp_max_abs_delta` on the same log line *is* correct and would have shown the ~5562 spike from that issue's evidence table — it was available but the origin-delta figure printed alongside it was actively misleading.) Cost is nil (trace level), but the diagnostic value is negative.

## Related
PERF-D9-NEW-01 (filed separately); memory note "Renderer Ghosting Investigation Open".

## Suggested Fix
Capture `let origin_delta = render_origin - Vec3::from_array(self.prev_render_origin);` **before** the overwrite, and log that local.

## Completeness Checks
- [ ] **TESTS**: Not strictly needed (trace-level diagnostic), but worth a quick manual verification the delta shows non-zero across a real cell crossing
