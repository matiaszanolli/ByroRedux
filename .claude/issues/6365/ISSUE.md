# TD2-2026-10-25-06: 45 ObScript effect handlers repeat the same caller prologue; 15 of them also open a `Binder`

Labels: low, scripting, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6365
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `crates/scripting/src/obscript_effects.rs` — `let caller = caller_of(context); let caller_entity = live(world, caller); if caller_entity.is_none() { return Action::Declined; }` ×45 (full 6-line `+ let mut bind = Binder::default(); let me = bind.actor(caller);` ×15, e.g. `:229-237`, `:262-270`, `:274-282`); 78 `live(world, caller)` and 87 `caller_of(context)` in the file overall.
- **Age**: all of it arrived with the #6139 table refactor (bd4cabf81 … d5d13edcc, 2026-10-24); `4a3e89d56` had 0 full prologues.
- **Effort**: small
- **Fix**: `run_row` (`obscript_cmd.rs:146`) already declines a missing caller for `needs_caller` rows. Extend it to resolve the live entity once: add `Command.needs_live_caller` (or a `Context::caller_entity: Option<EntityId>` filled by `run_row`) and a helper `fn open_actor_effects(world, context) -> Option<(u32, Binder, ActorRef)>` in `obscript_effects.rs` for the 15 Binder handlers. Each handler loses 5-6 lines; the "live caller" policy is then one line in `run_row`.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
