# SCR-D5-NEW10-01: MyQuest.Start()/Stop() on a direct VMAD Quest property silently mis-lowers to StartScene/StopScene instead of StartQuest/StopQuest or a clean decline

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2538
**Finding ID**: SCR-D5-NEW10-01

**Severity**: HIGH
**Dimension**: Recognizer-Chain Soundness
**Untrusted-Input**: No — a real-VMAD/real-`.pex`-data correctness gap, same class as the closed #2286
**Location**: `crates/scripting/src/translate/effects.rs:460-473` (`explicit_quest_receiver`, new this session), consumed by `prim_start_quest`/`prim_stop_quest` (`:475-493`); collides with the pre-existing `prim_start_scene`/`prim_stop_scene` (`:629-647`); `EFFECT_PRIMITIVES` table order (`:354-386`): `prim_start_quest`/`prim_stop_quest` are listed *before* `prim_start_scene`/`prim_stop_scene` ("first match wins")
**Status**: NEW (introduced this session by `a844c26b`)

## Description
Papyrus's `Quest.Start()`/`Quest.Stop()` and `Scene.Start()`/`Scene.Stop()` share the identical zero-arg AST shape `<ident>.Start()` / `<ident>.Stop()` — nothing in the AST alone distinguishes a `Quest Property` from a scene-form property; that information only lives in VMAD property-type metadata, which the translate-time recognizer chain does not consult. Before this session, only `prim_start_scene`/`prim_stop_scene` existed for this shape, using the permissive `receiver_object` fallback (any bare `Ident` not otherwise classified → `ObjectRef::Property(name)`). This session added `prim_start_quest`/`prim_stop_quest` *ahead* of them in the table, guarded by a new, deliberately narrower resolver (`explicit_quest_receiver`) that only accepts `Self`, `GetOwningQuest()`, or a local already bound via an explicit `Quest k = …` declaration — it declines every bare, unbound `Quest`-typed VMAD property reference (the single most common real-world shape: a controller script calling `SomeQuestProperty.Start()` on a quest it doesn't own, without first copying it to a local). That decline is safe *in isolation*, but because `prim_start_quest` returning `None` simply falls through to the next table entry rather than terminating the fragment, the same bare identifier is then picked up by the unmodified, fully-permissive `prim_start_scene`/`prim_stop_scene` and silently accepted as a scene reference. The chain never re-considers "maybe this bare identifier is actually a Quest and I should decline the whole statement" — it commits to whichever primitive matches first, and for this one shape the newly-added guard just hands the ambiguous case to the wrong sibling instead of removing the ambiguity.

## Evidence
Empirically reproduced (temporary test added, run, then reverted — working tree confirmed clean):
```rust
let body = first_fn_body(
    "ScriptName QF extends Quest\n\
     Quest Property MQ101 Auto\n\
     Function Fragment_99()\n\
     MQ101.Start()\n EndFunction\n",
);
lower_fragment(&body)
// => Some([StartScene { scene: Property("MQ101") }])
```
A genuine `Quest Property MQ101 Auto` called with `.Start()` — the MQ101-quest-controller idiom this audit's evidence base has repeatedly cited as real corpus content — lowers to `Effect::StartScene`, not `Effect::StartQuest` and not `None`. The crate's own pre-existing test `lowers_scene_start_and_stop_requests` pins the *exact* AST shape (a bare property identifier) that a real `Quest.Start()` call also produces; nothing in the recognizer syntactically distinguishes the two. Confirmed directly: `EFFECT_PRIMITIVES` table lists `prim_start_quest`/`prim_stop_quest` at lines 355-356, `prim_start_scene`/`prim_stop_scene` at 368-369 (quest before scene).

## Impact
Any vanilla or modded quest-controller script that calls `.Start()`/`.Stop()` on a directly-referenced (not locally-rebound) `Quest Property` — the ordinary way one script starts another quest it doesn't own — silently mis-lowers to a scene-start/stop request. At effect-application time this very likely *looks* harmless (the "scene" lookup for a form that is actually a `QUST` record has no `SceneRegistry` entry and silently no-ops), so the practical symptom is **the quest silently never starts/stops** — no crash, no log-visible contradiction, just a quest that should be running and isn't. This is precisely the "silently corrupts game logic" failure mode the dimension's invariant exists to catch.

## Related
Same conceptual defect family as the closed #2286 (a hand-authored assumption instead of declining) but a different mechanism — table-order collision between two separately correct primitives rather than a wrong literal mapping inside one. Not a duplicate of any currently open issue.

## Suggested Fix
Make the ambiguity mutual instead of one-sided. Either (a) have `prim_start_scene`/`prim_stop_scene`'s object resolver decline when the same bare identifier could plausibly be a Quest (track which property names appear as a `Quest`-typed VMAD property, already knowable from `script_instance`'s property table) and decline both `prim_start_quest` *and* `prim_start_scene` when receiver identity can't be disambiguated at translate time; or (b) resolve both candidates lazily at effect-application time (a single ambiguous `Effect::StartQuestOrScene { name }` variant, VMAD-typed resolution at apply time picks the correct one, declining only if neither matches) rather than committing to one interpretation during translation. Add a regression test pinning the exact repro above asserting it does **not** silently become `StartScene`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the exact repro (`Quest Property MQ101 Auto; MQ101.Start()`) and asserts it does not silently lower to `StartScene`
- [ ] **SIBLING**: Check for any other pair of primitives sharing an identical AST shape with table-order-dependent disambiguation
