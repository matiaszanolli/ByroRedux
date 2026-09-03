# #3251 — ECS-2026-08-24-03: AnimatedTextureFlip::handle_for_slot silently aliases an out-of-range index to bindless handle 0

**Severity**: LOW · **Dimension**: ECS
**Location**: `crates/core/src/ecs/components/animated.rs::AnimatedTextureFlip::handle_for_slot`

## Fix

Applied the issue's own suggested fix exactly:
`.map(|e| e.handles.get(e.current_index).copied().unwrap_or(0))` →
`.and_then(|e| e.handles.get(e.current_index).copied())`. An
out-of-range `current_index` now correctly reads as `None` ("no handle"),
triggering the caller's existing static-texture fallback, instead of
`Some(0)` (bindless slot 0 — some other entity's texture).

Not reachable today, as the issue itself notes — confirmed the attach
path (`anim_convert.rs`) keeps `handles.len()` and the valid
`current_index` range in lockstep for the channel a `TextureFlipEntry`
was built from — but this closes the gap for a future clip-swap path
that rebinds a differently-sized channel onto an already-attached entry
matched only by `texture_slot`.

## TESTS (issue's own checklist item)

Added 4 tests: the happy path (in-range index), an absent slot, the
out-of-range case the issue names explicitly (asserting `None`, not
`Some(0)`), and the degenerate empty-`handles` edge case.

**Reintroduce-and-revert verification**: temporarily restored the old
`.map(...).unwrap_or(0)` body — confirmed both the out-of-range test and
the empty-handles test failed with `Some(0)` where `None` was expected.
Restored the fix and reran — all 4 tests pass again.

## Incidental fix: stale save-shape-fingerprint baseline

Fixing `handle_for_slot`'s body moved
`save_io::serde_default_guard_tests::saved_type_shape_changes_require_
format_major_bump`'s fingerprint, failing the full-workspace gate. Root
cause (not a real save-format break): `AnimatedTextureFlip` is a **tuple**
struct with no `{` of its own, so the guard's brace-matching (which looks
for the struct's `{` to find where its "shape" ends) walks forward to the
next `{` in the file — the following `impl` block's — sweeping
`handle_for_slot`'s method body text into the captured span. This is a
known quirk that guard's own `#3332` comment already documents ("this
guard is file-scoped, not type-scoped"). Confirmed `AnimatedTextureFlip`
is on `registry_completeness_tests.rs`'s `NOT_SAVED_BY_DESIGN` allowlist
("per-frame output re-derived every tick... re-resolved by
`attach_animation_sinks` on load") — no saved data shape actually
changed, so `FORMAT_MAJOR` does not need a bump. Updated
`BASELINE_SHAPE_FINGERPRINT` to the new measured value
(`0xdb24_cca4_a32b_5bde`) with a dated comment explaining why, matching
this file's own established convention for exactly this situation
(`#3332`, `#3460`, `#3489` precedent).

## Verification

- `cargo check -p byroredux-core --tests`: clean, zero warnings.
- `cargo check --workspace --tests`: clean (one pre-existing, unrelated
  `unused_mut` warning in `grup_walker.rs:469` predates this fix).
- `cargo test -p byroredux-core --lib ecs::components::animated::`: 4
  tests passing, 0 failing (all new).
- `cargo test -p byroredux-core --lib`: 732 tests passing (+4), 0
  failing.
- `cargo test -p byroredux save_io::serde_default_guard_tests::`: 5
  tests passing, 0 failing (baseline updated).
- `cargo test -q --no-fail-fast` (full workspace): **7146 passing, 0
  failing**.
