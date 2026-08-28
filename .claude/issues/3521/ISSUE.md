# Issue #3521 — AUD-2026-08-27-D1-01

Source: `docs/audits/AUDIT_AUDIO_2026-08-27.md` · https://github.com/matiaszanolli/ByroRedux/issues/3521

Filed from `docs/audits/AUDIT_AUDIO_2026-08-27.md` (finding `AUD-2026-08-27-D1-01`).

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Lifecycle & Leaks
- **Location**: `crates/audio/src/lib.rs:979` (`drain_pending_oneshots`)
- **Related**: #852 (the `VecDeque` choice), #932 / #3059 / #3257 (the same scratch-capacity class, all filed and fixed)

## Description

The drain replaces the live `VecDeque` with a fresh default and consumes the old one by value:

```rust
let pending = std::mem::take(&mut audio_world.pending_oneshots);
```

`VecDeque::default()` allocates nothing, and `pending` is moved through `for p in pending` and dropped at end of scope — so the queue's heap buffer is **freed every tick that had any queued one-shot**, and the next `play_oneshot` re-allocates from zero. The `VecDeque` was deliberately chosen over `Vec` in #852 to make the cap-eviction path O(1); this undoes the adjacent half of that intent by making the steady-state cost an allocate+free pair per drain.

This is the exact class the project has already filed and fixed three times elsewhere: `FootstepScratch` (#932 — "pre-#932 a fresh `Vec<Vec3>` was allocated every frame"), `InteractionCandidateScratch` (#3059), and the `submersion_system` disturbance scratch (#3257, landed in `bbfd742f`). `footstep_system` itself carries the canonical remedy in-line — it `std::mem::take`s the scratch buffer and then **restores it on both the success path and the `AudioWorld`-absent bail path** (`byroredux/src/systems/audio.rs:174-176`, `184-190`, `205-208`) precisely so the capacity is not stranded. `drain_pending_oneshots` has no such restore.

## Evidence

`crates/audio/src/lib.rs:977-993` — after the loop over `pending` ends there is no write back into `audio_world.pending_oneshots`; the next producer call reaches `self.pending_oneshots.push_back(..)` (`crates/audio/src/lib.rs:557-563`) on a zero-capacity deque. The `mem::take` is not gratuitous — `audio_world.manager.as_mut()` is held mutably across the loop, so a `drain(..)` over a sibling field would not borrow-check — but a reusable second `VecDeque` field swapped back at the end would.

## Impact

One `alloc`/`free` pair per tick that dispatches any queued one-shot, on the `Stage::Late` per-frame path. Not a leak, not a correctness bug, and negligible next to the kira dispatch it wraps. It compounds with `AUD-2026-08-27-D7-01` (which makes *every* frame a drain frame rather than ~2/s), so fixing that one raises this from "every frame" to "every stride" and lowers its priority accordingly — fix D7-01 first.

## Suggested Fix

Add a `drain_scratch: VecDeque<PendingOneShot>` field to `AudioWorld` (declared adjacent to `pending_oneshots`, before `music`, so the drop order is unchanged), `std::mem::swap` it with `pending_oneshots` at the top of the drain, and swap the (now empty, capacity-retaining) buffer back at the end. Same shape as #3257's fix.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other per-tick `mem::take` scratch buffers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
