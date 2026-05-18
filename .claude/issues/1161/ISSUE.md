# #1161 — REN-D9-NEW-08: Frame-counter u32→f32 cast loses 1-frame precision after ~16.7M frames (~3.2 days)

**Severity**: LOW (INFO in the audit; mapped to `low` on GitHub label)
**Domain**: renderer
**Status**: OPEN
**Source Audit**: `docs/audits/AUDIT_RENDERER_2026-05-17_DIM9_DIM10.md` — Dimension 9

## Location

- `crates/renderer/src/vulkan/context/draw.rs:465` (assembly site)
- `crates/renderer/shaders/triangle.frag:342` (`interleavedGradientNoise`)

## Description

`cameraPos.w` is assembled as `self.frame_counter as f32` where `frame_counter: u32`:

```rust
position: [
    camera_pos[0],
    camera_pos[1],
    camera_pos[2],
    self.frame_counter as f32,
],
```

Past `2^24 = 16_777_216` frames (~3.2 days at 60 FPS, ~6.5 days at 30 FPS) the IEEE-754 single-precision rounding step exceeds 1.0, so consecutive frames map to the same `cameraPos.w` value. Every IGN call seeds noise from this float, so per-pixel temporal patterns freeze. The reservoir streaming (line 2369), shadow jitter (2407-2408), glass refraction roughness spread (1703-1706), metal reflection cone sample (2102-2103), and GI hemisphere sample (2513-2514) all degrade in lockstep — the soft penumbras and TAA-friendly accumulated smoothing stop refreshing.

The GI path at line 2512 uses `floor(frameCount * 0.25)` so its effective freeze threshold is ~67M frames (~12.8 days at 60 FPS), but shadow / reflection / refraction sites use `frameCount` directly.

## Suggested Fix

Either:

**(a) Wrap at upload**:

```rust
position: [
    camera_pos[0],
    camera_pos[1],
    camera_pos[2],
    (self.frame_counter & 0xFFFFFF) as f32,
],
```

Wraps the noise seed cleanly at the precision boundary, preserves the 2^24 distinct values that f32 can represent without precision loss.

**(b) Accept + document**:

Add a doc-comment alongside the existing `frame_counter` declaration at `context/mod.rs:724` calling out the 3-day limit explicitly so future audits don't refile.

Option (a) is preferred — minimal change, no behavioural surprise for long sessions.

## Related

- REN-D7-NEW-07 (already-fixed) — resize resets `frame_counter = 0`, so cell transitions during a long session may unintentionally reset the precision drift. Different concern.
