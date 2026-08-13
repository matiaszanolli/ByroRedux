# Investigation notes

## #2657 — mostly already fixed by an unrelated intervening commit

Before touching anything, reading `crates/scripting/src/translate/effects.rs` showed
`receiver_object` and `explicit_quest_receiver` ALREADY normalizing the lookup key
through `quest_property_key` (strip `::` prefix / `_var` suffix, lowercase) — the
exact fix #2657's mismatch #1 asked for, including a doc comment explicitly citing
"(#2657)". `git log -S "fn quest_property_key"` traced it to `53f7de9d` ("Fix #2653:
require an unambiguous quest receiver for Reset() and SetActive()"), landed the same
day as #2657 was filed, which incidentally normalized the same key space while fixing
a different, adjacent ambiguity. A hand-built `::MQ101_var` AST test already existed
too (`effects.rs` test `quest_start_on_a_direct_property_declines_rather_than_mislowering_to_scene`).

What genuinely remained open: mismatch #2 (the type test is exact `Type::Object("quest")`,
missing Quest-*derived* script types like `mq206script`). Investigated whether this is
fixable without new infrastructure — it isn't: `quest_property_names` only has the one
`Script` AST in scope, with no script-class-hierarchy resolver anywhere in the codebase
to answer "does `mq206script` transitively extend `Quest`?". Building one is out of
scope for this pass (would need either loading + extends-chain-resolving every script
in a load order, or a `.pex`-side type table with the same missing hierarchy info).
Not attempted — documented in-code (`quest_property_names`'s doc comment) and flagged
here as a candidate follow-up issue rather than guessed at with a naming heuristic
(e.g. "ends in *quest*" would violate the project's no-guessing policy and misclassify
real non-quest scripts).

Added one regression test exercising the FULL production pipeline
(`populate_quest_fragments_from_script`, not just the lower-level `classify_effect`
primitive) with a hand-built `::MQ101_var.Start()` AST, confirming mismatch #1's fix
holds end-to-end through the real call path.

## #2658 — real corpus measurement after the fix

Ran `fragment_coverage` (release) against `Skyrim - Misc.bsa` (14026 .pex) +
`Fallout4 - Misc.ba2` (7875 .pex) after switching it to
`lower_fragment_with_quest_properties`:

```
fully lowered (claimed): 9361 (32.6% of behavioral)
StartQuest   44   (was ~0 in the issue's pre-fix "context-free" measurement)
StopQuest   747   (was ~728)
StartScene  810
StopScene   249
```

Non-zero `StartQuest`/`StopQuest` on real content, where the issue's evidence showed
near-zero pre-fix, is the direct empirical confirmation that #2657's key-normalization
fix is now actually exercised by a harness that matches production. Total "claimed"
count is unchanged (9361) — expected, since this fix reclassifies WITHIN the claimed
set (StartScene → StartQuest), it doesn't change whether a fragment claims at all.

## #2659 — Arc snapshot chosen over "move the bail"

The issue offered two alternatives. Investigated "move the bail ahead of the clone"
first and found it's not a simple reorder: `DeferredFragmentEffects::new` (which does
the `QuestDefinitionRegistry` clone) must run OUTSIDE the `QuestStageState`/
`QuestObjectiveState` mutable-guard scope — that's the specific lock-ordering
constraint `#2539` (`6ad64ef6`) fixed, confirmed by reading that commit directly. Since
whether there's "work to do" (`queue.is_empty()`) can only be known AFTER draining the
journal, which needs those same guards, the only way to bail-before-clone is to
acquire the guards twice (build+check the queue, drop, clone the registry, re-acquire
for dispatch) — and this codebase's scheduler is genuinely parallel (rayon-backed,
`crates/core/src/ecs/scheduler.rs`), so a dropped-then-reacquired guard window is a
real (if narrow) behavioral change, not just a lock-ordering non-issue.

Took the lower-risk second option instead: made `QuestDefinitionRegistry`'s clone
O(1) by `Arc`-wrapping its internal map, so the existing unconditional clone stops
being expensive rather than trying to avoid calling it. Zero control-flow changes in
`fragment.rs`'s dispatch system. Verified soundness (both writers take `&mut World`,
so no reader can hold a stale `Arc` clone while a write via `Arc::make_mut` happens)
and correctness (a write after a snapshot is taken must clone-on-write, not mutate the
snapshot) with dedicated tests in `quest_stages.rs`.
