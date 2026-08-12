# #2657: SCR-D5-NEW11-01: Regression of #2538: known_quest_properties guard never fires on decompiled .pex -- the fix is inert on real input

**Severity**: MEDIUM
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/translate/effects.rs:1044-1071` (`receiver_object`, the un-normalized key), `crates/scripting/src/fragment.rs:1170-1184` (`quest_property_names`), regression test at `crates/scripting/src/translate/effects.rs:1425-1492`
**Status**: Regression of #2538 (incomplete fix, `90ae915c`)

## Description

#2538 was closed by threading the containing script's Quest-typed property names into `lower_fragment_with_quest_properties`, so `receiver_object` would decline a bare identifier known to be Quest-typed instead of accepting it as a scene reference. **Two independent key-space mismatches make that guard unreachable on the only input the production path ever sees.**

1. `quest_property_names` stores the *authored* property name lowercased (`mq101`), but `receiver_object` looks up the identifier taken straight off the `Expr::Ident` -- and a decompiled `.pex` auto-property read is the **backing variable** `::MQ101_var`, not `MQ101`. `ObjectRef::property_name()` (`crates/scripting/src/translate/compose.rs:72-78`) exists precisely to strip that `::`/`_var` decoration, but it is only applied downstream at dispatch, never before this lookup.
2. The type test is an exact `Type::Object("quest")` match, so properties typed with a Quest-*derived* script (`mq206script`, `dn019script`, `min03script`, ...) are never collected at all.

A parent-script property (via `extends`) is also invisible, but that is a non-issue in practice: `QF_` scripts extend `Quest`, which declares no Quest-typed properties.

## Evidence

Instrumented `crates/scripting/examples/fragment_coverage.rs` to run **both** entry points over the same real corpus (temporary edit, run, reverted -- tree verified clean):

```
fully lowered, context-free : 9361   effects: 11284
fully lowered, production   : 9361   effects: 11284
fragments claimed context-free but DECLINED in production: 0
```

Byte-identical. Cross-checking each lowered effect's receiver against its declared property type in the same script:

```
== StartScene(scene)          == StopScene(scene)
   quest        44               mq206script   1
   scene       810               quest        19
                                 scene       248
```

-> **63 `Start`/`StopScene` effects whose receiver is literally a `Quest`-typed property of the same script**, all surviving the fix unchanged (`mq102`, `seranacurequest`, `dlc1vq00`, `da13`, `db11`, `bos301`, `bosm01`, `dlc2mq02freatemplescenequest`, ...).

Direct AST-level repro (temporary test, run, reverted) -- building the AST the decompiler actually emits rather than one the `.psc` lexer can produce:

```rust
// `::MQ101_var.Start()`, with the correct quest-property context supplied
lower_fragment_with_quest_properties(&body, &["mq101"].into())
// => Some([StartScene { scene: Property("::MQ101_var") }])
```

The shipped regression test **cannot** catch this: the `.psc` `Ident` regex is `[a-zA-Z_][a-zA-Z0-9_]*` (`crates/papyrus/src/token.rs:253`), so no `parse_script`-built test body can ever contain a `::X_var` receiver.

Corroborating corpus signature: `StartQuest` **0** vs `StopQuest` **728** and `StartScene` **854**. `Self.Stop()` resolves through `explicit_quest_receiver`, while every *cross-quest* `X.Start()` still becomes `StartScene`.

## Impact

Low *runtime* impact today -- and this corrects the original #2538 report's impact claim. `a844c26b`, the **same commit that introduced the ambiguity**, also added a dispatch-time fallback (`crates/scripting/src/fragment.rs:506-568`) that resolves a `StartScene`/`StopScene` form-id against `QuestDefinitionRegistry` when it misses `SceneRegistry`, and performs the quest start/stop instead. So "the quest silently never starts" was already wrong at filing time, and the fix was written against a bad premise.

The real costs:
1. the codebase now carries a threaded-metadata mechanism, a `HashSet` clone per fragment, and a green regression test that collectively assert an ambiguity is resolved at translate time when it is not;
2. one genuine behavioural divergence remains -- the fallback early-returns on `deferred.quest_definitions.as_ref()?`, so with no `QuestDefinitionRegistry` populated a quest start is dropped, where `Effect::StartQuest` would still call `stages.start_quest(quest, None)`;
3. where the guard *would* fire it declines the **whole fragment**, discarding every sibling effect -- strictly worse than the fallback.

The metadata already threaded is exactly what is needed to resolve the ambiguity *positively* instead.

## Related

#2538 (closed), `a844c26b`, `90ae915c`, SCR-D5-NEW11-03 (why no instrument caught this), SCR-D5-NEW11-02 (the same class, different method pair)

## Suggested Fix

Normalize the lookup key through `ObjectRef::property_name()` semantics (strip a `::` prefix and `_var` suffix) before consulting `known_quest_properties`, and widen `quest_property_names` to accept Quest-derived script types -- or, better, key off the `.pex` property type table rather than the AST.

Then use the set **positively**: have `explicit_quest_receiver` accept `QuestRef::Property(name)` when the name is a known Quest property, so `MQ101.Start()` lowers to `Effect::StartQuest` instead of declining the fragment.

Add a regression test built from a hand-constructed `::X_var` AST (the `.psc` parser cannot express it).

## Completeness Checks
- [ ] **DECLINE-INVARIANT**: The recognizer still declines on every unmodeled term -- a partial lowering is worse than none
- [ ] **CORPUS**: Re-run `fragment_coverage` against real Skyrim SE + FO4 archives and record the yield delta
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
