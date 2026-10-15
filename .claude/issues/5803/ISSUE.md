# TD1-2026-10-15-04: `DeferredFragmentEffects::apply_at_depth` is 621 lines of 45 inline queue flushes; `apply_effect` (562, 67 arms) and `evaluate_guard` (305, 40 arms) are the same shape

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-04`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects/deferred.rs:380-1000` (`apply_at_depth`); `crates/scripting/src/fragment/effects.rs:138` (`apply_effect`); `crates/scripting/src/fragment/effects/evaluate.rs` (`evaluate_guard`)
- **Status**: NEW (the pre-split `apply_at_depth` was 188 lines, tracked under #5468 which closed on the file split, not the function)
- **Age**: 188 → 621 since 10-12 (M47.2 natives: `a0045b0cf`, `5320d4ac9`, `8b3b75d4d`, `e30e8bc2e`, …)
- **Effort**: small (apply_at_depth), medium (the other two)
- **Description**: `apply_at_depth` is a straight list of drains, `for x in take(&mut self.messages) {…}`, `if !self.keyword_changes.is_empty() {…}`, … ×45, each 5–40 lines that call a subsystem function. The only non-flat part is the provider-step recursion at L940–1000 that must stay last. Because the order is semantic (the #3946 comment above it), extracting each block into `fn flush_<queue>(&mut self, world)` and calling them in the same order is a mechanical, behaviour-preserving cut. `apply_effect` is a 67-arm `match` that already delegates its newest families to `effects/{ai_combat,inventory,lock,…}.rs`; the remaining arms should follow so the function is a dispatcher.
- **Evidence**: `awk` over `deferred.rs` L380–1010 for `^        (for|if|let|…)` yields the 45 flush heads listed in order; `fnscan.py`: `apply_at_depth 621 depth 8`, `apply_effect 562`, `evaluate_guard 305`.
- **Impact**: depth-8 nesting in the function every fragment goes through; a reorder bug here moves a gameplay effect across a lock boundary.
- **Related**: #3946 (the barrier-cap comment), #5468, finding 03.
- **Suggested Fix**: `impl DeferredFragmentEffects { fn flush_messages, flush_deletions, … }` in `deferred.rs` (or one per family file next to its applier), `apply_at_depth` becomes a 60-line ordered call list; keep the `fragment.rs::SOURCES` concat valid by not changing file names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
