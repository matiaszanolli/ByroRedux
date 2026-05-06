# Investigation — #828

**Domain**: ecs / animation (binary crate `byroredux/src/systems.rs`)

## Two sites, same bug

### Site 1 — AnimationStack path (L567-568, primary)

`events` and `seen_labels` declared inside the `for entity in stack_entities` loop. Every stack entity per frame pays two `Vec::new()` calls plus growth-doubling reallocs.

### Site 2 — AnimationPlayer path (L388, sibling per checklist)

`events` is already declared outside the inner `for ps in &playback_states` loop and `clear()`'d each iteration — *but* the `mem::take` at L400 destroys the buffer's capacity, so the next iteration starts at zero-cap and re-doubles. Same allocation churn pattern, just one level up.

## Fix

For both sites:

1. Hoist scratch Vecs to outer scope (Site 1 only — Site 2 already has them at the right level).
2. `clear()` at top of each iteration (Site 2 already does this; Site 1 needs to be added).
3. Replace `mem::take(&mut events)` with `events.clone()` at the storage-insert sites. `AnimationTextKeyEvent` is `#[derive(Copy)]` (verified at `crates/scripting/src/events.rs:62`), so the clone is a memcpy of N × 8 bytes (FixedString + f32 = 8 bytes per entry). The scratch Vec keeps its high-water-mark capacity across iterations.

`visit_stack_text_events` (`crates/core/src/animation/stack.rs:206`) already clears `seen` internally — no change needed there. `visit_text_key_events` doesn't take a seen-set.

## Why clone instead of mem::take

`mem::take(v)` swaps in `Vec::default()` (zero capacity). Next iteration's visitor walks again and the Vec doubles from 0. With `events.clone()`, the storage gets a sized-exactly Vec and the scratch keeps its capacity. Total allocator traffic is similar but the scratch settles at the high-water mark instead of churning.

## Test strategy

Audit suggested `dhat` allocation-counter tests — heavy infra. The behavior is identical (events still emit correctly), so existing animation_system tests cover correctness. Pin the steady-state behavior: drive the system twice with the same active stack, confirm the second emit produces the same events and that `mem::take` removal doesn't break the existing flow. The capacity-retention property would need allocation-counter infra to verify directly; skip for now and rely on review.

## Scope

1 file: `byroredux/src/systems.rs`
