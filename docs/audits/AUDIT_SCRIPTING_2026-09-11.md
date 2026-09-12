# Scripting Subsystem Audit — 2026-09-11

Eighteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_2026-09-06.md`). Run as the
skill prescribes — orchestrator + eight dimension agents (launched in two
batches of four, to respect the "max 3 concurrent" guidance loosely — batch 1
covered Dims 1–4, batch 2 Dims 5–8) — each writing `/tmp/audit/scripting/dim_N.md`
before this consolidation.

**Scope**: `crates/pex`, `crates/papyrus`, `crates/scripting` (owner crates),
`crates/hkx` (Dim 8), and the engine-side attach / cinematic / cell-loader
wiring (Dims 7–8). **Explicitly out of scope, still a reported coverage gap**
(unchanged from the 09-06 pass): the SKSE/JContainers/StorageUtil/ObScript
compatibility layer — `crates/scripting/src/{papyrus_provider/, obscript.rs,
obscript_runtime.rs, compatibility.rs}` and `crates/sdk` (now ~14k LOC). This
pass only re-audited the *seams* the existing recognizer/effect chain and
attach path make into that layer (Dims 5–7), which is exactly where the prior
pass's HIGHs lived — none were found this cycle.

**Method**: read `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` in full first,
diffed the 187 commits since its baseline commit `d9da17e3` (HEAD `b3db49fa`
at pass start) per dimension's file set, re-read current source for every
prior finding rather than trusting commit messages, ran `cargo check`/`cargo
test` scoped per crate, and — for the two highest-value prior HIGHs
(SCR-D3-2026-09-06-01's stack-overflow-via-re-fold bug, SCR-D8-2026-09-06-01's
lock-order inversion) — independently re-traced the fix mechanism end-to-end
(memoized `Node::depth`; the `drop(registry)` placement) rather than accepting
a passing regression test alone as proof.

**Dedup baseline**: `gh issue list --limit 300` (saved to
`/tmp/audit/scripting/issues.json`, cleaned up per Phase 4),
`docs/audits/AUDIT_SCRIPTING_2026-09-06.md`, and `git log d9da17e3..HEAD`
filtered per dimension.

## What changed since 2026-09-06

187 commits on `main` touched this domain (vs. 481 for the prior cycle's
window). The dominant shape of this window is **remediation, not growth**: a
batch explicitly titled "the five scripting HIGHs" (`2d1ad834`, Fix #3933–
#3937) closed every HIGH the 09-06 report filed, followed by four more
"scripting audit follow-up" batches (`f1a9b984` #3940–#3943, `5d0d96ef`
#3945–#3947/#3952, `a4cbe36f` #3930/#3944/#3953/#3954, `7a676119` #3955) that
closed every MEDIUM and every LOW with a concrete suggested fix, plus three
independent fixes for long-open issues (`df8eb2f3` #3487 MoveTo,
`4d5a80f5` #3496 arity ceilings, `e26579c1` #3159 Lock/SetLockLevel) and one
settling commit for a previously-inconsistent runtime model (`def298af`
#3892, `QuestStageState` event-subscriber model). `crates/hkx` had zero
commits in this window.

| Commit(s) | Effect on this domain |
|---|---|
| `2d1ad834` Fix #3933–#3937 ("the five scripting HIGHs") | Closes SCR-D3-2026-09-06-01 (decompiler stack-overflow via re-fold — `Node::depth` is now memoized on construction, checked at every fold and in `boolean::combine`), SCR-D5-2026-09-06-01 (provider-catalog-before-canonical precedence — `classify_effect` now runs first), SCR-D5-2026-09-06-02 (`apply_effects` O(N²) recursion — now an iterative borrowed-slice cursor stack), SCR-D6-2026-09-06-01 (SCEN package `Activate` marker ordering — routed through `PendingFragmentActivations`), SCR-D8-2026-09-06-01 (`SceneRegistry` lock-order inversion — `drop(registry)` restored before the quest acquisitions) |
| `20772317` Fix #3939 | Closes SCR-D5-2026-09-06-03 (`servable_catalog()` gates on `callback.is_some()`) |
| `f1a9b984` Fix #3940–#3943 | Closes SCR-D5-2026-09-06-04 (three-case `Option<Option<T>>` property reads), SCR-D7-2026-09-06-01 (statics-family `SCRI` arm added to `base_record_script`), SCR-D3-2026-09-06-02 (auto-state case predicate), SCR-D1-2026-09-06-02 (exhaustive-prefix truncation test) |
| `5d0d96ef` Fix #3945–#3947, #3952 | Closes SCR-D4-2026-09-06-02 (shared `pub` depth constants), SCR-D5-2026-09-06-06 (`CanonicalEvent::from_papyrus` now has a caller), the merged SCR-D5/D6-05 (`MAX_PROVIDER_FRAGMENT_BARRIERS` cap-before-descend) |
| `cf47d976` Fix #3948 | Closes SCR-D5-2026-09-06-07 (panic net widened to the whole PEX sequence) |
| `363d2eb7`/`19505237`/`cc29c06f` Fix #3949/#3950/#3951 | Close SCR-D6-2026-09-06-02/03/04 (doc drift, `OnEquipEvent` doc rot, `Access` under-declaration) |
| `df8eb2f3` Fix #3487 | Closes the long-open `MoveTo` structural-zero-decline issue — lowers the compiler-materialized default-arg shape |
| `4d5a80f5` Fix #3496 | Bounds arity on the last four unbounded primitives; adds a mechanized completeness scan so a future primitive can't reintroduce the class |
| `e26579c1` Fix #3159 | Adds `Effect::SetLocked`/`SetLockLevel` |
| `def298af` Fix #3892 | Deletes the dead dynamic quest-event-subscriber model; settles on three compile-time subscriber-id constants |
| `a4cbe36f` Fix #3930/#3944/#3953/#3954 | Closes SCR-D8-2026-09-06-02 (router/gate predicate sharing) and other follow-ups |
| `7a676119` Fix #3955 | Closes SCR-D8-2026-09-06-03 (stale doc reattached) |
| `8c5e02aa` refactor(boot) | Splits `boot.rs` into `boot/` (schedule/{early,update,post_update,physics,late}.rs) — scheduling-order findings now cite `boot/schedule/*.rs`, not `boot.rs` |

## Build & test state — CLEAN

```
$ cargo test -p byroredux-pex --all-targets
   68 passed; 0 failed; 1 ignored (r5_fidelity, game-data)

$ cargo test -p byroredux-papyrus --all-targets
   96 passed; 0 failed (up from 91+4)

$ cargo test -p byroredux-scripting
   440 passed; 0 failed; 4 ignored (game-data/archive gated)
   (up from 411/4 at the 09-06 pass — +29, almost all new regression guards
   for this cycle's fixes)

$ cargo test -p byroredux-scripting -p byroredux-plugin --lib
   961 + 440 passed; 0 failed; 27 ignored

$ cargo test -p byroredux --bin byroredux boot::schedule
   11 passed; 0 failed

$ cargo test -p byroredux --bin byroredux the_scene_registry_guard_is_released_before_the_quest_acquisitions
   1 passed

$ cargo check -p byroredux
   clean, no warnings surfaced
```

No failures anywhere in the domain this pass.

## Headline verdicts

### Untrusted-input robustness — **MET this cycle: no known panic/OOB/OOM/abort path**

The 09-06 pass's verdict ("NOT MET: a well-formed `.pex` can still abort the
process") is now closed. SCR-D3-2026-09-06-01 — the stack-overflow-via-re-fold
bug that bypassed `#3783`'s original per-call depth ledger — is fixed
correctly and completely, not just for the one shape it was found through:
depth is now a memoized field on `Node` itself (`node.rs`), maintained by
`Node::new` at construction and by the tree's one in-place mutation site
(`lift::replace_constant_id`), and checked both in `lift::rebuild_expression`'s
fold loop and in `boolean::combine` (which previously bypassed the cap
entirely via its iterative `reprocess` loop). Both dimensions that examined
this (Dim 2 via direct repro against the exact prior-bypassing shapes; Dim 3
via independent code trace) confirm no remaining bypass. The `call_sites.rs`
preflight DoS (SCR-D1-2026-09-06-01, 11.6s on a 1.7MB wire-valid `.pex`) is
also fixed — O(F·D) linear-`find`-per-function replaced by an O(D) indexed
build + O(1) lookup, verified against the exact adversarial shape the prior
report measured. The panic net (`catching_panics`) now wraps the whole
parse → preflight → decompile → provider-lower → recognize sequence for every
production entry variant, not just the decompile step.

### The 99.996% decompile-rate claim — **still measures what it claims**

Unchanged mechanism (`pex_corpus_smoke.rs`'s three-bucket `catch_unwind`
tally); the one LOW caveat from the prior pass (the harness's
`expected_top_level_item_count` using a stale case-sensitive auto-state
predicate) is fixed — both call sites now route through a shared
`pub fn is_auto_state`.

### The `.psc`-vs-`.pex` fidelity gate — **both halves still pin equality, unchanged**

### The decline-on-unmodeled invariant — **held throughout; the seams into the SDK/provider layer are now sound**

Dimension 5's most severe prior finding (provider catalog consulted *before*
the canonical effect table, letting one installed extension silently decline
or replace vanilla fragment behavior) is fixed by reordering
`classify_effect_with_providers` to try `classify_effect` first. The dispatch-
time O(N²) recursion in `Effect::Conditional` handling is fixed by an
iterative borrowed-slice cursor stack. Every other decline point re-verified
this pass (guard `?`-propagation, the two narrowed control-flow exceptions,
hole-binding resolution, the provider-barrier/no-callback consistency) holds.

## Coverage gap — SDK / extender-compatibility layer (reported, NOT audited, unchanged)

Per the skill's scoping note and the 09-06 pass's own assessment, this ~14k+
LOC layer (`crates/sdk`, `papyrus_provider/`, `obscript.rs`,
`obscript_runtime.rs`, `compatibility.rs`) still needs its own audit pass
designed with Dim-8-level rigor. This pass again only audited the seams
(Dims 5–7) — all of which are now sound, which removes the strongest prior
argument for urgency but does not substitute for a dedicated pass over the
untrusted `SCDA` decode discipline, the provider front-end/back-end typing
contract, and the `Access` declarations inside that layer itself.

## Decompiler Soundness Matrix (Dims 1–4)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes |
| `call_sites.rs` preflight | Yes | **Yes — now O(D), fixed (#3938→2d1ad834/#3934 chain)** | Yes — now inside the widened panic net | Yes (adversarial-shape regression test) |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | **Yes — depth now memoized on `Node`, fold-time check reads real cumulative depth** | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH`, now derived not restated) | **Yes — `combine`'s result depth-checked before being pushed back, closing the prior bypass** | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | **Yes — whole-body re-fold now reads memoized depth, no longer resets to 1** | Yes; `\|\|`-skip fails closed (#1732), doc-comment on that branch still stale (LOW, carried forward) |
| Lower (`lower.rs`) | Yes | Yes | **Yes — the SIGABRT class is closed; `lower_expr`'s recursion is now bounded transitively by the fixed upstream caps** | Yes for straight-line/property/event/if shapes, plus two new cross-block/cross-pass depth-cap regressions |
| `.psc` lexer + parser | Yes | Yes | Yes (`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH`, now `pub` and cross-asserted equal by a runtime test) | Yes (96 tests, +5 since last pass) |

One residual observation (new, LOW, Dim 2): `MAX_EXPR_DEPTH` (256, expression-
fold depth) and `MAX_REBUILD_DEPTH` (1024, control-flow nesting depth) are
independent caps that both feed the same recursive tree walks; they compose
additively (~1,280 worst case) with a comfortable margin against the measured
abort threshold (~10,000 on the smallest realistic stack) today, but neither
docstring acknowledges the other, so a future increase to either constant
alone could erode that margin unnoticed — see SCR-D2-2026-09-11-01 below.

### The two documented Champollion departures — unchanged, still correctly adjudicated

Both re-confirmed this pass with no change: the `boolean.rs` no-debug-line
guard remains benign (the one reachable false-merge shape is semantically
identical for a `Bool`), and the `control_flow.rs` `||`-skip remains
correct-and-fail-closed (#1732) — only the module-level doc comment
describing it as "advanced past" instead of "declined" is still stale
(SCR-D3-2026-09-11-01, LOW, carried forward unfixed across three fix cycles).

## Decline-Invariant Audit (Dim 5)

| Decline point | Verdict |
|---|---|
| Chain order `two_state_activator → rumble → quest_stage_gate`, `find_map` first-match | Conservative, unchanged |
| `classify_effect_with_providers`: canonical table first, provider catalog only for unclaimed statements | **Fixed this cycle** (was: provider-first, HIGH) |
| `split_and` leaving `\|\|` whole; per-atom `classify_guard_atom(..)?` | Conservative, unchanged |
| `lower_statements`: `bind_local` three-map classification, `Return(None)` inert, `_ => None` | Conservative, unchanged |
| Narrow `While`/`If` control-flow exceptions (`lower_3d_loaded_wait`, `Effect::Conditional`) | Conservative, unchanged; `MAX_CONDITIONAL_DEPTH` now a derived const, not a hand-copied literal |
| `Effect::Conditional` dispatch — unresolvable-guard decline | Conservative, unchanged (`resolved` flag + `continue`) |
| `Effect::Conditional` dispatch — recursion bound | **Fixed this cycle** — iterative borrowed-slice cursor stack replaces `branch ++ tail` recursion (was O(N²), HIGH) |
| `recognize_specific_actor_trigger`'s optional-property reads | **Fixed this cycle** — three-case `Option<Option<T>>` closures replace the two-case collapse (was MEDIUM) |
| `PapyrusProviderRuntime` default/no-callback vs. non-empty catalog | **Fixed this cycle** — `servable_catalog()` gates on `callback.is_some()` (was MEDIUM) |
| `AddItem`/`MoveTo` shape acceptance | **Fixed this cycle** — `MoveTo` now accepts the compiler-materialized default-arg shape (closes long-open #3487) |
| `SetStage`/`SetObjective*` arity | **Fixed this cycle** — all four bounded, plus a mechanized completeness scan against future primitives (#3496) |
| Lock/SetLockLevel | **Added this cycle** — closes #3159, kept as two separate effects by design |
| Hole binding (`QuestRef`/`ObjectRef` three-map resolution, alias-bound `SceneActorBindings::resolve`) | Conservative, unchanged |
| `translate_pex*` panic net | **Widened this cycle** — now covers the whole sequence for all three entry variants, not just `decompile_script` |
| `CanonicalEvent::from_papyrus` | **Fixed this cycle** — now has a live production caller (`quest_stage_gate.rs`), doc corrected |

No decline-invariant violation was found anywhere in this pass. This is the
first scripting audit cycle in the report series with zero new Dim-5 findings.

## Runtime Lifecycle Invariant Matrix (Dim 6)

| Invariant | Verdict |
|---|---|
| Two-phase lock-drop — `timer_tick_system`/`recurring_update_tick_system`/`trigger_detection_system` | Holds |
| `QuestStageFragments`/`SceneFragments` cloned before locks | Holds |
| Guard-free contract (`apply_effects` never under a live quest-resource guard) | Holds at all four production sites |
| SCEN package `Activate` marker ordering | **Fixed this cycle** — routed through `PendingFragmentActivations`, flushed head-of-frame before all four consumers (was HIGH) |
| `MAX_PROVIDER_FRAGMENT_BARRIERS` cap-then-partial-apply | **Fixed this cycle** — cap now gates before descending, not after partial application (was the merged LOW) |
| Marker drain coverage (Pattern A / Pattern B) | Holds; module doc now explicitly enumerates both with a non-overlap regression test |
| CTDA OR-precedence + empty ⇒ true | Holds |
| Safe-default sentinels; `RunOn` declines | Holds |
| Edge-trigger seed; multi-triggerer append; `intersects_sphere` tethered-horse-only; occupancy retain-prune | Holds |
| `actor_quest_trigger_is_in_sequence` | Holds; now shares its two predicates with the cinematic router (closes the Dim-8 router/gate divergence as a side effect) |
| `QuestAliasReadinessGate` three guards | Holds |
| `ScriptRegistry` no live hardcoded attach | Holds |
| `apply_effect` doc-comment accuracy | **Fixed this cycle** — rewritten with a pinning regression test so a future helper insertion can't silently detach it again |
| `OnEquipEvent` doc rot | **Fixed this cycle** — all three ground-truth docs corrected |
| `papyrus_provider_system`/`legacy_obscript_load_order_system` `Access` declarations | **Fixed this cycle** — mechanically enforced by a source-scan test, not just prose |
| `QuestStageState` dynamic-subscription model (#3892) | **Fixed/settled this cycle** — dead dynamic model deleted; three compile-time subscriber-id constants are now the sole model |

One new LOW: the regression test added to close the SCEN-package-`Activate`
HIGH pins three of the four real `ActivateEvent` consumers, omitting
`mg07_on_activate_dispatch` — today's registration order is correct, but the
gap means a future reorder could silently reintroduce a one-consumer version
of the same defect undetected (SCR-D6-2026-09-11-01).

## Havok / Cinematic Slice (Dim 8)

| Invariant | Verdict |
|---|---|
| `crates/hkx` untrusted-input discipline | Holds; zero commits to the crate since 08-30 |
| No behavior-graph execution | Holds |
| Spline static/dynamic split, track-count mismatch, knot monotonicity | Holds |
| Z-up→Y-up exactly once; deterministic candidate order | Holds |
| Once-per-serial playback (request retained, not drained — framing confirmed accurate); apply-then-drain root motion | Holds |
| Unknown annotation ignored safely; can't deadlock a quest stage | Holds |
| `cinematic_retained_entities`/`strip_retained_cell_root` scoping | Holds; one adjacent commit (#3299) checked for interaction, none found |
| `SceneRegistry` lock-order inversion (`scene_trigger_actor_approach_system_inner`) | **Fixed this cycle** — `drop(registry)` restored before the quest-resource acquisitions; regression test is a source-scan asserting `acquire < release < advances` (was HIGH) |
| Router ↔ gate agreement (cross-quest `GetStageDone` filter; centerless-trigger handling) | **Fixed this cycle** — both predicates extracted into shared functions the router and gate now both call (was LOW ×2) |
| Stale doc attached to scratch struct instead of the system fn | **Fixed this cycle** |
| `HorseTetherState`/`ActorCinematicState` lifetime (#3817) | **Confirmed still OPEN** — no removal site exists anywhere in the tree; correctly cited, not re-filed |

Zero new findings for this dimension — every prior finding closed, nothing new.

## Findings

**One MEDIUM, three LOW** (4 total, all NEW this cycle — no CRITICAL, no
HIGH). Every finding from the 2026-09-06 report is verified fixed except
`#3817` (confirmed still open, correctly not re-filed) and the two doc-rot/
minor observations noted inline above where a fix landed for the mechanism
but not the accompanying prose (SCR-D3-2026-09-11-01 carries forward the
`control_flow.rs` doc-comment item verbatim from 09-06, still unfixed after
three intervening fix cycles that touched adjacent lines).

---

### MEDIUM

#### SCR-D7-2026-09-11-01: Persistent-worldspace logical-actor stubs never route through the script-attach path — a persistent NPC's own base-record SCRI/VMAD or REFR-own VMAD never attaches while the actor is stub-only

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/exterior.rs:318-345`
  (`PersistentCellApplyJob::apply`'s logical-stub loop), `:366-408`
  (`prepare_logical_actor_stubs`), `:1138-1146` (`begin_worldspace_persistent_cell`)
- **Status**: NEW (searched `/tmp/audit/scripting/issues.json` and live
  `gh issue list --search` for "remote actor script" / "persistent actor
  script" / "logical stub script" — no match, open or closed. The adjacent
  #2664, CLOSED, fixed this same stub's missing `Transform`/`GlobalTransform`
  and its `stamp_quest_reference` duplication, but is scoped strictly to
  identity/alias-ranking and never touches script attachment.)
- **Description**: `PersistentCellApplyJob` builds `logical_stub_refs` from
  `remote_actor_refs` (persistent-CELL actor placements outside the initial
  streaming radius) plus the subset of `local_refs` that are actors but
  didn't spawn as a "live" `SceneAliasCandidate`. Every entry is materialized
  with exactly one call — `spawn_logical_quest_reference` — which stamps
  `Transform`/`GlobalTransform`/`SceneAliasCandidate` and nothing else. There
  is no call to `attach_script_for_refr`/`attach_quest_reference_script`
  anywhere in `exterior.rs` (confirmed by grepping the whole file for
  `"script"`). Contrast with the ordinary actor path in
  `references/mod.rs:719-729`, which always follows `stamp_quest_reference`
  with `attach_quest_reference_script` for the same "no full spawn root"
  shape. This is a whole code path that never reaches the attach function at
  all, not a per-lookup decline — it produces zero log output naming the
  decision, so it's invisible to the "M47.2 scripts:" counter and to the
  path's own one summary line.
- **Evidence**:
  ```rust
  // exterior.rs:318-345
  while self.next_logical_stub < self.logical_stub_refs.len() {
      let placed = &self.logical_stub_refs[self.next_logical_stub];
      super::references::spawn_logical_quest_reference(
          world, placed, &wctx.load_order,
          super::transition::position_zup_to_yup(placed.position),
          super::transition::rotation_zup_to_yup_quat(placed.rotation),
          placed.scale,
      );
      self.next_logical_stub += 1;
      budget.complete_unit();
  }
  ```
- **Impact**: Any vanilla persistent quest actor (Bethesda marks an NPC/CREA
  "Persistent" specifically so it survives outside normal cell streaming —
  quest-critical companions, ambush spawners, radiant-quest targets, actors
  referenced by a quest alias from a remote worldspace location) that also
  carries its own scripted behavior has that behavior silently absent for
  the entire time the actor is only a logical stub — which for an actor
  outside every session's streaming radius, or whose home cell never
  streams in, is effectively its whole lifetime. Blast radius is every
  ObScript/Papyrus game with worldspace-persistent-cell content whose
  persistent actors carry their own scripts; the same population #2664
  already identified for the identity/alias-ranking half.
- **Disproof attempted**: grepped the whole of `exterior.rs` for any
  `"script"` reference (only `mark_scene_actor_bindings_dirty` and the
  `SceneAliasCandidate` type name appear); confirmed no other call site in
  the engine stamps identity without also attaching scripts for this "no
  full spawn root" shape.
- **Related**: Sibling of the now-closed #2664 (identity/alias-ranking half
  of this same stub); same "recognized architecture, missing wiring at one
  call site" shape as the now-fixed SCR-D7-2026-09-06-01 (statics-family
  `SCRI` gap), three sessions apart.
- **Suggested Fix**: after `spawn_logical_quest_reference` in the
  `logical_stub_refs` loop, call `attach_quest_reference_script` (or
  `attach_script_for_refr` directly) with `placed.base_form_id` and
  `placed.script_instance.as_ref()`, incrementing a counter surfaced in the
  existing summary line so a fix here is observable without a game-data run.

---

### LOW

#### SCR-D2-2026-09-11-01: `MAX_EXPR_DEPTH` (256) and `MAX_REBUILD_DEPTH` (1024) are independent, uncorrelated caps that both feed the same recursive tree walks — currently safe by a comfortable margin, but the margin isn't asserted anywhere

- **Severity**: LOW (measured non-exploitable today; hygiene/hardening gap)
- **Dimension**: Decompiler CFG Construction & Opcode→Node Lift
- **Untrusted-Input**: Yes (bound, not unbounded)
- **Location**: `crates/pex/src/decompile/control_flow.rs:44`
  (`MAX_REBUILD_DEPTH = 1024`); `crates/pex/src/decompile/lift.rs:380`
  (`MAX_EXPR_DEPTH = 256`); `crates/pex/src/decompile/node.rs:379-384`
  (`Node::is_final` — an `IfElse`/`While` node is always "final" and never
  subject to the expression fold-time depth check on its own construction)
- **Status**: NEW (not a regression of #3933 — this composition existed
  identically before the fix; #3933 changed *how* expression depth is
  tracked, not whether `If`/`While` nesting depth shares a budget with it)
- **Description**: nothing checks an `If`/`While` node's nesting depth
  against `MAX_EXPR_DEPTH` (or any cap) at construction time; the only cap
  on `If`/`While` nesting is `MAX_REBUILD_DEPTH`, a call-stack recursion cap
  on `Reconstructor::rebuild` completely independent of `MAX_EXPR_DEPTH`. The
  actual worst-case tree depth reaching `Node`'s unbounded-recursion `Clone`,
  `count_constant_id`/`replace_constant_id`, and `lower_expr` is bounded by
  `MAX_REBUILD_DEPTH + MAX_EXPR_DEPTH ≈ 1024 + 256 = 1280`, not by either cap
  alone — comfortably below the measured SIGABRT threshold (~10,000 on the
  smallest realistic 2MB worker stack, ~30,000 on an 8MB main thread) but
  with a ~7.8x margin instead of an asserted one.
- **Impact**: none today; latent coupling — raising `MAX_REBUILD_DEPTH` alone
  in the future (e.g. for deeply-nested Starfield/FO4 bodies) without
  revisiting `MAX_EXPR_DEPTH`'s adjacency could erode the margin unnoticed.
- **Disproof attempted**: confirmed `MAX_REBUILD_DEPTH` still declines nesting
  past 1024 cleanly before any tree is built; confirmed the expression cap
  does still apply when something folds *into* an `If`/`While` node (the gap
  is specifically nesting-depth-compounding-with-expression-depth, not either
  axis alone); confirmed via arithmetic that the composed worst case is real.
- **Related**: #3933 (CLOSED, orthogonal — does not regress it);
  SCR-D4-2026-09-06-02 (CLOSED via #3945 — same "unenforced alignment between
  independently-declared depth constants" class, one level up)
- **Suggested Fix**: either add a `const _: () = assert!(MAX_REBUILD_DEPTH +
  MAX_EXPR_DEPTH <= SOME_DOCUMENTED_SAFE_BOUND);` tying the two, or give
  `lower_expr`/the `Node` tree walks a defensive depth counter that errors
  past a fixed absolute ceiling regardless of which upstream cap let the
  tree grow.

#### SCR-D3-2026-09-11-01: `control_flow.rs`'s module doc still says the residual `||`-shape branch is "advanced past" — three fix cycles after the 2026-09-06 report flagged this exact sentence as stale, it is still unfixed

- **Severity**: LOW (documentation only; the code path itself is correct
  and tested — `Err(self.fail())`, not a fallthrough)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: No (doc-only)
- **Location**: `crates/pex/src/decompile/control_flow.rs:27-29`
- **Status**: CARRIED FORWARD — the 09-06 report's re-adjudication table
  named this exact line ("Correct and fail-closed … the module-level
  sentence at `:27-29` still says 'advanced past' — stale prose only") and it
  was not touched by `f1a9b984` or `5d0d96ef`, the two follow-up commits that
  otherwise cleared every other stale-prose item this pass re-checked.
- **Description**: the doc comment reads "…only fires for the `||`-shapes
  the boolean pass declined to collapse — see SCR-D3-01/#1732 for that tail"
  and describes the branch as "advanced past." The actual arm (`:210-220`)
  is `return Err(self.fail())` — a hard decline, not an advance. The bare
  `SCR-D3-01` citation is also now ambiguous against this report series'
  dated finding-ID convention.
- **Impact**: none functional; doc-rot risk only — a reader trusting the
  comment over the code would mis-describe the failure mode when extending
  this branch.
- **Disproof attempted**: re-read `:210-220` directly (confirmed `Err`, not
  fallthrough); `git log -p --follow` on this hunk shows no edit since at
  least `88e7dbfc`.
- **Related**: 09-06 report, "The two documented Champollion departures"
  table, row 2; #1732
- **Suggested Fix**: reword to "declined, not advanced past" and drop the
  bare `SCR-D3-01` shorthand in favor of the issue number (`#1732`) only.

#### SCR-D6-2026-09-11-01: the pinned `ActivateEvent` consumer-order test omits the fourth real consumer, `mg07_on_activate_dispatch`

- **Severity**: LOW (test-coverage gap only; today's registration order is
  correct — this is exactly the invariant that regressed once already, as
  SCR-D6-2026-09-06-01, without any test catching it for 187 commits)
- **Dimension**: Scripting Runtime Systems — Lifecycle, Stage & Lock Ordering
- **Untrusted-Input**: No
- **Location**: `byroredux/src/boot/schedule/mod.rs:75-79` (the consumer
  list in `activation_flush_is_scheduled_before_every_activate_event_consumer`);
  `byroredux/src/boot/schedule/update.rs:75-77`
  (`mg07_on_activate_dispatch` fn), `:315` (registration)
- **Status**: NEW
- **Description**: the regression test added to close SCR-D6-2026-09-06-01
  asserts `fragment_activation_flush_system` runs before three named
  `ActivateEvent` consumers (`rumble_on_activate_dispatch`,
  `quest_advance_dispatch`, `two_state_activator_system`). The prior HIGH
  named a *fourth* real consumer, `mg07_on_activate_dispatch`
  (confirmed reading `ActivateEvent` via the standard two-phase
  collect/drop pattern), registered at `update.rs:315` — after the flush at
  `:172`, so today's order is correct — but the test doesn't cover it. A
  future reorder (the exact edit class the `#3739`/`#3855` boot-splitting
  refactors already warn about) could silently reintroduce a
  one-consumer version of the original HIGH, undetected.
- **Evidence**: `grep -n "mg07_on_activate_dispatch"
  byroredux/src/boot/schedule/mod.rs` → zero matches inside the pinned test.
- **Impact**: none today; latent regression risk scoped to exactly the
  invariant this crate has already been burned by once.
- **Disproof attempted**: confirmed `mg07_on_activate_dispatch` reads
  `ActivateEvent` (not some other marker); confirmed no other, broader test
  covers this specific ordering relation.
- **Related**: SCR-D6-2026-09-06-01 (the HIGH this test was added to guard,
  CLOSED); `#3936` (the fix commit)
- **Suggested Fix**: add the `mg07_on_activate_dispatch` registration-site
  needle to the consumer array at `mod.rs:75-79`.

---

## Future-Phase Readiness

- **Obscript / `SCTX` (M47.2 Phase 5)** — still unbuilt; unchanged from 09-06.
  This cycle's fixes reinforce the same lessons the 09-06 pass drew for a
  future third frontend: the untrusted-input contract must be
  recursion-bounded as a property of the *tree* (the `Node::depth`
  memoization pattern, not a per-call ledger), time-bounded (the
  `call_sites.rs` O(D) rework), and the decline-on-unmodeled rule must hold
  at every consumer including a provider/extension seam (the
  primitives-first reorder).
- **The fragment lowerer (b2)** — fully wired and live-verified; every open
  soundness item from 09-06 (the provider seam, the sequential-Conditional
  recursion, the cap-then-partial-apply shape) is now closed. No new
  soundness gap identified this cycle.
- **M47.1 condition resolvers** — unchanged: all 13 catalog functions remain
  implemented with correct safe-default sentinels; live-headless-cell
  re-verification against real CTDA data remains outstanding (not attempted
  this cycle either — no engine launch).
- **M47.3 Phase 4+** — unchanged: Created Object alias spawn, Story Manager
  event fills, true `LCTN` traversal, reference-collection aliases,
  unloaded-world Find-Matching search, and the injected overlay families
  remain parsed-and-exposed rather than applied. Documented, not silent.
- **Persistent-actor script attachment** (new this cycle, SCR-D7-2026-09-11-01) —
  the logical-stub path needs the same `attach_quest_reference_script` call
  every other actor-identity-stamping site already makes; this is a small,
  well-scoped fix, not a design gap.
- **The SDK / extender layer** — still needs its own dedicated audit pass.
  This cycle found the domain's seams into it (Dims 5–7) fully sound, which
  removes the most urgent argument for scheduling that pass immediately, but
  the untrusted `SCDA` decode discipline and the provider typing contract
  inside the layer itself remain unaudited.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 3 |
| **Total** | **4** |

Dimensions producing new findings: 2 (1 LOW), 3 (1 LOW), 6 (1 LOW), 7 (1
MEDIUM). Dimensions producing zero new findings: 1, 4, 5, 8 — the first time
in this report series that four of eight dimensions closed with a completely
clean sheet. All nine findings (5 HIGH-or-MEDIUM, 4 LOW-or-lower, counting
the merged item) from the 2026-09-06 report are verified fixed except #3817
(confirmed still open, correctly not re-filed).

---
*Generated by the `/audit-scripting` skill. Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-09-11.md`
