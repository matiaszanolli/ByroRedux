# #2659: SCR-D6-NEW11-02: DeferredFragmentEffects::new deep-clones the whole QuestDefinitionRegistry every frame, before the early-bail

**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/fragment.rs:335-341` (`DeferredFragmentEffects::new`), consumed by `quest_fragment_dispatch_system`; early-bail at `:1372-1374`
**Status**: NEW (introduced by the #2539 fix, `6ad64ef6`)

## Description

The #2539 fix correctly snapshot-clones `QuestDefinitionRegistry` before taking the `(QuestStageState, QuestObjectiveState)` write guards, which is what eliminated the nested acquisition that issue was about.

But the clone happens unconditionally in `new()`, **before** the `queue.is_empty() || frags.is_empty()` bail at `:1372-1374`. So a frame with no quest activity at all still deep-copies the entire quest-definition registry.

## Evidence

Measured on real `Skyrim.esm` (1811 QUST records): **0.651 ms/frame in release**.

On a synthetic 5,000-quest load order (a heavily-modded install): **15.6 ms/frame** -- i.e. the entire frame budget.

The registry's only writers take `&mut World` and run at load time (`crates/scripting/src/fragment.rs:339` reads it via `world.try_resource`), so the value is immutable for the whole frame it is being copied for.

## Impact

A flat per-frame cost proportional to load-order quest count, paid whether or not any fragment dispatches. At vanilla scale it is a real but survivable ~4% of a 16.6 ms budget; at modded scale it is frame-rate determining.

Note the project's standing invariant that a CPU bottleneck is a bug (dev hardware is a Ryzen 7950X / RTX 4070 Ti -- the CPU should never be the limiter).

## Related

#2539 (closed -- this is a cost its fix introduced, not a defect in what it fixed), #2269, SCR-D6-NEW11-03

## Suggested Fix

Move the bail ahead of the clone -- construct `DeferredFragmentEffects` lazily only once there is work to do -- or replace the deep clone with an `Arc` snapshot the registry swaps on mutation. Since its writers are load-time-only (`&mut World`), an `Arc<QuestDefinitionRegistry>` read is sound and free.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
