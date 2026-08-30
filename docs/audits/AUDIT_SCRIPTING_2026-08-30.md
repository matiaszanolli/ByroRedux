# Scripting Subsystem Audit — 2026-08-30

Sixteenth full pass over the M30/M47 Papyrus / `.pex` / ECS scripting domain
(prior reports: `AUDIT_SCRIPTING_2026-06-23.md` … `_08-27.md`). Run
**single-agent, in-process, no sub-agent fan-out** — every dimension executed
directly and written to `/tmp/audit/scripting/dim_N.md` before consolidation,
per this run's explicit instruction (a prior suite run lost dimensions to
un-retrievable sub-agent results).

**Scope**: `crates/pex`, `crates/papyrus`, `crates/scripting` (owner crates),
plus `crates/hkx` (Dim 8 — folded in, and **actually covered**: full read of
`packfile.rs` and `animation.rs`'s decode half) and the engine-side attach /
cinematic / cell-loader wiring (Dim 7).

**Method**: source reads, greps, `cargo check`/`cargo test` scoped per package,
git archaeology against the previous report's commit, and **one new empirical
probe** — a hand-built in-memory `Pex` driven through `decompile_script` at
increasing chained-temp depths, which produced this pass's HIGH.
No engine launch, no game-data corpus run (memory-constrained session).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 400
--state open` (saved to `/tmp/audit/scripting/issues.json`, cleaned up per
Phase 4), `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`, and
`git log 0262f716..HEAD` over every path in scope.

## What changed since 2026-08-27

`crates/pex`, `crates/papyrus` and `crates/hkx` have **no source change at all**
in range. Five `crates/scripting` files changed, all remediation of
previously-filed findings:

| Commit | Effect on this domain |
|---|---|
| `26f8738d` | Fix #3277 — `push_quest_stage_advances` makes the shared `QuestStageAdvancedBatch` merge invariant structural; all six producers routed through it. Fix #3278 (partial) — `Effect::Disable`'s receiver is now alias-aware |
| `265f0c9b` | Fix #3278 — `ReferenceEnableState` finally gets a runtime consumer in `cell_loader/spawn.rs::placement_is_disabled` |
| `b28acb0c` | Fix #3441 — breaks the `ActorValues`/`CharacterRuleset` lock cycle in `condition.rs`'s `GetActorValue` arm |
| `a1327227`, `9c5bea16`, `aa16a936`, … | cell-loader / exterior-streaming refactors, incidental |

All three in-domain fixes verified present, correctly shaped, and guarded by
new regression tests.

## Build & test state — CLEAN

```
$ export CARGO_BUILD_JOBS=4
$ cargo check -p byroredux-scripting -p byroredux-pex -p byroredux-papyrus -p byroredux-hkx --all-targets
    Finished in 3.30s

$ cargo test -p byroredux-pex -p byroredux-papyrus     # 90 + 56 + 4 + 1 passed, 1 ignored
$ cargo test -p byroredux-scripting -p byroredux-hkx   # 334 + 18 passed, 4 ignored (game-data-gated)
0 failed.
```

## Headline verdicts

### Untrusted-input robustness — **NO PANIC, NO OOB, NO OOM … but YES, A PROCESS ABORT**

This verdict changes this cycle, and it is the reason for the pass's one HIGH.

A `.pex` that is entirely **well-formed** — valid magic, valid opcodes, valid
operands, every count inside the wire format's own `u16` ceilings — can drive
the decompiler to a **stack overflow**, which is a `SIGABRT`, not a panic.
`translate_pex`'s `catch_unwind` (#1816/#3287) and
`populate_quest_fragments_from_pex`'s `Err` degradation are both bypassed: there
is no unwinding to catch. Reproduced at N = 40 000 chained folds in **both**
debug and release on the 8 MB main thread. See SCR-D2-2026-08-30-01.

Everything else in the untrusted-input surface is sound: the `.pex` reader, the
CFG builder, the lift/copy-prop pass, the `.psc` lexer + Pratt parser, and the
whole of `crates/hkx` are bounds-safe, allocation-bounded and total.

### The 99.996% decompile-rate claim — **measures what it claims**

`crates/pex/examples/pex_corpus_smoke.rs` calls `decompile_script` inside a
`catch_unwind` and tallies `decompiled_ok` / `decompiled_err` /
`decompiled_panic` as three **separate** buckets; the percentage is
`ok / (ok + err + panic)`. A panic is not counted as a success. #3017's
item-count cross-check (predicting the item count from `decompile_script`'s own
documented production rule) is wired alongside it. The claim is a **robustness**
measure and not a fidelity one — which the harness doc and `boolean.rs`'s
departure note both state plainly.

### The `.psc`-vs-`.pex` fidelity gate — **both halves pin equality**

`recognizes_da10_and_reproduces_hand_builder`
(`translate/recognizers/quest_stage_gate.rs:428`, runs unconditionally) and
`da10_pex_reproduces_hand_builder_byte_for_byte`
(`crates/scripting/tests/pex_recognize_e2e.rs:81`, `#[ignore]`-gated on Skyrim
SE data) both assert field-by-field equality against the same
`da10_main_door(...)` hand builder. The `.pex` half does not run in CI — a known
and documented limitation, not re-filed.

