# Investigation: Issue #48

## Root Cause
update_rgba (texture_registry.rs:215) calls device_wait_idle() before
destroying the old texture. Called every frame when Scaleform UI is
active, causing full GPU pipeline stall.

## Call Order
RedrawRequested handler:
1. update_rgba (line 1434) — BEFORE draw_frame
2. draw_frame (line 1455) — waits on in-flight fence at top

## Why device_wait_idle Exists
The old texture might be referenced by an in-flight command buffer.
With MAX_FRAMES_IN_FLIGHT=2, frame N-1 might still be using it.

## Fix: Deferred Destruction
Instead of immediate destroy + device_wait_idle:
1. Store the previous texture in a `pending_destroy` field on TextureEntry
2. On next update: destroy the pending texture (by then both frames
   have completed), then move current to pending, create new current
3. No device_wait_idle needed — natural frame pacing provides the gap

This works because update_rgba is called every frame, so the pending
texture always has ≥2 frame's worth of time before being destroyed.

## Scope
1 file: texture_registry.rs (add pending_destroy field, deferred logic).
