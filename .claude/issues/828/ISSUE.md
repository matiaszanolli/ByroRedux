ECS-PERF-06: animation_system allocates fresh events/seen_labels Vec per AnimationStack entity per frame|## Description

The transform/blend code path explicitly hoisted `channel_names_scratch` and `updates_scratch` out of the per-entity loop (per-comments referencing #251/#252) — but the text-event scratches `events: Vec<AnimationTextKeyEvent>` and `seen_labels: Vec<FixedString>` were left as fresh allocations inside the loop. Every AnimationStack entity therefore pays two `Vec::new()` calls per frame, plus growth-doubling reallocs as labels accumulate.

## Location

`byroredux/src/systems.rs:567-568` (inside the `for entity in stack_entities` loop in `animation_system`)

## Evidence

```rust
for entity in stack_entities {                          // per-frame outer loop
    // ... advance, cache prep ...
    let mut events: Vec<AnimationTextKeyEvent> = Vec::new();   // ← fresh alloc per entity
    let mut seen_labels: Vec<FixedString> = Vec::new();        // ← fresh alloc per entity
    let accum_root: Option<FixedString>;
    // ...
    visit_stack_text_events(stack, &registry, &mut seen_labels, |time, sym| {
        events.push(AnimationTextKeyEvent { label: sym, time });
    });
    // ... mem::take(&mut events) at line 659 hands ownership away,
    //     so the next iteration would need a fresh Vec anyway ──
    //     but only when `events.is_empty()` was false. The empty
    //     case discards the allocation needlessly.
}
```

## Impact

Today AnimationStack entities are rare (count = number of NPCs with multi-layer animation; Megaton has ~0). M41 and beyond: ~10–50 NPCs per cell with stacks. At 50 stack entities × 2 Vec allocations × 60 fps = 6 000 small allocations/sec. Negligible CPU but adds allocator churn.

## Suggested Fix

Hoist both Vecs to the outer scope alongside `channel_names_scratch` and `updates_scratch`; `clear()` at the top of each iteration.

For the `events` vec, replace the `mem::take` insert pattern at line 659 with `eq.insert(entity, AnimationTextKeyEvents(events.drain(..).collect()))` so the buffer's capacity stays with the scratch — or change `AnimationTextKeyEvents` to accept a `&mut Vec` and drain into its own owned Vec at the storage boundary.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check the `AnimationPlayer` text-event path at `systems.rs:388` — same `Vec::new()` per outer-loop iteration pattern, also needs hoisting
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: No lock change — Vecs are local
- [ ] **FFI**: N/A
- [ ] **TESTS**: Allocation-counter test (e.g. `dhat`) over 100 frames with a populated AnimationStack scene

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-06