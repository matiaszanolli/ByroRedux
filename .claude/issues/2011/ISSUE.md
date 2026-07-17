# 2011: ECS-2026-07-16-01: GuardState doc comment overstates its write frequency

https://github.com/matiaszanolli/ByroRedux/issues/2011

Labels: low, ecs, documentation

**Severity**: LOW · **Dimension**: 7 (Component Lifecycles)
**Location**: `crates/core/src/ecs/components/guard.rs:55-62`
**Status**: NEW
**Audit**: docs/audits/AUDIT_ECS_2026-07-16.md (ECS-2026-07-16-01)

## Description
The doc comment on `GuardState` says guarding "reads *and* written every tick, the same shape `WanderState` has." This is inaccurate. `guard_system` only ever sets `GuardState` on first sight (`existing_state == None`); once resolved, every later tick takes the `Some(s) => (s.anchor, None)` branch and Pass 2's write loop is a no-op. `WanderState`, by contrast, genuinely mutates every tick. The doc block's own preceding sentence ("anchor is resolved or picked exactly once ... frozen — mirrors `TravelState::destination`") contradicts the later sentence.

## Evidence
```rust
// crates/core/src/ecs/components/guard.rs:55-62
/// ... anchor is resolved or picked exactly once ... and then frozen —
/// mirrors TravelState::destination. Unlike Travel, there is no
/// terminal marker: guarding continues indefinitely, so this state is
/// read *and* written every tick, the same shape WanderState has.
```
```rust
// byroredux/src/systems/guard.rs:120-126 — write only on first sight
let (anchor, state) = match existing_state {
    Some(s) => (s.anchor, None),
    None => {
        let anchor = resolve_anchor(world, behavior, current);
        (anchor, Some(GuardState { anchor }))
    }
};
```

## Impact
Documentation-only; `guard_system` is the sole consumer of `GuardState`, and its tests correctly exercise the frozen-anchor behavior. Risk is future maintainers reading only the component doc and assuming per-tick write handling is needed (e.g. save/load snapshot path, change-detection granularity).

## Related
None (new code, no prior issue).

## Suggested Fix
Reword the last sentence to match actual discipline: "guard_system reads this state every tick for the leash check, but anchor itself is written only once, the same frozen-after-resolution discipline TravelState::destination has, not WanderState's continuous mutation."

## Completeness Checks
- [ ] TESTS: N/A (documentation-only fix; existing `guard_system_holds_position_once_within_leash_and_returns_when_displaced` already pins actual runtime behavior)