## Decompiler Soundness Matrix (Dims 1–4)

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 linear chain) | Yes (#2666 fail-closed) | Yes |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH = 1024`) | **No — aborts on a deep node tree (SCR-D2-2026-08-30-01)** | Partly |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap | Yes (#1732 fail-closed) | Partly |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes for straight-line/property/event shape |
| `.psc` lexer + parser | Yes | Yes | Yes (`MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH = 256`) | Yes |

Re-verified directly this pass (no source changed since 08-19, but nothing was
taken on trust):
- `MAX_OPCODE = 51` with `#[repr(u8)]` discriminants contiguous `0..=50` (full
  enum read, no gaps) and the `transmute` guarded `byte >= MAX_OPCODE`.
  `from_u8_round_trips_and_rejects_oob` iterates the **whole** valid range;
  `metadata_matches_champollion_full_table` (#2127) pins all 51 `OPCODES` rows
  against an independently transcribed literal.
- Every operand index in `create_node` is `< arg_count()` for its opcode
  (checked arm by arm), so the direct `a[n]` indexing cannot go OOB.
- The var-arg vec is still `Vec::new()` + `push` (#1710); every other
  `with_capacity` in the reader is fed by a `u16` count.
- `MAX_REBUILD_DEPTH = 1024` in **both** `control_flow.rs:37` and
  `boolean.rs:56`, each with its own `pass` label (#2667).
- `MAX_EXPR_DEPTH` / `MAX_STMT_DEPTH = 256`, both with balanced
  increment/decrement across the error path, and no bypass of the gated entry
  point (all 15 expression entries funnel through `parse_expr()`).
- `EVENT_NAMES`: **267 entries, strictly sorted, all lowercase, zero
  duplicates**, every high-frequency recognizer-relevant event present
  (programmatically re-checked this pass).
- All **47** `.psc` keyword tokens carry `ignore(ascii_case)`; the `Ident`
  regex is `priority = 1` beneath them.

### The two documented Champollion departures — adjudicated

| Departure | Verdict |
|---|---|
| `boolean.rs`: **no debug-line guard**, relying on the structural signal alone | **Benign as documented.** The compensating requirement (`collapse` demands the operand block fall through to the rejoin, #2655) is present, and the module doc is already honest that the corpus decompile *rate* does not validate the departure — the smoke harness discards the `Script`, and the R5 fidelity gate is one `#[ignore]`d script. No new finding; the residual risk is the same known coverage gap. |
| `control_flow.rs`: the deliberate `\|\|`-skip | **Correct and fail-closed.** The `last.is_conditional()` arm is `return Err(self.fail())` (#1732), so a script reaching it declines cleanly through `translate_pex` rather than emitting a truncated AST. |

## Decline-Invariant Audit

| Decline point | Verdict |
|---|---|
| `classify_if_condition`'s per-atom `classify_guard_atom(atom, player_param)?` | Conservative — a real `?`, not a dropped `Option` |
| `split_and` leaving `\|\|` whole | Conservative and intentional; a disjunction is one atom no primitive claims |
| #1905 mixed-quest cross-check | Conservative — declines rather than retargeting |
| `lower_statements`'s `_ => return None` | Conservative; the two narrowed exceptions (`Stmt::While` via `lower_3d_loaded_wait`; `Stmt::If` via `Effect::Conditional`) are still exactly as narrow as documented |
| `receiver_object`'s explicit `key == "self"` + local declines | Conservative (see the skill-drift note below on which locals now bind) |
| `prim_set_objective_*`'s `i32::try_from(..).ok()?` | Conservative — declines out-of-range instead of truncating; a genuine fix, not a loosened range check |
| `ScriptInstance::object_form_id`'s `alias == -1` requirement (#2186) | Conservative at the single shared accessor |
| `translate_pex` on bad bytes / decompiler `Err` / decompiler panic | Clean `None` on all three, all three test-guarded |
| **`Effect::Conditional`'s guard resolution at dispatch** | **LEAKS — an unresolvable guard quest selects the `else` branch and runs it. See SCR-D5-2026-08-30-01** |

## Runtime Lifecycle Invariant Matrix

| Invariant | Verdict |
|---|---|
| Marker drain coverage | **Complete.** All 46 `impl Component for` types enumerated: 16 Pattern-A (drained in `event_cleanup_system`), 10 Pattern-B (drained at the head of their one owning consumer), 20 persistent state. **No marker in neither bucket.** Both contract tests present |
| `event_cleanup_system` is last | Yes — `boot.rs:1520`, the highest `Stage::Late` registration |
| Two-phase lock drop | `timer.rs:48` and `recurring_update.rs:168` use explicit `drop()`; `trigger_detection_system` uses an equivalent lexical block (`trigger.rs:156-175`); `quest_fragment_dispatch_system` still clones `QuestStageFragments` before the two mutable resource acquires |
| Scheduler ordering | Unchanged: `trigger_detection` → `quest_advance` → `quest_startup` → `quest_alias_refresh` → `quest_alias_readiness` → `scene_playback` → `scene_fragment_dispatch` → … → `quest_fragment_dispatch`; `event_cleanup` last. All `add_exclusive`, so the nested component-lock-under-resource-lock shape in `apply_effect` cannot ABBA against a concurrent system |
| Cascade bound | `VecDeque` FIFO; `cascade_steps` incremented **only** for `is_cascade == true`; WARN past `MAX_CASCADE = 64`; only genuine transitions re-queue (#2124's correct `adv.previous_stage != adv.new_stage`) |
| Same-frame quest-advance sink (#3277) | Structural — all six producers route through `push_quest_stage_advances`; no bare `insert` survives |
| Edge-trigger seed (#1817) | `occupant_inside: Option<bool>`; the `None` branch never pushes to `entered` |
| Multi-triggerer delivery | `triggerers: Vec<EntityId>`; **no consumer indexes `[0]`** — `quest_advance_system` fans out one `(entity, triggerer)` pair per element |
| `TriggerOccupancyState` growth | Bounded — `retain(|key, _| observed.contains(key))` prunes each pass |
| `QuestAliasReadinessGate` | All three guards present (`is_running`, `< only_below_stage`, `!get_stage_done`), with `drop(bindings)`/`drop(registry)` before the write acquire |
| CTDA OR-precedence | Correct, including the trailing-`or_next` clamp against a truncated CTDA tail |
| `ScriptRegistry` hardcoded attach | Fully retired — `register_spawners` survives only in comments and one test doc |
| **Cinematic retention lifetime** | **Unbounded for the `HorseTetherState` half. See SCR-D8-2026-08-30-01** |

## Findings

Three new findings: **1 HIGH, 2 MEDIUM, 1 LOW** (4 total). No CRITICAL.

---

### HIGH

#### SCR-D2-2026-08-30-01: a well-formed `.pex` within the wire format's own `u16` ceiling aborts the process by stack overflow — `Node`'s derived recursive `Clone` is unbounded, and the boolean pass deep-clones every block scope unconditionally

- **Dimension**: Decompiler CFG & Lift (the defect is in the shared `Node` tree; the first trigger is the boolean pass)
- **Untrusted-Input**: **Yes**
- **Severity**: HIGH — the domain table rates "stack overflow via unbounded recursion in a decompiler tree walk" HIGH, and a stack overflow is strictly worse than a panic: it is a `SIGABRT` that `catch_unwind` **cannot** intercept, so #1816's panic guard is bypassed entirely
- **Files**: `crates/pex/src/decompile/boolean.rs:158` (trigger), `crates/pex/src/decompile/node.rs` (the unbounded `Clone`/drop glue), `crates/pex/src/decompile/lift.rs::rebuild_expression` (what builds the deep tree)

**Mechanism.** `rebuild_expression`'s copy-propagation nests each folded producer
inside its consumer, so a chain of N temp-producing instructions
(`::temp0 = a + b; ::temp1 = ::temp0 + b; …`) collapses into a single `Node`
expression tree of depth N. Nothing caps N except the wire format: a function's
instruction count is a `u16` (max 65535), and the string table (also `u16`) has
room for the ~40 000 distinct `::tempN` identifiers such a chain needs.

`Node` derives `Clone` and its children are `Box<Node>`, so `Node::clone` (and
the drop glue) recurse once per tree level with no cap. The first site to hit it
is `BoolPass::rebuild`:

```rust
// crates/pex/src/decompile/boolean.rs:158
let scope = self.scopes.get(&current).cloned().unwrap_or_default();
```

That deep-clones the **entire** node scope of **every** block on **every**
visit — before the `block.is_conditional() && !scope.is_empty()` test that is
the only reason the clone exists. A straight-line function with no conditional
blocks at all still pays the full deep copy, and blows the stack doing it.

**Reproduction** (this audit; `crates/pex` only, no game data). A temporary
example built a `Pex` in memory with one function of N chained `iadd`s into
`::tempK` plus a final `assign`, then called `decompile::decompile_script`.
Main thread, 8 MB stack:

| N | debug | release |
|---|---|---|
| 20 000 | OK | OK |
| 27 000 | OK | — |
| 30 000 | **abort** | — |
| 40 000 | abort | **abort** |
| 65 000 | abort | abort |

Phase isolation at N = 40 000 / 65 000:
- `build_cfg` + `lift_function` (including `rebuild_expression`,
  `count_constant_id`, `replace_constant_id`, and the deep tree's drop):
  **survives at 65 000** — those hand-written walks have small frames.
- `scopes.get(&0).cloned()` **alone**: OK at 30 000, **aborts at 40 000**. This
  isolates the failure to the derived `Clone`, not to any hand-written walk.
- The full pipeline aborts inside `rebuild_boolean_operators`, before its first
  progress print — consistent with line 158.

Exit status 134 (`SIGABRT`) with `fatal runtime error: stack overflow`. No
unwinding.

**Blast radius.** `.pex` bytes reach `decompile_script` from a user/mod-supplied
archive via `--scripts-bsa` → `ScriptProvider::extract_pex` → `translate_pex`
(`byroredux/src/asset_provider/script.rs:279`,
`cell_loader/references/attach.rs`) and via
`populate_quest_fragments_from_pex`. One hostile or corrupt `.pex` in a mod
archive kills the engine at cell load with no diagnosable error. No vanilla
script approaches 40 000 instructions in one function, so this is a
robustness/untrusted-input defect, not a compatibility one.

The 8 MB figure is the **main** thread. Rust's default for a `thread::Builder`
without an explicit `stack_size` is 2 MB, and no call site in this repo sets one
(`grep stack_size` finds only `streaming.rs` and test files, none of which set
it), so any future move of cell-load work onto a worker thread lowers the
threshold by roughly 4x — to ~10 000, still far inside the wire format.

**Suggested fix.** Two independent, cheap pieces:

1. **Cap the tree at its source.** Thread a nesting depth through
   `rebuild_expression`'s fold loop and return a new
   `DecompileError::ExpressionTooDeep` past a bound comfortably above real
   Papyrus (a few hundred; the `.psc` frontend's `MAX_EXPR_DEPTH` is 256, and
   this is the *same quantity* arriving through the other frontend, so matching
   it is defensible without guessing). This is the structural fix — it also
   protects `lower_expr`'s recursion and every downstream consumer that walks
   the emitted `ast::Expr`, including `translate/effects.rs`.
2. **Drop the gratuitous clone.** boolean.rs:158 should test
   `block.is_conditional()` (and the scope's emptiness / `last_result` shape)
   *before* cloning, or borrow rather than clone. Independently of the stack,
   it is a full deep copy of every block's expression trees on every visit, and
   once more per `reprocess` re-visit.

**Regression guards**: a `#[test]` building an N-chain (N a few thousand, under
the new cap) asserting a clean `Err`; and a `#[test]` asserting the boolean pass
does not clone a non-conditional block's scope.

---

### MEDIUM

#### SCR-D5-2026-08-30-01: an `Effect::Conditional` guard whose quest cannot be resolved does not decline — it silently selects the `else` branch and runs its effects

- **Dimension**: Recognizer-Chain Soundness (dispatch half)
- **Untrusted-Input**: No
- **File**: `crates/scripting/src/fragment.rs:1349-1357`

```rust
let passes = guards.iter().all(|guard| {
    resolve_quest_logged(&guard.quest, context, vmad)
        .is_some_and(|quest| stages.get_stage_done(quest, guard.stage) == guard.done)
});
let branch = if passes { then_effects } else { else_effects };
```

`is_some_and` collapses two distinct outcomes into one `false`: *"the guard was
evaluated and is false"* and *"the guard could not be evaluated at all"*. The
2026-08-24 pass checked this arm and correctly concluded it does not
wrong-default to `true`; the question nobody asked is what happens on the
`false` side. Because a `Conditional` has an `else` arm, `false` is **not
inert** — it runs code. So an unevaluable predicate executes the branch the
author reserved for the predicate being definitively false, which can be a
`SetStage`, `SetObjectiveCompleted`, `Disable`, or `SetGlobalValue`.

This is the decline-on-unmodeled invariant applied one layer later. Every
sibling site gets it right: `apply_quest_scoped_effect`'s
`resolve_quest_logged(quest, context, vmad)?` propagates `None` and the effect
is simply not applied; `resolve_object` / `resolve_actor` decline the same way.
`Effect::Conditional` is the one place where "cannot resolve" has a
*consequence*. The tell is in the log line itself —
`"fragment effect skipped: unresolved quest ref {via:?}"` is accurate for every
other caller and actively wrong for this one, where nothing is skipped and a
branch is chosen.

**How reachable**: `QuestRef::SelfRef` / `OwningQuest` always resolve to the
dispatch context, so the common intra-quest `GetStageDone(N)` guard is safe. The
exposure is `QuestRef::Property(name)`, which returns `None` when the named
property is absent from the quest's registered VMAD **or** when it is
alias-bound (`alias >= 0`, declined at `ScriptInstance::object_form_id` per
#2186). Correctly authored content should hit neither, so this is
latent-not-live — hence MEDIUM rather than the HIGH the severity table assigns
the recognizer-side analogue. It is filed rather than dropped because the
failure is silent, has no fallback, and `AUDIT_SCRIPTING_2026-08-27.md`'s
live-corpus histogram counts **871** `Conditional` effects across Skyrim + FO4 +
Starfield — the shape is not hypothetical.

**Suggested fix**: distinguish the third state.

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

and reword `resolve_quest_logged`'s message, or give the guard path its own
`log::warn!` (an unresolvable guard is a data defect worth surfacing, not a
routine skip). **Regression guard**: a `Conditional` with an unbound
`QuestRef::Property` guard and non-empty `else_effects` must apply **neither**
branch.

---

#### SCR-D8-2026-08-30-01: `HorseTetherState` has no removal site anywhere in the engine, so a tethered cart and horse are permanently exempted from cell unload and permanently stripped of `CellRoot`

- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: No
- **Files**: `crates/scripting/src/cinematic.rs:171-181` (component),
  `crates/scripting/src/fragment.rs:1033-1041` and
  `crates/scripting/src/cinematic.rs:560` (the two insert sites),
  `byroredux/src/cell_loader/unload.rs:20-25` (the retention consumer)

`cinematic_retained_entities` seeds its retained set from two sources. The
`ActorCinematicState` half **is** bounded: `Effect::ExitCart`
(`fragment.rs:1103`) sets `state.vehicle = None`, so the rider and cart fall out
of the retained set when the ride ends.

The `HorseTetherState` half is not. A repository-wide grep finds the component
**inserted** (the `Effect::TetherToHorse` arm, and `cinematic.rs:560`), **read**
(`unload.rs`, `trigger.rs:185`, `save_io.rs:730`), **registered** and
**save-serialized** (`save_io.rs:405`) — and **never removed**. There is no
`remove::<HorseTetherState>`, no despawn path that clears it, and `ExitCart`
does not touch it.

Consequences, all silent:
1. The cart, the horse, and everything transitively under their `Children`
   (render sub-meshes, bone hierarchies, colliders, lights) are excluded from
   `victims` for every subsequent cell unload, for the whole session. Their
   meshes/textures/colliders are never released.
2. They also have `CellRoot` stripped, so they belong to no cell and no ordinary
   teardown path can ever reach them — the retention is terminal, not
   "skipped this time".
3. `HorseTetherState` is save-registered, so the condition survives save/load.
4. `trigger_detection_system` keeps treating the horse as a tethered active
   mover forever — the widened `intersects_sphere` /
   `TETHERED_HORSE_TRIGGER_RADIUS` contact test and the
   `was_inside.is_none() && active_mover` first-observation exception apply to
   every newly-streamed volume, indefinitely.

The magnitude is bounded (vanilla authors one MQ101 cart), so this is a
permanent leak of a fixed set rather than unbounded growth — hence MEDIUM. It is
filed because the Dim 8 checklist asks exactly this question, the answer for one
of the two halves is no, and neither `AUDIT_SCRIPTING_2026-08-24.md` nor
`_08-27.md` (both of which examined `cinematic_retained_entities`) nor any open
issue covers it.

**Suggested fix**: give the tether a terminator. The natural site is the same
`Effect::ExitCart` arm that already clears `ActorCinematicState.vehicle` — or a
dedicated `Effect::UntetherFromHorse` if vanilla MQ101 signals the end of the
ride some other way. **Do not guess which**: read the authored `QF_MQ101_…`
fragment for the stage that ends the cart ride and mirror it. **Regression
guard**: a sibling to `active_tether_retains_horse_cart_rider_and_hierarchy`
asserting that after the untether those entities are back in `victims` and carry
`CellRoot` again.

---

### LOW

#### SCR-D3-2026-08-30-01: `decompile_script`'s auto-state match is the one case-SENSITIVE Papyrus identifier comparison in `lower.rs`

- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: Yes (both names come from the `.pex` string table)
- **File**: `crates/pex/src/decompile/lower.rs:415`

```rust
for state in &object.states {
    if state.name == object.auto_state_name {
```

Papyrus identifiers are case-insensitive, and every other identifier comparison
in this same file is case-insensitive: `parent_class_name
.eq_ignore_ascii_case("none")` (:437), `return_type_name
.eq_ignore_ascii_case("none")` (:251), `is_event_name` lowercasing before its
binary search, and `lower_expr` lowercasing the `true`/`false`/`none` identifier
literals. This one site uses `==`.

If a `.pex` ever carried `auto_state_name = "Waiting"` alongside a state named
`"waiting"`, the auto state would be emitted as a named
`ScriptItem::State { is_auto: false }` instead of having its callables hoisted
to script scope. `translate_script`'s recognizers walk script-scope handlers, so
every event handler in that object would become invisible and the script would
silently decline — the same failure mode as a missing `EVENT_NAMES` entry.

**Honest severity qualification**: defensive, not a live bug. Champollion
compares string-table *indices* here, which is stricter still, and a compiler
emitting the auto-state name twice with different casing has not been observed.
This pass did not run the 26k-file corpus to look for one, so there is no
corpus evidence either way. Filed LOW because the inconsistency is real, the
fix is one word, and the failure is silent if it ever occurs.

**Suggested fix**: `if state.name.eq_ignore_ascii_case(&object.auto_state_name)`,
plus a two-line unit test with a mismatched-casing auto state asserting the
handler lands at script scope.

---

## Stale candidates dropped

**Five** candidate findings were dropped after checking their premise against
current code:

1. *"The reader's `with_capacity` calls can be driven to an OOM by a hostile
   count."* — Dropped: every one is `u16`-fed (<= 65535) except the var-arg vec,
   which is `Vec::new()` + push per #1710.
2. *"`lower_binary_op`'s default arm silently turns an unknown operator into
   `==`."* — Dropped: enumerating every `Node::binary_op` / `boolean::combine`
   call site shows the produced operator set is exactly `+ - * / % == < <= > >=
   is && ||`, and `is` is intercepted by the guarded arm at lower.rs:110. The
   default is genuinely unreachable.
3. *"`Effect::Conditional`'s dispatch recursion is unbounded."* — Dropped as a
   separate finding: it is bounded transitively by whatever `lower_statements`
   produced, and that lowering-side cap is already **#3279**.
4. *"`actor_quest_trigger_is_in_sequence` and `scene_trigger_actor_approach_system`
   disagree, so a horse can be routed toward a trigger the gate then refuses."*
   — Dropped: re-traced both. Between scenes they are equivalent (both reduce to
   the global minimum not-done, conditions-satisfied `BaseForm` `target_stage >=
   current_stage`). During a running scene the asymmetry (gate `<=` awaited,
   router `==` awaited) is one-directional and permissive, so routed-then-refused
   cannot occur. Matches the 2026-08-24 conclusion.
5. *"The `#3278` `Disable` fix records a form-id in a different keyspace from
   the consumer."* — Dropped: `entity_global_form_id`,
   `resolve_entity_by_global_form_id` and `cell_loader/spawn.rs::placement_is_disabled`
   all key on `FormIdPool::resolve(..).local.0`. Consistent. (Whether that
   keyspace is right across a multi-plugin load order is an ESM-domain question
   that predates this fix and is implied by every existing caller.)

## Skill-file drift found this pass (code is right, the checklist is stale)

Per the standing rule — when the skill's premise disagrees with the code, trust
the code and say so. **Two** items:

1. **Dim 5, `receiver_object`.** The checklist states it must "decline any
   local-variable receiver, including a side-effect-free ident copy", and that
   `ObjectReference k = SomeAlias.GetActorRef()` "remains correct" as a decline
   via #1907. That is no longer the code. `Scope` now carries `object_locals`,
   `bind_local` has an `Object` binding arm, and `object_expr_ref`
   (`effects.rs:1150`) resolves a `GetRef`/`GetActorRef` receiver back to its
   `ObjectRef::Property` — so an alias-getter-derived local **is** tracked and
   accepted. `receiver_object`'s own doc says so ("a local is accepted only when
   `bind_local` proved it came from a VMAD alias getter"). The narrower decline
   that survives, and that `add_item_declines_on_local_receiver` actually
   guards, is a *plain ident copy* (`ObjectReference k = SomeContainer`), which
   lands in `decl_locals`.
2. **Dim 7, `base_record_script_instance` coverage.** The checklist describes the
   chain as ACTI/CONT/NPC/CREA plus the #2189 item family. It is now
   substantially wider: the MODL-only world-placement family
   (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT, via `cells.statics`,
   deliberately placed last so a typed map wins) and TERM both landed under
   #2663, each with a resolves/declines test pair.

## Existing / correctly-tracked — NOT re-filed

All re-verified present against current source:

- **#3501** — `decompile/mod.rs`'s pass-order docstring is still wrong (it lists
  control-flow as phase 3, lower as 4, boolean as 5; the real order in
  `decompile_body` is cfg → lift → **boolean** → control-flow → lower).
- **#3279** — `Effect::Conditional`'s `lower_statements` recursion still has no
  explicit depth cap (`effects.rs:358/361`).
- **#3487 / #3489 / #3496 / #3498** — the four 08-27 Dim 5 findings; all premises
  still hold.
- **#3493** — `apply_effect` still has no doc comment; the nested-lock-safety
  docstring still sits on the three-line `copied_transform` helper.
- **#3160** — `docs/smoke-tests/m47-triggers.sh` still has no assertion that can
  fail on a script-attach regression. The counters it would key on **are** wired
  (`synth_child.rs:143/235/237/720` → `complete.rs:190`), so the gap is in the
  script, not the engine.
- **#3159** — no Lock/Unlock effect primitive.
- **#2668** — `OffsetMap::to_original` is still an unindexed linear scan
  (`crates/papyrus/src/lexer.rs:66`).

Noted but too small to file (and `/audit-tech-debt`'s territory):
`crates/papyrus/src/lexer.rs:11/40` — the `removed` accumulator is written on
every elision then discarded (`let _ = removed;`) under a comment claiming it is
recorded "for end-of-file offset mapping", which it is not.

## Future-Phase Readiness

- **Obscript / `SCTX` (M47.2 Phase 5)** — the invariants this pass pinned that a
  third frontend must inherit: the untrusted-input contract (bounds-safe,
  allocation-bounded, **and now explicitly recursion-bounded** — SCR-D2's fix
  should land as a shared cap, not a `.pex`-only one), and the
  decline-on-any-unmodeled-term rule at the `ScriptSource` boundary. The
  recognizer chain and `compose`/`effects` primitive tables are frontend-agnostic
  and need no change.
- **The fragment lowerer (b2)** — fully wired and live-verified; the open
  soundness item is SCR-D5-2026-08-30-01's guard-resolution third state, which
  should be settled before any further widening of what a `Conditional` may
  contain.
- **M47.1 condition resolvers** — all 13 catalog functions remain implemented
  with correct safe-default sentinels; the outstanding item is still
  *re-verification against a live headless cell with real CTDA data* rather than
  unit tests. Not attempted this pass (no engine launch).
- **M47.3 Phase 4+** — Created Object alias spawn, Story Manager event fills,
  true `LCTN` traversal, reference-collection aliases, unloaded-world
  Find-Matching search, and the injected packages/spells/keywords overlay
  families all remain parsed-and-exposed rather than applied. Documented, not
  silent; not re-filed.
- **Corpus instrumentation** — the `AddItem`/`MoveTo` yield question was closed
  by the 2026-08-27 pass (`AddItem` non-zero at 54 emissions; `MoveTo`
  structurally zero, tracked as #3487). Not re-run this pass; no game-data
  corpus run was performed.

## Findings Count

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 2 |
| LOW | 1 |
| **Total** | **4** |

Dimensions producing **no new findings**: **1** (`.pex` reader & opcode decode),
**4** (Papyrus lexer & Pratt parser), **6** (scripting runtime systems), and
**7** (engine attach & trigger wiring).
Dimensions producing findings: **2** (1 HIGH), **3** (1 LOW), **5** (1 MEDIUM),
**8** (1 MEDIUM).
