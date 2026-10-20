# TD2-2026-10-19-05: Eight request-drain systems open with the same query-collect / query_mut-remove preamble, and `flight_target` / `push_direction` are the same "level away vector" twice

Labels: low,gameplay,ai,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: preambles `byroredux/src/systems/flee.rs:73-80`, `knockdown.rs:65-72`, `revive.rs:32-39`, `script_requests.rs:43-51,174-181`, `cinematic.rs:132-139,397-445,460-507`; vectors `flee.rs:108-109` vs `knockdown.rs:84-85`
- **Age**: M47.3, 2026-10-17/18 (all new this window)
- **Effort**: trivial
- **Description**: `match world.query::<R>() { Some(q) if !q.is_empty() => q.iter().map(..).collect(), _ => return }` then `if let Some(mut q) = world.query_mut::<R>() { for .. { q.remove(..) } }`. The vector twin is `Vec3::new(a.x-b.x, 0.0, a.z-b.z).normalize_or_zero()` + "if zero use `Vec3::X`", byte-identical.
- **Evidence**: none yet; the family grew by five systems in two days and each re-spelled the preamble. (`scripting/src/cleanup.rs:239 drain_component` is the discard-only sibling.)
- **Proposed fix**: `pub(crate) fn take_requests<T: Component + Copy>(world: &World) -> Vec<(EntityId, T)>` in `systems/mod.rs` (query, collect, remove, one lock each), and `fn level_away(from: Vec3, to: Vec3) -> Vec3` in `systems/locomotion.rs` beside `step_toward_detailed`; both callers `flee_target`/`push_direction` keep only their own scale/lift.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
