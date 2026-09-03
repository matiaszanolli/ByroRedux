# #3685 — PERF-D5-2026-08-30-05: the volumetrics gate-off arm re-clears the whole integrated froxel volume every frame, with no already-cleared latch

**Severity**: LOW · **Dimension**: GPU Pipeline
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs::record_volumetrics_pass`

## Fix

`record_volumetrics_pass` had two call sites that unconditionally issued a
full `record_neutral_frame` clear over the integrated froxel volume every
frame: the `!vol.requires_dispatch(...)` gate-off arm (no medium, no fog
volumes, no lingering combustion — the common case in fog-free cells) and
the TLAS/cluster/geometry-not-ready fallback arm. The image is already
neutral after the first such frame in a streak; nothing writes it again
until a real dispatch runs.

Per the issue's own suggested fix, reused the exact in-repo precedent:
`record_caustic_splat_pass`'s per-FIF skip-clear latch (`#2507`). Its pure
decision function had no caustic-specific logic in it at all, so lifted it
to a shared name, `skip_clear_decision` (was `caustic_skip_clear_decision`),
generalizing its doc comment rather than duplicating the state machine —
matching the issue's own "lift into a shared pure helper" option.

Restructured `record_volumetrics_pass`'s inner `if let Some(ref mut vol) =
self.volumetrics` block so both former unconditional clear sites now
report a `ran: bool` (true only when a real dispatch executed and
succeeded) instead of calling `record_neutral_frame` directly, then apply
the shared latch decision once at the end of the block:

```rust
let (should_clear, next_latch) =
    skip_clear_decision(ran, self.volumetrics_cleared_on_skip[frame]);
self.volumetrics_cleared_on_skip[frame] = next_latch;
if should_clear {
    vol.record_neutral_frame(&self.device, cmd, frame);
}
```

A failed dispatch (`Err` from `vol.dispatch`) reports `ran = true` rather
than `false` — this is a deliberate choice, not an oversight: the pre-fix
code never cleared on a dispatch failure either (composite just falls back
to stale prior-frame content on that exact frame, an existing, unrelated
behavior this fix doesn't touch), and reporting it as "ran" resets the
latch so the *next* genuine skip streak still clears its first frame
rather than trusting a latch left over from before the failed attempt.

Added the sibling `volumetrics_cleared_on_skip: [bool; MAX_FRAMES_IN_FLIGHT]`
field next to `caustic_cleared_on_skip` on `VulkanContext`, initialized in
`init.rs` and reset on resize in `resize.rs` (same place the caustic latch
resets), per the issue's own suggested fix.

## SIBLING (issue's own checklist item)

The issue's own "Related" section named `caustic_skip_clear_decision` /
`caustic_cleared_on_skip` (#2507) as the in-repo precedent and template —
addressed above by sharing the function outright rather than duplicating
it. No other pass in this file has the same "dispatch conditionally
skipped, but downstream sampling is unconditional every frame" shape (SVGF
and TAA's own failure latches degrade to "keep sampling stale content"
instead of clearing, per the existing doc comment on `svgf_failed`)."

## TESTS (issue's own checklist item)

The three existing `skip_clear_decision` unit tests (renamed from
`caustic_skip_clear_decision`, unmodified otherwise) already exercise the
shared state machine both callers now depend on.

`record_volumetrics_pass` needs a live `VulkanContext` (no fixture exists
in this crate) — matching this session's established convention for that
situation (e.g. #3690's `retention_hoisting_tests`), added a static
source-scan test,
`record_volumetrics_pass_routes_skip_clears_through_the_shared_latch`,
scoped to the file's production portion (before its own `#[cfg(test)]`
module), pinning:
- the function body calls
  `skip_clear_decision(ran, self.volumetrics_cleared_on_skip[frame])`;
- `vol.record_neutral_frame(` appears exactly once in the body (the single
  latch-gated call site, not the two former unconditional ones).

**Reintroduce-and-revert verification**: temporarily reintroduced an
unconditional `vol.record_neutral_frame(...)` call alongside the
`requires_dispatch` check (simulating the old always-clear behavior) —
confirmed the new test failed (`left: 2, right: 1`, the exact count
mismatch). Restored the fix and reran — all 4 tests in
`context::post_passes::tests` pass again.

## Verification

- `cargo check -p byroredux-renderer --tests`: clean.
- `cargo test -p byroredux-renderer --lib context::post_passes::`: 4 tests
  passing, 0 failing (+1 new).
- `cargo test -q -p byroredux-renderer`: 823 tests passing (+1), 0
  failing.
- `cargo check -p byroredux --tests`: clean (downstream crate, confirms
  the new `VulkanContext` field doesn't break construction elsewhere).
- `cargo test -q --no-fail-fast` (full workspace): **7108 passing, 0
  failing**.
