# AR-09: No NiControllerManager sequence state machine equivalent

## Finding: AR-09 (LOW)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: Animation Readiness
**Games Affected**: All games with KFM-driven animation state machines

## Description

Gamebryo's NiControllerManager acts as a sequence state machine: it manages activation, deactivation, and transitions between NiControllerSequences with cross-fade timing and sync groups.

Redux has AnimationStack (manual layer blending) and kfm.rs (KFM parser for transition metadata), but there is no runtime state machine that consumes KFM transition data to automatically drive AnimationStack layer changes. Currently, animation transitions must be managed manually by application code.

## Impact

Low immediate impact since game-specific animation state machines are typically driven by gameplay logic anyway. The KFM parser provides catalog data; AnimationStack provides the blend mechanism. The missing piece is the connecting glue — an enhancement rather than a correctness issue.

## Suggested Fix

Implement an `AnimationController` component or system that consumes KfmFile transition tables and drives AnimationStack.play() calls with specified blend times.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
