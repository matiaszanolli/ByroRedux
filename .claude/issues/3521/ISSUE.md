# #3521 — AUD-2026-08-27-D1-01: drain_pending_oneshots' std::mem::take strands the pending_oneshots heap capacity on every drain tick

**Severity**: LOW · **Dimension**: Spatial Sub-Track Lifecycle & Leaks
**Location**: `crates/audio/src/lib.rs::drain_pending_oneshots`

## Fix

Verified the premise: `std::mem::take(&mut audio_world.pending_oneshots)`
replaced the live `VecDeque` with a fresh zero-capacity default and moved
the old (capacity-holding) one into `pending`, which was then consumed
by-value in the loop and dropped at end of scope — an allocate+free pair
on the next `play_oneshot` push, every tick that drained anything.

The issue suggested a dedicated `drain_scratch` field + `mem::swap`
(matching #3257's shape), reasoning that `mgr` being borrowed mutably
across the loop would make draining `pending_oneshots` in place fail to
borrow-check. Tried that premise directly before implementing the
suggested fix: `audio_world.pending_oneshots.drain(..)` **does**
borrow-check today — `mgr` (borrowed from `audio_world.manager`) and the
loop body's other field accesses (`reverb_send`, `active_sounds`,
`underwater`) are all disjoint fields from `pending_oneshots`, and Rust's
field-sensitive borrow checking accepts draining it in place alongside
them. So the simpler fix landed instead: replaced the
take-then-consume-by-value shape with `VecDeque::drain(..)` in place,
which empties the queue while retaining its allocated capacity — no new
field, no swap dance, no drop-order reasoning needed.

Updated the adjacent `#851` comment (which referenced the old mechanism
by name) to describe the current one, and added a note documenting why
the new explanatory comment never spells out the old approach's own
method-path text — see TESTS below.

## SIBLING (issue's own checklist item)

The issue names three prior fixes of the identical class
(`FootstepScratch` #932, `InteractionCandidateScratch` #3059, the
`submersion_system` disturbance scratch #3257) as precedent, confirming
this codebase treats each site as its own ticket rather than a bulk sweep.
A fresh grep for the same `mem::take(&mut *.<pending|queue|scratch>)`
shape turns up further candidates (`crates/scripting/src/fragment.rs`,
`crates/scripting/src/papyrus_provider.rs`,
`crates/renderer/src/texture_registry.rs`,
`crates/renderer/src/vulkan/egui_pass.rs`,
`crates/renderer/src/vulkan/context/skinned_blas_refit.rs`,
`byroredux/src/extensions.rs`, `crates/mod-runtime/src/runtime.rs`) — each
needs its own per-site judgement of hot-path relevance (some are
per-command dispatch queues, not per-frame scratch buffers) before a fix
is warranted, so left as future individual findings rather than folded
into this LOW-severity single-site ticket.

## TESTS (issue's own checklist item)

- `drain_pending_oneshots_drains_in_place_instead_of_stranding_capacity`
  — a source-level pin (the loop body is unreachable in a headless test:
  it needs both a live `listener` and a live `manager`, neither available
  without an audio device — same gate `audio_system_no_op_when_
  audio_world_inactive` already pins, matching the established
  device-only-path convention `oneshot_marker_is_consumed_on_both_
  dispatch_failure_arms` already uses in this same file). Asserts the
  function body contains `.pending_oneshots.drain(` and does not contain
  the old take-based method-path text.
  - Hit a self-matching trap while writing it: my own explanatory
    comment in `lib.rs` initially spelled out the old method's literal
    path, which the source scan then matched against its own describing
    prose instead of real code, giving a false failure. Reworded the
    comment to describe the old approach without the literal substring
    (the file now says so explicitly, matching the convention
    `crates/core/src/ecs/components/material.rs`'s own structural guards
    already use for this exact hazard) and re-verified.
- `vecdeque_full_range_drain_preserves_capacity` — the runtime property
  the fix depends on, verified directly and independent of any audio
  device: draining a `VecDeque`'s full range in place does not release
  its buffer.

**Reintroduce-and-revert verification**: temporarily restored the old
take-then-consume-by-value shape — confirmed
`drain_pending_oneshots_drains_in_place_instead_of_stranding_capacity`
failed with the expected message. Restored the fix and reran — all 32
tests in `byroredux-audio`'s `tests` module pass again.

## Verification

- `cargo check -p byroredux-audio --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-audio`: 32 passing, 0 failing (+2 new).
- `cargo test -q --no-fail-fast` (full workspace): **7162 passing, 0
  failing**.
