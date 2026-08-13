# #2656 — SCR-D4-NEW11-01: parse_property_flags reaches across the newline and swallows the Auto of a following Auto State

**Severity**: MEDIUM · **Domain**: papyrus (byroredux-papyrus)
**Location**: `crates/papyrus/src/parser/script.rs:421-453` (`parse_property_flags`), interacting with `mod.rs:77-87` (`peek_with_span`) and `script.rs:551-557` (`parse_state`)

`parse_property_flags` is a `loop { match self.peek() { ... } }`, and `Parser::peek` skips `Token::Newline`. So the flag loop doesn't stop at the end of the property declaration line — it scans into subsequent lines. `Auto` is both a property flag AND the leading token of a top-level `Auto State` item, so a short-form property declaration immediately followed by `Auto State` has its `Auto` consumed by the property's flag loop, and `parse_state` builds the state with `is_auto: false`, silently, no diagnostic. Only vulnerable flag loop (checked all six) — `Auto` is the only flag that's also a legal item-starter. Bounded today: `is_auto` has no runtime consumer, `.pex` path hardcodes it false, `parse_script` has no production caller.

**Suggested fix**: have `parse_property_flags` stop at a `Newline` (peek raw inside the loop, or track the property declaration's line and break on crossing it). Add regression tests (cases A-E from the issue). Make the `r5_round_trip` fixture assertion independent of the trailing doc comment (currently passes only by accident).

---

# #2657 — SCR-D5-NEW11-01: Regression of #2538: known_quest_properties guard never fires on decompiled .pex — the fix is inert on real input

**Severity**: MEDIUM · **Domain**: scripting (byroredux-scripting)
**Location**: `crates/scripting/src/translate/effects.rs:1044-1071` (`receiver_object`), `crates/scripting/src/fragment.rs:1170-1184` (`quest_property_names`), regression test at `effects.rs:1425-1492`

Two independent key-space mismatches make #2538's guard unreachable on the only input the production path ever sees: (1) `quest_property_names` stores the lowercased authored property name (`mq101`), but `receiver_object` looks up the identifier straight off `Expr::Ident` — a decompiled `.pex` auto-property read is the backing variable `::MQ101_var`, not `MQ101`; `ObjectRef::property_name()` strips that decoration but is only applied downstream at dispatch, never before this lookup. (2) the type test is an exact `Type::Object("quest")` match, missing Quest-*derived* script types (`mq206script` etc). Real-corpus evidence: 63 Start/StopScene effects whose receiver is literally a Quest-typed property of the same script, surviving the #2538 fix unchanged. Low *runtime* impact (a dispatch-time fallback already resolves most of these correctly) but the fix itself is inert, and where the guard would fire it declines the *whole fragment* (worse than the fallback).

**Suggested fix**: normalize the lookup key through `ObjectRef::property_name()` semantics before consulting `known_quest_properties`; widen `quest_property_names` to accept Quest-derived script types (or key off the `.pex` property type table). Then use the set *positively*: `explicit_quest_receiver` should accept `QuestRef::Property(name)` when known-Quest, lowering to `Effect::StartQuest` instead of declining. Add a regression test built from a hand-constructed `::X_var` AST (the `.psc` parser can't express this shape).

---

# #2658 — SCR-D5-NEW11-03: fragment_coverage and mq101_conformance measure the context-free lowering path, not the production one

**Severity**: MEDIUM · **Domain**: scripting (byroredux-scripting)
**Location**: `crates/scripting/examples/fragment_coverage.rs:147`, `crates/scripting/examples/mq101_conformance.rs:1407,1450`

Both harnesses call `lower_fragment` (empty quest-property set) while the single production caller (`fragment.rs::populate_quest_fragments_from_script`) calls `lower_fragment_with_quest_properties` with a real set. Today the two paths happen to agree exactly (9361/9361, 11284/11284) *only because of* #2657's bug — once that's fixed, the harnesses will silently diverge from production, and the M47.3 Phase-2 coverage checkbox can't be honestly ticked from the harness as written.

**Suggested fix**: lift `quest_property_names` out of `fragment.rs` into `translate::effects` (or `pub(crate)` + re-export), have both examples call `lower_fragment_with_quest_properties` with the per-script set. Consider marking `lower_fragment` `#[doc(hidden)]`/test-only so future call sites can't accidentally pick the context-free path.

---

# #2659 — SCR-D6-NEW11-02: DeferredFragmentEffects::new deep-clones the whole QuestDefinitionRegistry every frame, before the early-bail

**Severity**: MEDIUM · **Domain**: scripting (byroredux-scripting)
**Location**: `crates/scripting/src/fragment.rs:335-341` (`DeferredFragmentEffects::new`), consumed by `quest_fragment_dispatch_system`; early-bail at `:1372-1374`

The #2539 fix correctly snapshot-clones `QuestDefinitionRegistry` before taking the `(QuestStageState, QuestObjectiveState)` write guards (eliminating a nested-lock issue), but the clone happens unconditionally in `new()`, *before* the `queue.is_empty() || frags.is_empty()` bail. A frame with no quest activity still deep-copies the entire registry. Measured: 0.651 ms/frame on real Skyrim.esm (1811 QUST records); 15.6 ms/frame (entire frame budget) on a synthetic 5,000-quest load order. The registry's only writers run at load time (`&mut World`), so it's immutable for the whole frame being copied.

**Suggested fix**: move the bail ahead of the clone (construct `DeferredFragmentEffects` lazily, only once there's work to do) — or replace the deep clone with an `Arc` snapshot swapped on mutation (sound and free since writers are load-time-only).
