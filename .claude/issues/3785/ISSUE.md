# #3785 — SCR-D5-2026-08-30-01: an Effect::Conditional guard whose quest cannot be resolved does not decline — it silently selects the else branch and runs its effects

**Severity**: MEDIUM · **Location**: `crates/scripting/src/fragment.rs::apply_effects`
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-30.md` (SCR-D5-2026-08-30-01)

`Effect::Conditional`'s guard evaluation used `is_some_and`, which collapses "the guard was
evaluated and is false" and "the guard's quest ref could not be resolved at all" into the same
`false`. Unlike every other `resolve_quest_logged` caller (which simply skips the one effect via
`?`), `Conditional` has an `else` arm — `false` is NOT inert, it selects and runs `else_effects`
(potentially `SetStage`/`SetObjectiveCompleted`/`Disable`/`SetGlobalValue`). Reachable via
`QuestRef::Property` on a quest whose named property is absent from VMAD or alias-bound (#2186);
871 `Conditional` effects exist across the Skyrim+FO4+Starfield corpus per the sibling audit.

## Fix implemented

- `apply_effects`'s `Conditional` arm now distinguishes the third state: a `resolved` flag tracks
  whether every guard resolved. When any guard doesn't, the whole `Conditional` is declined
  (`continue` — neither `then_effects` nor `else_effects` run; the fragment body's remaining
  effects still execute normally, matching every other declining-effect site's semantics).
- Added a dedicated `log::warn!` at the guard-declining site — `resolve_quest_logged`'s own
  `debug!` line stays unchanged for its many inert callers; this is the one site where the
  consequence is a chosen branch, so it gets a louder, more specific diagnostic.

Regression test (issue's own TESTS checklist item):
`apply_effects_declines_conditional_with_unresolvable_guard` — a `Conditional` with a
`QuestRef::Property` guard and no VMAD (unresolvable) plus non-empty, distinguishable
`then_effects`/`else_effects`; asserts zero `QuestStageAdvanced` and neither target stage marked
done. Verified live: stashing the fix makes the test fail exactly as expected (`else_effects` ran,
advanced to stage 7) before restoring.

**SIBLING** (issue's own checklist item): grepped every `resolve_*(...).is_some_and(...)` /
`.unwrap_or(false)` chain in `crates/scripting/src/`. Found one more candidate —
`actors_3d_loaded` (`fragment.rs`), which feeds `WaitForActors3DLoaded`'s suspend/poll decision.
Its `false` branch is also non-inert in the sense that an unresolvable actor ref is
indistinguishable from "not yet loaded" and both cause the same poll-and-retry — but the
consequence there is a silent infinite retry loop, not a wrong state-changing effect execution,
a meaningfully different severity/shape than this issue's defect. Not fixed here — flagged for
separate triage since it needs its own investigation (timeout policy, diagnostic).

**LOCK_ORDER**: verified no-op — the fix is pure local control flow around already-acquired
`guards`/`stages` references; no `World`/`RwLock` acquisition added or reordered.

Full workspace: `cargo test --no-fail-fast` 7038 passing, 0 failing.
