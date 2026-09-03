# #3687 — PERF-D6-2026-08-30-02: `update_morph_weights` heap-allocates a fresh `Vec<f32>` per morph slot per frame and unconditionally marks the slot dirty

**Severity**: MEDIUM · **Dimension**: Update/Sync Cost
**Location**: `byroredux/src/render/skinned.rs::update_morph_weights`,
`crates/renderer/src/vulkan/morph_compute.rs::MorphSlot::stage_weights`

## Fix

`MorphSlot` already owns a right-sized `pending_weights: Vec<f32>` staging
buffer, but `update_morph_weights` was building a brand-new `Vec<f32>` via
`.collect()` every frame for every morph-target entity, then handing it to
`MorphSlot::stage_weights`, which unconditionally copied it in and always
marked `pending_weights_dirty = true` — discarding both the buffer reuse
and any chance to skip the GPU upload when weights hadn't actually moved.

Changed `stage_weights`'s signature from taking an owned `Vec<f32>` to a
closure (`impl FnMut(usize) -> f32`), writing directly into the slot's own
`pending_weights` buffer in place and comparing each value before writing
it, so the dirty flag only flips when a weight genuinely changed:

```rust
// crates/renderer/src/vulkan/morph_compute.rs
pub fn stage_weights(&mut self, weight_at: impl FnMut(usize) -> f32) {
    self.pending_weights_dirty |= stage_weights_into(&mut self.pending_weights, weight_at);
}

fn stage_weights_into(pending: &mut [f32], mut weight_at: impl FnMut(usize) -> f32) -> bool {
    let mut changed = false;
    for (i, slot) in pending.iter_mut().enumerate() {
        let new = weight_at(i);
        if *slot != new {
            *slot = new;
            changed = true;
        }
    }
    changed
}
```

`update_morph_weights` (`byroredux/src/render/skinned.rs`) now passes a
closure straight over `weights.get(i)` instead of collecting an
intermediate `Vec`:

```rust
// #3687 — writes directly into `slot`'s own pending-weights
// buffer via the closure, no per-frame `Vec` allocation.
slot.stage_weights(|i| weights.get(i));
```

The logic that needs testing (`stage_weights_into`) is a free function
taking only primitives/owned slices — pulled out of the `MorphSlot`
method specifically so it's unit-testable without a live Vulkan device
(`MorphSlot::create` needs a `GpuUploadCtx`), matching this crate's
established free-function-extraction pattern for that situation.

## SIBLING (issue's own checklist item)

`grep -rn 'stage_weights(' --include='*.rs' .` — the only production call
site is `skinned.rs`'s `update_morph_weights`, now updated. No other
callers exist.

## TESTS (issue's own checklist item)

Added 4 tests to `crates/renderer/src/vulkan/morph_compute.rs`'s existing
`#[cfg(test)] mod tests`:
- `writes_every_index_in_order`
- `identical_values_report_no_change`
- `one_differing_value_reports_a_change_and_only_that_slot_moves`
- `does_not_allocate_a_new_buffer` — asserts `pending.as_ptr()` and
  `.capacity()` are unchanged after staging (pins the "write in place,
  don't reallocate" half of the fix, not just the dirty-flag half).

**Reintroduce-and-revert verification**: temporarily replaced
`stage_weights_into`'s body with the old unconditional-write-always-dirty
logic (`*slot = weight_at(i); ... ; true` for every index) — confirmed
`identical_values_report_no_change` failed with the expected panic
("re-staging byte-identical weights must not report a change..."), while
the other 3 tests still passed (they don't probe the no-op case). Restored
the fix and reran — all 5 tests (4 new + the pre-existing `#3244`
fence-ordering test) pass again cleanly.

## Verification

- `cargo check -p byroredux-renderer --tests`: clean.
- `cargo test -p byroredux-renderer --lib morph_compute::tests`: 5
  passing, 0 failing (+4 new).
- `cargo check -p byroredux-renderer -p byroredux --tests`: clean.
- `cargo test -q -p byroredux-renderer -p byroredux`: 822 tests passing
  (renderer crate, +4), 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7102 passing, 0
  failing**.
