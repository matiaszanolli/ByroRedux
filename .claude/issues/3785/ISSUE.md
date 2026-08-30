# #3785 — SCR-D5-2026-08-30-01: an Effect::Conditional guard whose quest cannot be resolved does not decline — it silently selects the else branch and runs its effects

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: medium, scripting, quests, bug

---

**Audit**: `/audit-scripting` — `docs/audits/AUDIT_SCRIPTING_2026-08-30.md` (Dimension 5 — Recognizer-Chain Soundness, dispatch half), HEAD `64f64480`
**Finding ID**: `SCR-D5-2026-08-30-01`

- **Severity**: MEDIUM
- **Status**: NEW
- **Untrusted-Input**: No

## Location

`crates/scripting/src/fragment.rs:1349-1357`

## Description

```rust
let passes = guards.iter().all(|guard| {
    resolve_quest_logged(&guard.quest, context, vmad)
        .is_some_and(|quest| stages.get_stage_done(quest, guard.stage) == guard.done)
});
let branch = if passes { then_effects } else { else_effects };
```

`is_some_and` collapses two distinct outcomes into one `false`: *"the guard was evaluated and is false"* and *"the guard could not be evaluated at all"*.

The 2026-08-24 pass checked this arm and correctly concluded it does not wrong-default to `true`; the question nobody asked is **what happens on the `false` side**. Because a `Conditional` has an `else` arm, `false` is **not inert** — it runs code. So an unevaluable predicate executes the branch the author reserved for the predicate being definitively false, which can be a `SetStage`, `SetObjectiveCompleted`, `Disable`, or `SetGlobalValue`.

This is the decline-on-unmodeled invariant applied one layer later. Every sibling site gets it right: `apply_quest_scoped_effect`'s `resolve_quest_logged(quest, context, vmad)?` propagates `None` and the effect is simply not applied; `resolve_object` / `resolve_actor` decline the same way. **`Effect::Conditional` is the one place where "cannot resolve" has a *consequence*.**

The tell is in the log line itself — `"fragment effect skipped: unresolved quest ref {via:?}"` is accurate for every other caller and actively wrong for this one, where nothing is skipped and a branch is chosen.

## How reachable

`QuestRef::SelfRef` / `OwningQuest` always resolve to the dispatch context, so the common intra-quest `GetStageDone(N)` guard is safe.

The exposure is `QuestRef::Property(name)`, which returns `None` when the named property is absent from the quest's registered VMAD **or** when it is alias-bound (`alias >= 0`, declined at `ScriptInstance::object_form_id` per #2186). Correctly authored content should hit neither, so this is latent-not-live — hence MEDIUM rather than the HIGH the severity table assigns the recognizer-side analogue.

It is filed rather than dropped because the failure is **silent**, has no fallback, and `AUDIT_SCRIPTING_2026-08-27.md`'s live-corpus histogram counts **871** `Conditional` effects across Skyrim + FO4 + Starfield — the shape is not hypothetical.

## Evidence

Re-verified at HEAD: `fragment.rs:1349-1353` unchanged, still `is_some_and`, with `let branch = if passes { then_effects } else { else_effects };` immediately following.

## Suggested Fix

Distinguish the third state:

```rust
let mut resolved = true;
let passes = guards.iter().all(|guard| {
    match resolve_quest_logged(&guard.quest, context, vmad) {
        Some(q) => stages.get_stage_done(q, guard.stage) == guard.done,
        None => { resolved = false; false }
    }
});
if !resolved { continue; }   // decline the whole Conditional — run neither branch
```

and reword `resolve_quest_logged`'s message, or give the guard path its own `log::warn!` (an unresolvable guard is a data defect worth surfacing, not a routine skip).

**Regression guard**: a `Conditional` with an unbound `QuestRef::Property` guard and non-empty `else_effects` must apply **neither** branch.

## Related

- #2186 (alias-bound properties decline at `ScriptInstance::object_form_id` — one of the two ways the resolve returns `None`)
- #3279 (`Effect::Conditional`'s `lower_statements` recursion depth cap — same effect kind, different defect)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `is_some_and` / `unwrap_or(false)` over a `resolve_*` result in `crates/scripting/src/`, especially any with a non-inert false arm
- [ ] **LOCK_ORDER**: If a RwLock scope changes around the guard evaluation, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — an unresolvable guard must run neither branch
