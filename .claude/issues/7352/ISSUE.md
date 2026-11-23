# #7352: SAVE-07: the 449ea0fed baseline refresh absorbed an undocumented new `Option` field in a saved type (`VmResume.answer`) with no bump

- **Severity**: LOW
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:1349-1353` (justification); `crates/scripting/src/translate/effects/vm_bridge.rs:14-24` (`VmResume`) and `:50-56` (`VmStep::Replay`), both from b35f2881e.
- **Status**: NEW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (v135 saves are refused by the major since v136; no live exposure)
- **Where**: `byroredux/src/save_io/serde_default_guard_tests.rs:1349-1353` (justification); `crates/scripting/src/translate/effects/vm_bridge.rs:14-24` (`VmResume`) and `:50-56` (`VmStep::Replay`), both from b35f2881e.
- **Premise (quoted)**: justification: "`FragmentResumeCondition::MessageAnswered { ticket }` was appended ... no existing variant or field moved". The same refresh also swallowed `pub answer: Option<i32>` on `VmResume` (inside saved `Effect::Interpret { resume }` continuations) and the `VmStep::Replay(u32)` variant. The skill's rule (and `serde_default_guard_tests`' own policy comments): a new `Option` in a saved type bumps, because a missing `Option` field default-fills on decode, which is the `#[serde(default)]` mechanism the blanket rule forbids (#1714).
- **Problem**: the enum variants are name-tagged and read-compatible, so the refresh without a bump is defensible for them. The `answer` field is not covered by that argument: a pre-b35f2881e v135 save decodes only because serde fills a missing `Option` with `None`. Harmless here (a missing answer is "no answer yet"), but the baseline comment misdescribes what moved, and the next such field may not default to a safe value.
- **Fix sketch**: amend the comment at the refresh (or the skill's triage list) to name `VmResume.answer` / `VmStep::Replay`; no code change needed.
- **Dedup**: none.
- **Dimension agent ID**: SAVE-D2-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other parked-state paths, load-drain steps, notification callers and `atomic_write` callers the finding names)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_SAVE_2026-11-23.md`
