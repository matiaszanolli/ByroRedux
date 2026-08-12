# #2653: SCR-D5-NEW11-02: Reset() and SetActive() are claimed as quest effects through the permissive bare-identifier receiver, mis-lowering ObjectReference.Reset() / Weather.SetActive()

**Severity**: HIGH
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/translate/effects.rs:540-548` (`prim_reset_quest`), `:550-559` (`prim_set_quest_active`), reached through `receiver_quest` (`:974-985`) -> `quest_via` (`crates/scripting/src/translate/compose.rs:121-133`)
**Status**: NEW

## Description

`quest_via`'s bare-identifier arm accepts **any** `Expr::Ident` as `QuestRef::Property(name)` -- no type check and, unlike `receiver_object`, no known-property filter. `prim_reset_quest` matches `<ident>.Reset()` with zero args; `prim_set_quest_active` matches `<ident>.SetActive([bool])`. Both method names are shared with non-Quest types in the game's own API, and neither has the dispatch-time disambiguation fallback that `StartScene`/`StopScene` received.

A quest fragment containing `MyContainerRef.Reset()` therefore does **not** decline: it emits `Effect::ResetQuest`, the fragment is *claimed*, every sibling effect is applied, and the real `ObjectReference.Reset()` semantics are silently dropped.

This is the generalization of #2538 that that fix's sweep missed. #2538 swept `EFFECT_PRIMITIVES` for *intra-table* duplicate method names. The actual hazard class is **table-vs-Papyrus-API** collision: one modeled method name declared on more than one receiver type in the game's own API, reached through a permissive receiver resolver.

## Evidence

Authoritative, from the game's own decompiled base scripts (`scripts\\quest.pex`, `objectreference.pex`, `cell.pex`, `weather.pex` out of `Skyrim - Misc.bsa`; temporary probe run then deleted):

```
reset      ["Cell", "ObjectReference", "Quest"]                        <<< COLLISION
setactive  ["DLC1VQ08BossRoomCleanupScript",
            "MQ206ThroatoftheWorldTriggerScript", "Quest", "Weather"]  <<< COLLISION
```

Behavioural repro (temporary test, run, reverted):

```rust
// "ObjectReference Property MyContainer Auto ... MyContainer.Reset()"
lower_fragment(&body)  // => Some([ResetQuest { quest: Property("MyContainer") }])
// "SomeRef.SetActive(false)"
lower_fragment(&body2) // => Some([SetQuestActive { quest: Property("SomeRef"), active: false }])
```

#2538's context set cannot help -- `MyContainer` is not a Quest property, so it is absent from the set by construction. The 1-arg overload `MyContainer.Reset(SomeMarker)` correctly declines; only the zero-arg call collides, which is the common calling form.

A full sweep of all 14,026 base `scripts\\*.pex` for every modeled method name found these two live collisions and confirmed nine others safe (`AddItem`, `GetOwningQuest`, `SetOpen`, `PlayIdle`, `SetRestrained`, `Wait`, `Start`, `Stop`, and the single-declaring-type remainder).

## Impact

Currently **latent in vanilla Skyrim/FO4** -- measured incidence across 28,758 behavioral fragments (`Skyrim - Misc.bsa` + `Fallout4 - Misc.ba2`): `ResetQuest` **0**, `SetQuestActive` **4, all 4 on genuinely quest-typed properties**. But it is reachable from mods, DLC-embedded scripts, and the unscanned Starfield/FO76 corpora.

When hit: the object is never reset / the weather never applied, `QuestStageState::reset` runs against a non-quest form id, `scene_actor_bindings_dirty` is set spuriously, and every other effect in the fragment is applied as though the whole fragment had been understood.

The domain escalation table rates "recognizer emits a component on an unmodeled term instead of declining" HIGH on impact regardless of likelihood -- an inert unrecognized script is safe, a wrongly-lowered one corrupts game state with no fallback to mask it.

## Related

#2538 (closed -- the `Start`/`Stop` instance of the same class), #2289 (decline-path test coverage), SCR-D5-NEW11-01 in the same report

## Suggested Fix

Gate `prim_reset_quest` and `prim_set_quest_active` on a narrow receiver the way `prim_start_quest`/`prim_stop_quest` are: accept `QuestRef::SelfRef` / `OwningQuest` / a quest-bound local, and (once SCR-D5-NEW11-01's key normalization lands) a *known* Quest-typed property; decline a bare unqualified identifier. Add decline tests for both.

Longer term, the structural fix is to stop letting `quest_via` hand out `QuestRef::Property` for arbitrary identifiers, and to make the base-script method-name sweep a checked-in gate so the next primitive added to the table is validated against the real Papyrus API surface rather than only against its table siblings.

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: The recognizer still declines on every unmodeled term -- a partial lowering is worse than none
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **CORPUS**: Re-run `fragment_coverage` against real Skyrim SE + FO4 archives and record the yield delta
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
