# #3886: TD8-2026-09-05-03: `crates/core/src/animation/controller.rs` (454 LOC) is a fully dead subsystem — nothing constructs `AnimationController` outside its own tests, and no system reads it

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-03) via `/audit-publish`, 2026-09-05. Labels: `low,animation,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3886 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD8-2026-09-05-03), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/core/src/animation/controller.rs` (whole file: `AnimationController`, `ControllerTransition`, `ControllerTransitionDefaults`, `TransitionKind`, `apply_pending_transition`, `add_sequence`, `add_transition`, `request_sequence`, `set_sync_group`, `from_kfm_discriminant`), re-exported from `crates/core/src/animation/mod.rs`
- **Status**: NEW
- **Effort**: small (≤2 h) — the work is a keep-or-delete decision plus the mechanical removal
- **Age**: `07dc6b16`, 2026-04-23 ("Fix #338: add AnimationController — KFM-driven sequence state machine")

**Description**
The module presents itself as the glue that closes legacy-audit gap AR-09 / #338 — "the KFM parser provides catalog data, `AnimationStack` provides the blend mechanism, and this module connects them". Neither end was ever connected. `AnimationController` is a `Component`, but **no spawn path attaches it to any entity**, no system in `byroredux/src/systems/animation.rs` (or anywhere) queries it, and `apply_pending_transition` — its only entry point, publicly re-exported from `crates/core/src/animation/mod.rs` — has zero callers.

It is not a `#[allow(dead_code)]` case: the compiler cannot see it because every item is `pub` in a library crate.

**Evidence**
```
$ grep -RIn "AnimationController::new\|AnimationController {" --include="*.rs" crates byroredux tools
  crates/core/src/animation/controller.rs:37   # a ```text doc snippet (deliberately non-compiling, per #3348)
  crates/core/src/animation/controller.rs:140  # the struct definition
  crates/core/src/animation/controller.rs:301,307,314,324,331,339  # its own #[cfg(test)] mod

$ grep -RIn "apply_pending_transition" --include="*.rs" crates byroredux tools | grep -v controller.rs
  crates/core/src/animation/mod.rs:17          # the re-export, and nothing else

$ grep -RIn "AnimationController" --include="*.rs" crates/save byroredux/src | grep -v registry_completeness_tests
  →  (empty)   # not even save-registered; its only mention outside the crate
                #  is a rationale string in registry_completeness_tests.rs
$ grep -RIn "AnimationController" --include="*.rs" crates/nif/src
  crates/nif/src/kfm.rs:216,231,297            # three doc comments describing an
                                               #  integration that was never written
```

**Impact**
454 LOC of tested-but-unreachable state-machine code in `byroredux-core`, the most widely-depended-on crate in the workspace, plus three `crates/nif/src/kfm.rs` doc comments that tell a reader the KFM parser feeds a controller it has never fed. Any `AnimationStack` change must keep this parallel consumer compiling for no runtime benefit.

**Related**: #338 (the legacy-audit gap it claimed to close), TD8-2026-09-05-01 (same "shipped ahead of a consumer that never arrived" shape)

**Suggested Fix**
This is a judgement call, not a mechanical delete: either wire it (a KFM-driven actor needs `add_sequence` population at spawn and `apply_pending_transition` in the animation stage), or delete the module + its `animation/mod.rs` re-export + the `registry_completeness_tests.rs` row and reword the three `kfm.rs` doc comments to describe the KFM data as unconsumed. Given the project has no external consumers, "delete and re-derive from `git log` when an actor actually needs sequence blending" is the cheaper posture — but it should be an explicit decision, not an audit default.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
