# Scripting Subsystem Audit — 2026-08-12

Eleventh full pass over the M30/M47 Papyrus/`.pex`/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`, `_07-03.md`,
`_07-06.md`, `_07-16.md`, `_07-21.md`, `_07-25.md`, `_08-03.md`, `_08-07.md`).
Run as 7 dimension agents (max 3 concurrent), covering `crates/pex/`,
`crates/papyrus/`, `crates/scripting/`, and the engine-side attach path
(`byroredux/src/cell_loader/references/`, `byroredux/src/asset_provider/`,
`byroredux/src/commands/quest.rs`, `crates/plugin/src/esm/records/`).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 300`
(226 open issues) + per-issue `gh issue view` state checks + direct verification
against `docs/audits/AUDIT_SCRIPTING_2026-08-07.md`.

**Test baseline** (all green, no regressions):
`cargo test -p byroredux-pex` — 49 unit + 1 doc, 0 failed, 1 ignored
(`da10_main_door_decompiles_to_the_r5_reference_shape`, needs Skyrim SE data).
`cargo test -p byroredux-papyrus` — 85 unit + 4 integration, 0 failed.
`cargo test -p byroredux-scripting` — **280 passed** (up from 276), 0 failed,
3 ignored (`pex_recognize_e2e`, game-data-gated) + 2 ignored doc-tests.
`cargo test -p byroredux cell_loader::references` — **26 passed** (up from 24).
`cargo test -p byroredux quest` — 13 passed.

## What changed since 2026-08-07

`crates/pex/` is **byte-for-byte unchanged** for the fourth consecutive pass
(unchanged since 2026-07-25); `crates/papyrus/` likewise, its only diff being
the `thiserror` line removed by `ad8335ba`'s TD8 dead-dep sweep (verified
behaviour-neutral). All growth is downstream:

- `crates/scripting/src/scene.rs` **+982**, `quest_stages.rs` **+746**,
  `fragment.rs` **+325**, `translate/effects.rs` **+266**
- `byroredux/src/cell_loader/references/mod.rs` **+440**, `spawn.rs` **+800**
- new console surface `byroredux/src/commands/quest.rs`

Four prior-pass findings were closed by fixes in this window, each independently
re-verified below: **#2538** (`90ae915c`), **#2539** (`6ad64ef6`), **#2269**
(`dc9ba0e5`), and **SAVE-D6-01** (`c4c30afd`).

**Methodology upgrade this pass.** Dimensions 1 and 2 stopped taking the
decompiler's "verbatim Champollion port" claim on faith and diffed it
line-for-line against the actual upstream source at
`/mnt/data/src/reference/Champollion`. Dimension 5 ran the
`fragment_coverage` harness against real Skyrim SE + FO4 archives (28,758
behavioral fragments) rather than reasoning about yield. Dimension 7 ran a
`VMAD` corpus census over `Skyrim.esm` and `Fallout4.esm`. Three of this pass's
most consequential findings are direct products of that shift — they were not
reachable by re-reading Rust source alone, which is what the three prior
zero-finding passes over `crates/pex/` did.

## Executive Summary

**Shipped and re-confirmed live**: M30.2 `.psc` parser; M47.0 event hooks;
M47.1 condition eval (19 catalog functions); M47.2 `.pex` reader + 5-phase
decompiler + recognizer chain + dynamic attach path + XPRM trigger volumes +
fragment lowerer + QUST VMAD property table + `AddItem`/`MoveTo` object
targeting; the MQ101 PACK/SCEN/DIAL/two-state-activator/player-control runtime;
M47.3 quest-lifecycle effects and the quest-alias fill-and-apply runtime
(`SceneActorBindings`, alias-injected faction/inventory application, the
permanent inventory-grant save ledger, alias-bound `ObjectRef::Property` /
`RunOn::QuestAlias` resolution).

**Deferred, correctly, not flagged as defects**: Obscript/SCTX frontend (Phase
5); M47.3 Phase 4+ (Created Object alias spawn, Story Manager event fills, true
`LCTN` traversal, reference-collection aliases, unloaded-world Find-Matching
search, injected packages/spells/keywords overlay families).

**Findings this pass: 20 new — 0 CRITICAL / 2 HIGH / 10 MEDIUM / 8 LOW**, plus
one prior finding re-confirmed **still unfixed** (#2542, LOW). This is a sharp
rise from last pass's 5, and the rise is real rather than a scoping artifact:
17 of the 20 come from surfaces that either grew substantially this window
(Dims 5/6/7) or were probed with a new instrument (Dims 1–4). Notably,
**Dimensions 1–4 broke a three-pass zero-finding streak** — not because the code
changed (it did not) but because it was checked against upstream and against
pathological input for the first time.

**Untrusted-input robustness verdict — CLEAN, with one qualification.** No
panic, OOB index, or unbounded allocation is reachable from hostile `.pex` or
`.psc` bytes. Re-verified independently: every `.pex` primitive read funnels
through `take()`; the `OpCode::from_u8` transmute guard is `>=` over contiguous
`0..=50` discriminants with full 51-value test coverage; hostile var-arg counts
never feed `Vec::with_capacity`; all four recursion caps
(`MAX_REBUILD_DEPTH=1024` ×2, `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH=256`) are present
and threaded; `ScriptInstanceData::parse`'s only attacker-influenced capacity is
clamped at `count.min(4096)`. **The qualification**: SCR-D3-NEW11-01 is a
*wrong-AST* hazard reachable from a hand-crafted `.pex` — memory-safe and
fail-soft, but it is untrusted-input-reachable and produces silently incorrect
output rather than an error.

**The 99.996% (26640/26641) decompile-rate claim — HONEST, but it does not
measure what one of its citations claims.** `pex_corpus_smoke.rs` genuinely runs
`decompile_script`; `catch_unwind`'s panic arm and the `Err` arm both feed the
failure count and neither reaches the numerator. However `Ok(Ok(_))` **discards
the resulting `Script` without any shape check**, so the rate measures
*robustness*, not *fidelity* — a decompile that succeeds with a wrong AST scores
as a success. This matters because `boolean.rs`'s module doc cites this rate as
validation for its no-debug-line-guard departure, and SCR-D3-NEW11-01 is exactly
a wrong-AST-without-error defect in that pass. The citation does not support the
claim it is attached to.

**The `.psc`-vs-`.pex` fidelity gate — half of it does not run.**
`recognizes_da10_and_reproduces_hand_builder` (the `.psc` side) passes and does
pin byte-equality against `da10_main_door(...)`. But that test never touches
`decompile_script`. The `.pex` side —
`da10_pex_reproduces_hand_builder_byte_for_byte` (#1740) and
`da10_main_door_decompiles_to_the_r5_reference_shape` — remains `#[ignore]`-gated
on Skyrim SE game data and **did not execute in any dimension's test run this
pass**. The domain therefore has no executing end-to-end fidelity gate on the
decompiler in a default `cargo test`, which is the structural reason
SCR-D3-NEW11-01 and SCR-D5-NEW11-01 both went unnoticed.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`) | Yes | Yes | Yes | Yes — now upstream-diffed |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes — now upstream-diffed |
| Lift + copy-prop (`lift.rs`) | Yes | Yes (#2024 O(n) intact) | Yes | Yes — equivalence with upstream's restart semantics *proved*, not assumed |
| Boolean (`boolean.rs`) | Yes | Yes (`MAX_REBUILD_DEPTH`, #1815) | Yes | **No — see SCR-D3-NEW11-01** |
| Control-flow (`control_flow.rs`) | Yes | Yes, same cap (#1729) | Yes | Yes (fail-closed #1732 intact) |
| Lower (`lower.rs`) | Yes | Yes | Yes | Yes (`lower_binary_op` default arm proved unreachable) |

**The two documented Champollion departures, re-adjudicated:**

- **`control_flow.rs`'s deliberate `||`-skip — BENIGN, confirmed.** Still fails
  closed with `ControlFlowFailed` (#1732); `translate_pex` degrades it to a
  clean decline.
- **`boolean.rs`'s missing debug-line guard — NO LONGER BENIGN.** Four prior
  passes adjudicated this benign by reasoning about one ambiguous shape (an
  `If`-guarded reassignment, which genuinely *is* semantics-preserving). A
  second ambiguous shape exists — a `While` loop whose one-statement body writes
  the condition variable — and it is **not** semantics-preserving: the collapse
  deletes the back edge and the loop vanishes from the AST, replaced by a
  fabricated `&&`. See SCR-D3-NEW11-01.

Beyond the matrix, this pass mechanically verified against upstream C++ the
three items the checklist flags as fatal-if-wrong and that no prior pass had
checked against the source: `CallStatic`/`CallMethod`/`CallParent` operand
indices (exact — method-name operand never swapped, so `SetStage`-keyed
recognizers are safe), `jmpf`/`jmpt` edge polarity (exact), and the
`is_final`/`is_temp_var` asymmetry (exact, including the case-sensitive `::temp`
vs caseless `::nonevar` split). ByroRedux is *stricter* than upstream in three
places, all correct hardening: the `n >= 0` var-arg guard, `FunctionType` byte
validation, and the `>1` copy-prop path counting before mutating.

## Decline-Invariant Audit

The load-bearing invariant held across most of the newly-grown surface, but this
pass found **the first confirmed live leak** and established that the mechanism
behind last pass's HIGH was never actually closed.

| Decline point | Verdict |
|---|---|
| `classify_guard_atom(...)?` per-atom | Conservative — no swallowed `None` |
| `split_and` refuses to split `\|\|` | Conservative, intentional |
| `lower_fragment` flat-sequence model | Conservative — everything outside `VarDecl`/`Assign`/`Return(None)`/`ExprStmt` declines |
| `lower_3d_loaded_wait` (the one `While` exception) | **Has not widened** — still exactly OR-of-`!Is3DLoaded` + single positive `Utility.Wait` |
| `receiver_object` `self` guard + local-receiver decline | Conservative |
| `ObjectRef::Property` alias-bound resolution | Conservative — no path trusts a raw `form_id` beside a live `alias >= 0`; unfilled alias returns `None` |
| `AddItem`/`MoveTo` conservative shapes | Conservative |
| `RECOGNIZERS` order (per-script before generic) | Correct |
| `translate_pex` clean-`None` incl. `catch_unwind` | Intact (#1816) |
| **`quest_via` bare-identifier arm** | **LEAKS — SCR-D5-NEW11-02 (HIGH)** |
| **#2538's `known_quest_properties` guard** | **INERT on real input — SCR-D5-NEW11-01** |
| `two_state_activator::vmad_bool` | Collapses two cases — SCR-D5-NEW11-04 (LOW) |

The structural lesson: #2538's fix swept `EFFECT_PRIMITIVES` for *intra-table*
duplicate method names. The actual hazard class is **table-vs-Papyrus-API**
collision — one modeled method name declared on more than one receiver type in
the game's own API, reached through a permissive receiver resolver. Sweeping the
14,026 base `scripts\*.pex` for every modeled method name found two live
collisions the fix missed (`Reset`, `SetActive`) and confirmed nine others safe.
That sweep should become a checked-in gate.

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage | **CLEAN** — all 44 `impl Component for` types enumerated: 14 drained by `event_cleanup_system`, 10 self-drained unconditionally at their consumer's head (each verified to have no early return before the drain), remainder persistent state. `event_cleanup_system` is the last scheduled system. **No marker re-fires every frame.** |
| Marker *ordering* | **DEFECT — SCR-D6-NEW11-01 (HIGH)**. The inverse problem: a marker emitted after 3 of its 4 consumers and drained the same frame. |
| Two-phase lock-drop | CLEAN — explicit `drop()` verified in `timer_tick_system` (`timer.rs:48`), `recurring_update_tick_system` (`recurring_update.rs:168`), `trigger_detection_system` (block-scoped, `trigger.rs:119-138`) |
| Cascade bound | CLEAN — `MAX_CASCADE=64` + WARN; no-op re-set skip (#2124) compares `previous_stage != new_stage` correctly |
| Lock-nesting surface (#2269/#2539) | **PARTIALLY isolated** — both named fixes are real, but 6 resource + 12 component acquisitions remain nested in the hold scope (SCR-D6-NEW11-03). No live reverse-order acquirer; all scripting systems are `add_exclusive`. |
| CTDA OR-precedence | CLEAN — trailing-`or_next` clamp guards the OOB; empty list → `true` |
| Edge-trigger seed (#1817) | CLEAN — `None` branch never pushes to `entered` |
| Quest-stage history | CLEAN — `stages_done` retained across advances; `reset` scoped to one quest |
| `recurring_update` timing | CLEAN — accumulate-not-reset; all four named guards pass |
| Alias faction/overlay rollback | CLEAN — restores only when `original_rank.is_none()`; overlays removed for entities absent from the desired set |
| `ScriptRegistry` / `register_spawners` | CLEAN — retired under #2191; no non-test call site survives |
| Alias-fill single-entity loop | **DEFECT — SCR-D6-NEW11-04**: `ALCS` collection aliases not excluded |
| Canonical reference-identity stamping | CLEAN at all 10 `is_primary_synth` sites (grew from 9); **an 11th, un-gated hand-rolled site exists in `exterior.rs`** — SCR-D7-NEW11-03 |

---

## Findings

### HIGH

#### SCR-D5-NEW11-02: `Reset()` and `SetActive()` are claimed as quest effects through the permissive bare-identifier receiver, mis-lowering `ObjectReference.Reset()` / `Weather.SetActive()`

- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:540-548`
  (`prim_reset_quest`), `:550-559` (`prim_set_quest_active`), reached through
  `receiver_quest` (`:974-985`) → `quest_via`
  (`crates/scripting/src/translate/compose.rs:121-133`)
- **Status**: NEW
- **Description**: `quest_via`'s bare-identifier arm accepts **any**
  `Expr::Ident` as `QuestRef::Property(name)` — no type check and, unlike
  `receiver_object`, no known-property filter. `prim_reset_quest` matches
  `<ident>.Reset()` with zero args; `prim_set_quest_active` matches
  `<ident>.SetActive([bool])`. Both method names are shared with non-Quest types
  in the game's own API, and neither has the dispatch-time disambiguation
  fallback that `StartScene`/`StopScene` received. A quest fragment containing
  `MyContainerRef.Reset()` therefore does not decline: it emits
  `Effect::ResetQuest`, the fragment is *claimed*, every sibling effect is
  applied, and the real `ObjectReference.Reset()` semantics are silently dropped.
  This is the generalization of #2538 that the fix's intra-table sweep missed.
- **Evidence**: Authoritative, from the game's own decompiled base scripts
  (`scripts\quest.pex`, `objectreference.pex`, `cell.pex`, `weather.pex` out of
  `Skyrim - Misc.bsa`; temporary probe run then deleted):
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
  #2538's context set cannot help — `MyContainer` is not a Quest property, so it
  is absent from the set by construction. The 1-arg overload
  `MyContainer.Reset(SomeMarker)` correctly declines; only the zero-arg call
  collides, which is the common calling form.
  Measured incidence in the production fragment path across 28,758 behavioral
  fragments (`Skyrim - Misc.bsa` + `Fallout4 - Misc.ba2`): `ResetQuest` **0**,
  `SetQuestActive` **4 — all 4 on genuinely `quest`-typed properties**.
- **Impact**: Currently **latent in vanilla Skyrim/FO4** — but reachable from
  mods, DLC-embedded scripts, and the unscanned Starfield/FO76 corpora. When hit:
  the object is never reset / the weather never applied, `QuestStageState::reset`
  runs against a non-quest form id, `scene_actor_bindings_dirty` is set
  spuriously, and every other effect in the fragment is applied as though the
  whole fragment had been understood. The domain's escalation table rates
  "recognizer emits a component on an unmodeled term instead of declining" HIGH
  on impact regardless of likelihood.
- **Related**: #2538 (closed), #2289, SCR-D5-NEW11-01
- **Suggested Fix**: Gate `prim_reset_quest`/`prim_set_quest_active` on a narrow
  receiver the way `prim_start_quest`/`prim_stop_quest` are — accept
  `QuestRef::SelfRef`/`OwningQuest`/a quest-bound local and (once
  SCR-D5-NEW11-01's normalization lands) a *known* Quest-typed property; decline
  a bare unqualified identifier. Longer term, stop letting `quest_via` hand out
  `QuestRef::Property` for arbitrary identifiers, and make the base-script
  method-name sweep a checked-in gate so the next primitive added to the table is
  checked against the real Papyrus API surface, not only against its table
  siblings.

#### SCR-D6-NEW11-01: A fragment's `Effect::Activate` marker is emitted after 3 of its 4 consumers and drained the same frame — every lowered `<Ref>.Activate()` in a quest fragment is inert

- **Severity**: HIGH (domain escalation: "Transient marker not drained / drained
  out of stage order" — here worse than a frame late; three of four consumers
  never see it at all)
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: producer `crates/scripting/src/fragment.rs:571-585`
  (`Effect::Activate` arm of `apply_effect`); scheduling
  `byroredux/src/boot.rs:748` (`rumble_on_activate_dispatch`), `:757`
  (`quest_advance_dispatch`), `:786` (`two_state_activator_system`), `:797`
  (`quest_fragment_dispatch`), `:843` (`mg07_on_activate_dispatch`), `:1196`
  (`event_cleanup_system`, `Stage::Late`)
- **Status**: NEW
- **Description**: `quest_fragment_dispatch_system` is the *last* producer of
  `ActivateEvent` in the Update stage, but three of its four consumers are
  scheduled **earlier** in the same stage, and `event_cleanup_system` drains
  `ActivateEvent` at `Stage::Late` in the *same* frame. A marker emitted at slot
  797 therefore never reaches a slot-748/757/786 consumer on this frame, and no
  longer exists on the next. Only `mg07_on_activate_dispatch` (843) sits
  downstream.
- **Evidence**: Proven with a schedule-order probe (temporary test, run,
  reverted): marker emitted = `true`, `TwoStateActivator.is_open` after the next
  frame = **`false`**. The cited guard
  `dispatch_activate_then_set_open_updates_mq101_style_gate` only asserts the
  component exists — it never runs the consumer, which is why the gap survived.
- **Impact**: Every lowered `<Ref>.Activate()` in a quest fragment silently
  no-ops against a two-state activator or a quest-advance REFR. Silent — no
  crash, no log contradiction, just a door/lever/quest gate that should have
  fired and didn't. This is the same "silently corrupts game logic" class the
  decline invariant exists to prevent, arriving through scheduling rather than
  through lowering.
- **Related**: #2269, #2539 (same function's growth), SCR-D6-NEW11-03
- **Suggested Fix**: Either move `quest_fragment_dispatch` ahead of the
  `ActivateEvent` consumers in `boot.rs`, or route fragment-emitted activations
  through the existing deferred-effect queue so they land at the head of the next
  frame before any consumer runs. Add a regression test that actually *runs* the
  consumer after the producer and asserts `is_open` flips.

### MEDIUM

#### SCR-D3-NEW11-01: Boolean pass's missing debug-line guard silently erases a `While` loop whose one-statement body writes the loop-condition variable

- **Severity**: MEDIUM
- **Dimension**: Decompiler Control-Flow / Boolean / Lower (Dimension 3)
- **Untrusted-Input**: **Yes**
- **Location**: `crates/pex/src/decompile/boolean.rs:143-158` (the `&&`/`||`
  candidate test) and `:172-247` (`collapse`); documented departure at `:17-22`
- **Status**: NEW — corrects the "benign" adjudication in
  `AUDIT_SCRIPTING_2026-07-02.md:103`, `_07-06.md:109`, `_08-03.md:148`,
  `_08-07.md:137`. Distinct from #2028/SCR-D3-NEW-01, which covered only the
  `operand_key == rejoin_key` degenerate shape.
- **Description**: `collapse` decides a block pair is a short-circuit `&&`/`||`
  from three structural signals only — the source block is conditional, its last
  statement computes the condition variable, and the fall-through edge's block is
  a single statement recomputing *the same* variable. It never checks that the
  operand block actually **falls through to the rejoin block**. A loop body
  satisfies all three signals while its `next` edge points *backwards* to the
  loop head. The collapse then deletes the operand block — destroying the back
  edge — merges the rejoin block's statements and adopts its edges, so the
  `While` disappears entirely, replaced by a fabricated `&&`.
- **Evidence**: Empirically reproduced (throwaway harness, since deleted; tree
  clean). Input (structurally valid — `build_cfg` accepts it, every jump in
  range):
  ```
  0: cmp_eq ::temp0, a, b        ; loop condition
  1: jmpf   ::temp0, +3  -> 4    ; loop exit test
  2: cmp_eq ::temp0, c, d        ; loop body — single stmt, writes ::temp0
  3: jmp    -3           -> 0    ; back edge
  4: return
  ```
  Output (verbatim, trimmed):
  ```
  Function Case1
    body: [ ExprStmt( BinaryOp{ left: BinaryOp{a Eq b}, op: And,
                                right: BinaryOp{c Eq d} } ),
            Return(None) ]
  ```
  No `Stmt::While` anywhere. Control case confirms the benign shape prior audits
  reasoned about still behaves correctly (`If bDone / bDone = Bar() / EndIf` →
  `bDone = Foo() && Bar()`, genuinely equivalent), so the fix must not break it.
- **Impact**: A wrong-but-non-panicking AST — the exact failure class the
  domain's escalation table calls out, and one **invisible to both instruments
  `boolean.rs:21-22` cites as validation**: the corpus smoke scores such a file
  as a clean success, and the R5 fidelity gate is a single `#[ignore]`d script
  that did not run. MEDIUM rather than HIGH because the shape could not be
  constructed from official-compiler output (a discarded call result goes to
  `::NoneVar`, never the condition temp), vanilla decompiles 26640/26641 with no
  reported shape regressions, and recognizers decline the resulting bare
  `ExprStmt`. The exposure is a hand-crafted or third-party-compiled `.pex`
  shipped by a mod. **Escalate to HIGH if a vanilla instance is ever found.**
- **Related**: #2028, #1732, the departure text at `boolean.rs:17-22`
- **Suggested Fix**: One edge check in `collapse` before accepting — the operand
  block must fall through to the rejoin:
  `self.cfg.block(operand_key).is_some_and(|b| b.next == rejoin_key && !b.is_conditional())`.
  Verified against every shape this pass: rejects the loop case, preserves both
  real short-circuit shapes and the benign `If`-guard case. Add the loop stream
  as a regression test, and correct `boolean.rs:20-22` — the corpus rate cannot
  validate a wrong-AST-without-error departure.

#### SCR-D4-NEW11-01: `parse_property_flags` reaches across the newline and swallows the `Auto` of a following `Auto State`, silently demoting the script's auto-state

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser (Dimension 4)
- **Untrusted-Input**: **Yes**
- **Location**: `crates/papyrus/src/parser/script.rs:421-453`
  (`parse_property_flags`), interacting with
  `crates/papyrus/src/parser/mod.rs:77-87` (`peek_with_span`) and
  `crates/papyrus/src/parser/script.rs:551-557` (`parse_state`)
- **Status**: NEW
- **Description**: `parse_property_flags` is a `loop { match self.peek() { … } }`
  and `Parser::peek` deliberately **skips `Token::Newline`**. The flag loop
  therefore does not stop at the end of the property's declaration line — it
  scans into subsequent lines for more flags. `Auto` is both a property flag and
  the leading token of a top-level `Auto State` item, so when a **short-form
  property declaration is the last thing before an `Auto State`**, the flag loop
  consumes the state's `Auto`. `parse_state` then finds no `KwAuto` and builds
  the state with `is_auto: false`, with **zero errors reported**. All six flag
  loops were checked; this is the only vulnerable one, because `Auto` is the only
  flag that is also a legal item-starter.
- **Evidence**: Reproduced with a throwaway integration test (since removed).
  Five cases, all parsing with **zero** reported errors:
  ```
  A: Auto property + Auto State   → STATE Waiting is_auto=false   <-- WRONG
  B: control, plain `State`       → STATE Waiting is_auto=false   (identical to A)
  C: a Function separates them    → STATE Waiting is_auto=true    (correct)
  D: full-form property           → STATE Waiting is_auto=true    (correct)
  E: top-level variable           → STATE Waiting is_auto=true    (correct)
  ```
  A vs B is load-bearing: `Auto State Waiting` and `State Waiting` produce
  **byte-identical ASTs**. The crate's one guarding assertion
  (`r5_round_trip.rs:96`, `active.is_auto` on the real
  `defaultRumbleOnActivate.psc`) passes **only by accident** — every property in
  that fixture has a trailing `{ doc comment }`, and `peek()` skips `Newline` but
  not `DocComment`; removing the doc comment makes the same script parse wrong.
- **Impact**: Bounded today, which keeps it out of HIGH: `is_auto` has no runtime
  consumer, `.pex` hardcodes it `false`, and `parse_script` has **no production
  caller** anywhere in the workspace (the engine's live path consumes `.pex`).
  The exposure is future: the moment `.psc` gains a production consumer, or
  `is_auto` gains one, a script's auto-state is silently demoted with no
  diagnostic.
- **Related**: #2185 (the sibling `skip_to_next_line` EOF hang, fixed)
- **Suggested Fix**: Have `parse_property_flags` stop at a `Newline` — either
  peek raw (not newline-skipping) in the flag loop, or record the property's line
  and break when the next flag token's span crosses it. Add cases A–E as
  regression tests.

#### SCR-D5-NEW11-01: #2538's `known_quest_properties` guard never fires on decompiled `.pex` — the fix is inert, and the finding it fixed overstated its impact

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:1044-1071`
  (`receiver_object`, the un-normalized `key`), `crates/scripting/src/fragment.rs:1170-1184`
  (`quest_property_names`), regression test at `effects.rs:1425-1492`
- **Status**: **Regression of #2538** (incomplete fix, `90ae915c`)
- **Description**: Two independent key-space mismatches make the guard
  unreachable on the only input the production path ever sees.
  (1) `quest_property_names` stores the *authored* property name lowercased
  (`mq101`), but `receiver_object` looks up the identifier straight off the
  `Expr::Ident` — and a decompiled `.pex` auto-property read is the **backing
  variable** `::MQ101_var`, not `MQ101`. `ObjectRef::property_name()`
  (`compose.rs:72-78`) exists precisely to strip that decoration, but is applied
  only downstream at dispatch, never before this lookup.
  (2) The type test is an exact `Type::Object("quest")` match, so properties
  typed with a Quest-*derived* script (`mq206script`, `dn019script`,
  `min03script`, …) are never collected.
- **Evidence**: Instrumented `fragment_coverage` to run **both** entry points
  over the same corpus (temporary edit, run, reverted):
  ```
  fully lowered, context-free : 9361   effects: 11284
  fully lowered, production   : 9361   effects: 11284
  fragments claimed context-free but DECLINED in production: 0
  ```
  Byte-identical. Cross-checking each lowered effect's receiver against its
  declared property type shows **63 `Start`/`StopScene` effects whose receiver is
  literally a `Quest`-typed property of the same script**, all surviving the fix
  unchanged (`mq102`, `seranacurequest`, `dlc1vq00`, `da13`, `db11`, `bos301`, …).
  Direct AST-level repro: `::MQ101_var.Start()` with correct context still lowers
  to `StartScene`. The shipped regression test **cannot** catch this — the `.psc`
  `Ident` regex `[a-zA-Z_][a-zA-Z0-9_]*` (`crates/papyrus/src/token.rs:253`)
  cannot produce a `::X_var` receiver.
  Corroborating corpus signature: `StartQuest` **0** vs `StopQuest` **728** —
  `Self.Stop()` resolves through `explicit_quest_receiver`, while every
  *cross-quest* `X.Start()` still becomes `StartScene`.
- **Impact**: Low *runtime* impact, and this corrects the prior report:
  `a844c26b` — the same commit that introduced the ambiguity — also added a
  dispatch-time fallback (`fragment.rs:506-568`) resolving a `StartScene` form-id
  against `QuestDefinitionRegistry` and performing the quest start instead. So
  **"the quest silently never starts" was already wrong at filing time**, and
  #2538's fix was written against a bad premise. Real costs: (i) the codebase now
  carries threaded metadata, a `HashSet` clone per fragment, and a green
  regression test that collectively assert an ambiguity is resolved at translate
  time when it is not; (ii) one genuine divergence remains — the fallback
  early-returns on absent `QuestDefinitionRegistry` where `Effect::StartQuest`
  would still call `stages.start_quest`; (iii) where the guard *would* fire it
  declines the **whole fragment**, discarding every sibling effect — strictly
  worse than the fallback.
- **Related**: #2538 (closed), `a844c26b`, `90ae915c`, SCR-D5-NEW11-03
- **Suggested Fix**: Normalize the lookup key through `ObjectRef::property_name()`
  semantics before consulting `known_quest_properties`, and widen
  `quest_property_names` to accept Quest-derived script types (better: key off
  the `.pex` property type table rather than the AST). Then use the set
  **positively** — have `explicit_quest_receiver` accept `QuestRef::Property(name)`
  for a known Quest property so `MQ101.Start()` lowers to `Effect::StartQuest`
  instead of declining the fragment. Add a regression test built from a
  hand-constructed `::X_var` AST.

#### SCR-D5-NEW11-03: `fragment_coverage` and `mq101_conformance` measure the context-free lowering path, not the production one

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/examples/fragment_coverage.rs:147`,
  `crates/scripting/examples/mq101_conformance.rs:1407,1450`
- **Status**: NEW
- **Description**: Both harnesses call `lower_fragment` (empty quest-property
  set) while the single production caller,
  `fragment.rs::populate_quest_fragments_from_script`, calls
  `lower_fragment_with_quest_properties` with a real set. `fragment_coverage` is
  the crate's coverage-regression gate and the instrument the M47.3 Phase-2
  checklist points at; `mq101_conformance` is the MQ101 behavioural gate. Neither
  measures what the engine actually does.
- **Evidence**: Full call-site enumeration in the Dim-5 report. Concretely:
  because these harnesses use the context-free path, the whole of
  SCR-D5-NEW11-01 — a shipped fix that changes nothing on real data — was
  invisible to every existing instrument. Today the two paths agree exactly
  (9361/9361, 11284/11284), but only *because* of that bug; the moment it is
  fixed, the harnesses will silently diverge from production.
- **Impact**: The coverage and conformance gates can report a claim rate and
  effect histogram production does not reproduce, in either direction. The M47.3
  Phase-2 "live-corpus re-measurement" checkbox cannot be honestly ticked from
  the harness as written.
- **Related**: SCR-D5-NEW11-01, `docs/engine/m47-3-quest-alias-design.md` Phase 2
- **Suggested Fix**: Lift `quest_property_names` into `translate::effects` (or
  make it `pub(crate)`) and have both examples call
  `lower_fragment_with_quest_properties` with the per-script set. Consider making
  `lower_fragment` `#[doc(hidden)]`/test-only so a future call site cannot
  accidentally pick the context-free path.

#### SCR-D6-NEW11-02: `DeferredFragmentEffects::new` deep-clones the whole `QuestDefinitionRegistry` every frame, before the early-bail

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:337-341`
  (`DeferredFragmentEffects::new`), consumed by
  `quest_fragment_dispatch_system`; early-bail at `:1372-1374`
- **Status**: NEW (introduced by the #2539 fix, `6ad64ef6`)
- **Description**: The #2539 fix correctly snapshot-clones
  `QuestDefinitionRegistry` before taking the `(QuestStageState,
  QuestObjectiveState)` guards, eliminating the nested acquisition. But the clone
  happens unconditionally in `new()`, **before** the
  `queue.is_empty() || frags.is_empty()` bail — so a frame with no quest activity
  at all still deep-copies the entire registry.
- **Evidence**: Measured on real `Skyrim.esm` (1811 QUST records): **0.651
  ms/frame in release**. On a synthetic 5,000-quest load order (a heavily-modded
  install): **15.6 ms/frame** — i.e. the whole frame budget.
- **Impact**: A flat per-frame cost proportional to load-order quest count, paid
  whether or not any fragment dispatches. At vanilla scale it is a real but
  survivable ~4% of a 16.6 ms budget; at modded scale it is frame-rate
  determining. Note the user's stated invariant that a CPU bottleneck is a bug.
- **Related**: #2539, #2269, SCR-D6-NEW11-03
- **Suggested Fix**: Move the bail ahead of the clone (construct
  `DeferredFragmentEffects` lazily only once there is work), or replace the deep
  clone with an `Arc` snapshot the registry swaps on mutation — its writers are
  load-time-only (`&mut World`), so an `Arc<QuestDefinitionRegistry>` read is
  sound and free.

#### SCR-D6-NEW11-03: #2539's lock isolation is partial — the hold scope still nests 6 resource acquisitions (3 writes) and 12 component acquisitions

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs` — the
  `(QuestStageState, QuestObjectiveState)` hold scope spanning the cascade loop;
  residual `SceneActorBindings` read via `resolve_object` (`:246-248`); three
  `PlayerControlState` writes; 12 component acquisitions incl. `Inventory`
  (`AddItem`) and `GlobalTransform`+`Transform` (`MoveTo`)
- **Status**: NEW (the two resources #2539 named are correctly fixed; these are
  the residual)
- **Description**: `6ad64ef6` did exactly what it scoped:
  `QuestDefinitionRegistry` is snapshot-cloned before the guards and every former
  in-scope `try_resource` now reads `deferred.quest_definitions`; every in-scope
  `mark_scene_actor_bindings_dirty(world)` became a deferred flag. But
  `SceneActorBindings` is still **read**-acquired inside the scope via
  `resolve_object`, so the `QuestStageState → SceneActorBindings` nesting is only
  half-eliminated, and five other resources plus twelve component locks remain
  nested.
- **Evidence**: Full enumeration in the Dim-6 report. No live reverse-order
  acquirer exists for any residual resource — all scripting systems are
  `add_exclusive` (`boot.rs:747-846`) — matching #2269's own "no live deadlock
  today" risk profile.
- **Impact**: None live today. The surface any future parallelization must sweep
  is larger than #2539's closure implies, and the issue was closed as though the
  isolation were complete.
- **Related**: #2539 (closed), #2269 (closed), #2270 (open — the undocumented
  "snapshot before iterate" house rule)
- **Suggested Fix**: Resolve alias lookups before the guards (the `resolve_object`
  results the loop needs are knowable from the queue up front), and record the
  residual nesting in the house-rule doc #2270 asks for, so the next arm added to
  `apply_effect` does not silently extend it again.

#### SCR-D6-NEW11-04: `ALCS` collection aliases are not excluded from the single-entity fill loop — a collection alias binds one candidate and receives the whole collection's injected data

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems (Dimension 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/scene.rs` — the alias fill loop in
  `resolve_alias_bindings`; `ALMI` parsed and never read
- **Status**: NEW
- **Description**: `docs/engine/m47-3-quest-alias-design.md` lists
  reference-collection aliases as a Phase 4+ deferral, i.e. they should decline
  and diagnose as unavailable. Instead an `ALCS` collection alias with match
  conditions falls through the ordinary single-entity path: it binds **exactly
  one** candidate and that one entity receives the whole collection's injected
  factions and inventory. It also diagnoses as `Bound`, not
  `ReferenceCollectionRuntimeUnavailable`, so the observability added by
  `0775df28` reports success.
- **Evidence**: Probe (temporary test, run, reverted) confirms the single
  binding and the injection application, and confirms the diagnostic string.
- **Impact**: Contradicts the design doc's own deferral: a documented "not built
  yet" path silently half-works instead of declining, which is the decline
  invariant's failure mode applied to the alias runtime. One arbitrary member of
  a collection gets faction membership and inventory intended for the whole set.
- **Related**: `docs/engine/m47-3-quest-alias-design.md` §"Remaining subsystem
  boundary"
- **Suggested Fix**: Detect `ALCS` in the fill loop and decline with the
  `ReferenceCollectionRuntimeUnavailable` diagnostic the design doc specifies,
  until the Phase 4+ collection runtime exists. Read or explicitly drop `ALMI`.

#### SCR-D7-NEW11-01: Actor REFRs never run the script-attach path — `NPC_` base VMAD and `ACHR`-own VMAD are silently dropped for every spawned actor

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/mod.rs:539-606` (the actor
  branch, which `continue`s at `:605` after only `stamp_quest_reference`);
  `crates/plugin/src/esm/records/index.rs:615-620` (the `npcs`/`creatures` arms);
  `byroredux/src/cell_loader/references/attach.rs:158-187`
- **Status**: NEW
- **Description**: `attach_script_for_refr` has exactly three call sites, all
  reachable only from `spawn_synth_child`. The actor branch in
  `load_references_budgeted` intercepts any `child_form_id` present in
  `record_index.npcs` **before** `spawn_synth_child` is called, drives
  `NpcSpawnJob`, stamps the canonical identity, and `continue`s. It never calls
  `attach_script_for_refr`, and no other path does — so the `npcs` arm of
  `base_record_script_instance`, added specifically so scripted actors could
  attach, is **unreachable from the live attach path**, and the placed actor's
  own `ACHR` VMAD is never consumed either.
- **Evidence**: Corpus census over real masters (temporary instrument, run then
  deleted): `Skyrim.esm` — **805/5118 `NPC_`** and **822/10504 `ACHR`** carry a
  `VMAD` (`MQ304LostSoulSons3`, `dunTransmogrifyDremora`, `OgolRef`, …);
  `Fallout4.esm` — **382/3015 `NPC_`**, **516/7615 `ACHR`**.
  `grep -rn attach_script_for_refr\|attach_vmad_scripts` returns only
  `references/{mod,attach}.rs` and their tests; `npc_spawn.rs` contains no script
  wiring at all.
- **Impact**: Every VMAD-scripted actor in Skyrim SE / FO4 / Starfield content
  loads with zero canonical script behaviour, and never contributes to the
  `M47.2 scripts:` counter — so the smoke gate cannot observe the gap either.
  Silent decline (no wrong state), but it removes the single largest non-`ACTI`
  VMAD population from the recognizer chain's reach, and it blocks M47.3
  directly: the alias-fill runtime binds actor entities whose attached Papyrus
  behaviour can never fire.
- **Related**: #2189 (closed — the item-family half of the same structural
  omission); #2567 (open, creature placements); SCR-D7-NEW11-02
- **Suggested Fix**: In the `NpcSpawnProgress::Complete` arm, alongside the
  existing `synth_idx == 0` stamp, call `attach_quest_reference_script(...)` with
  the same `refr_script_instance_for_synth_child(...)` value `spawn_synth_child`
  receives, so actors go through the identical REFR-then-base VMAD merge and feed
  the same counter. Add a test that a scripted `NPC_` REFR increments
  `scripts_recognized`.

#### SCR-D7-NEW11-02: The world-placement base-record family and `TERM` decode `VMAD` as a presence-only flag, so `base_record_script_instance` can never resolve them

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (root cause in `crates/plugin`)
- **Untrusted-Input**: No
- **Location**: `crates/plugin/src/esm/cell/support.rs:74`
  (`b"VMAD" => has_script = true,` — no decode, and `StaticObject` has no field
  to hold one); `crates/plugin/src/esm/records/dispatch_world_placement.rs:25-27`;
  `crates/plugin/src/esm/records/misc/world.rs:362-396` (`parse_term` discards
  `CommonNamedFields::script_instance`);
  `crates/plugin/src/esm/records/index.rs:605-629`
- **Status**: NEW
- **Description**: The exact sibling of the closed #2189. `CommonItemFields` was
  taught to decode `VMAD` and an `items` arm was added — but the two other
  populations reaching cell load through different parsers were not. (1) The
  MODL-only world-placement family (DOOR/FURN/FLOR/MSTT/TACT/LIGH/STAT/IDLM/
  ADDN/BNDS) is parsed by `build_static_object_from_subs`, whose `VMAD` arm sets
  a boolean and drops the payload. (2) `TERM` *is* parsed through
  `CommonNamedFields` (which decodes fully) but `parse_term` throws the decoded
  `script_instance` away — justified by an in-source comment
  (`world.rs:385-387`: *"TERM is FO3/FNV-only, so the helper's VMAD arm never
  fires here"*) that is **factually wrong**.
- **Evidence**: Corpus census (temporary instrument, run then deleted):
  `Skyrim.esm` — `FURN` **34/400**, `DOOR` **5/244**, `FLOR` **3/86** → **42**
  unreachable base records (`GenPullChainAnim01NoPlayer`,
  `CartFurniturePassenger`, `TrapTriggerHinge`, `RiftenRWDoorJail01PRISONER`, …).
  `Fallout4.esm` — `FURN` **157/598**, `TERM` **207/778**, `MSTT` **36/961**,
  `FLOR` **18/53**, `DOOR` **17/371**, `LIGH` **3/801**, `TACT` **3/43**, `STAT`
  **1/19368** → **442** unreachable (`WorkshopBar03Counter`,
  `VRWorkshopShared_VRTerminalMusicSubMenu`, `LoadElevatorDoorHiTech_MinUse`, …).
- **Impact**: Silent decline for a measured 42 Skyrim / 442 FO4 scripted base
  records — a **larger** population than the item family #2189 was filed for. A
  scripted crafting station, planter, workshop bar, jail door, elevator door, or
  FO4 terminal attaches nothing. Partly masked: a REFR carrying its *own* VMAD
  still attaches, so only base-record-level scripts on these types are lost.
- **Related**: #2189 (closed); SCR-D7-NEW4-01 in
  `docs/audits/AUDIT_SCRIPTING_2026-07-25.md`; SCR-D7-NEW11-01
- **Suggested Fix**: Add `script_instance: Option<ScriptInstanceData>` to
  `StaticObject`, populate it in `build_static_object_from_subs`'s `VMAD` arm the
  way `CommonNamedFields` does, and add a `statics` arm at the end of
  `base_record_script_instance` (last, so typed maps keep priority). Separately
  add `script_instance` to `TermRecord`, wire it from `common.script_instance`,
  add a `terminals` arm, and delete the incorrect comment. Pin with a test
  mirroring `base_record_script_instance_resolves_an_item_records_vmad`.

#### SCR-D7-NEW11-03: The exterior logical-actor stub open-codes `stamp_quest_reference` and omits `Transform`/`GlobalTransform`, excluding those candidates from every distance-ranked alias fill

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/exterior.rs:255-289`
  (`PersistentCellApplyJob::apply`'s logical-stub loop); contrast
  `byroredux/src/cell_loader/references/mod.rs:1093-1134`; consumed at
  `crates/scripting/src/scene.rs:832-856`
- **Status**: NEW
- **Description**: The worldspace persistent-cell loader spawns a logical actor
  identity for every persistent `ACHR` with no 3D. It builds the
  `FormIdComponent` + `SceneAliasCandidate` pair **by hand** — a verbatim copy of
  `stamp_quest_reference`'s body — instead of calling it, and unlike
  `spawn_logical_quest_reference` (the interior equivalent, which the skill
  explicitly requires to stay transform-bearing) it inserts **no transform at
  all**, though `placed.position` is available and converted elsewhere in the
  same file.
- **Evidence**: `exterior.rs:266-282` inserts exactly two components;
  `references/mod.rs:1129-1132` inserts `Transform` + `GlobalTransform` *then*
  stamps. `scene.rs:852` —
  `let position = world.get::<GlobalTransform>(entity)?.translation;` inside a
  `filter_map`, so a transform-less candidate is **dropped**, not merely ranked
  last.
- **Impact**: A quest whose Find-Matching / Unique-Actor alias is authored with
  "Closest" (or anchored on another alias) can silently fail to fill when its
  only candidates are persistent-cell logical actors — precisely the population
  (Skyrim's persistent worldspace `ACHR`s) M47.3 was built around. Silent: an
  unfilled alias produces no log line and no error. The duplication is also a
  standing regression vector for #2541's invariant — this is the un-covered
  eleventh stamping site.
- **Related**: #2541 (open); `docs/engine/m47-3-quest-alias-design.md`
  §"Remaining subsystem boundary" (this is *not* the deferred unloaded-world
  search — these stubs are already candidates, they are just positionless)
- **Suggested Fix**: Replace the hand-rolled block with a call to
  `spawn_logical_quest_reference` (making it `pub(crate)`), passing the converted
  position and the REFR's rotation/scale. Add a test asserting a distance-ranked
  alias whose only candidate is a logical stub still fills.

### LOW

#### SCR-D1-NEW11-01: `FunctionInfo`'s field docstrings describe Champollion, not this codebase — one asserts a decompiler safety guard that was deliberately removed

- **Severity**: LOW · **Dimension**: PEX Reader & Opcode Decode ·
  **Untrusted-Input**: No · **Status**: NEW
- **Location**: `crates/pex/src/model.rs:251`, `:253-255`
- **Description**: `line_numbers` is documented as feeding the boolean pass's
  cross-line-merge guard. That guard was **deliberately removed**
  (`boolean.rs`'s departure #1) and the field has **zero readers workspace-wide**.
  `function_type` is documented as falling back to `Method` on an unknown byte;
  the reader yields `None`.
- **Impact**: Documentation only, but the blast radius is audit accuracy on the
  crate's highest-scrutiny pass: it advertises a false-positive-merge safety
  guard that does not exist — directly relevant now that SCR-D3-NEW11-01 shows
  the absent guard has a real consequence.
- **Related**: #2290, SCR-D3-NEW11-01
- **Suggested Fix**: Two one-line edits — say `line_numbers` is parsed for stream
  alignment and currently unread, and that `function_type` is `None` on an
  unknown byte.

#### SCR-D2-NEW11-01: `rebuild_expression`'s producer-drop guard is a `debug_assert!`, compiled out in release

- **Severity**: LOW · **Dimension**: Decompiler CFG & Lift ·
  **Untrusted-Input**: Yes (latent) · **Status**: NEW
- **Location**: `crates/pex/src/decompile/lift.rs:403`
- **Description**: Copy-propagation splits its work across two independently
  maintained traversals (`count_constant_id`→`child_nodes()`,
  `replace_constant_id`→`child_nodes_mut()`). They agree today — diffed
  arm-for-arm across all 17 `NodeKind` variants — but nothing pins the parity and
  `node.rs` has **zero tests**. A future divergence would take the success path
  with `slot` still `Some`, silently deleting the producer statement while the
  consumer keeps a dangling `::tempN` reference: a wrong AST rather than a
  fail-closed `Err`.
- **Impact**: Latent, not live. LOW today; the fix is one line.
- **Suggested Fix**: Return `ExpressionRebuildFailed` on an unconsumed slot, as
  the `>1` arm already does. Add a `child_nodes`/`child_nodes_mut` parity test.

#### SCR-D3-NEW11-02: `collapse`'s edge adoption can delete an on-stack ancestor block; the `.expect` survives only because the depth cap fires first

- **Severity**: LOW · **Dimension**: Decompiler Control-Flow / Boolean / Lower ·
  **Untrusted-Input**: Yes · **Status**: NEW
- **Location**: `crates/pex/src/decompile/boolean.rs:172-177`, `:203-206`,
  `:221-232`; doc claim at `:23-26`
- **Description**: When an inner collapse's rejoin block is an *enclosing*
  conditional currently on the recursion stack, `collapse` removes that ancestor
  while its own call has not yet run — the ancestor then `.expect`s on a block
  that no longer exists. Probed with a crafted CFG: it **fails closed** via
  `RecursionLimit`, not a panic, because adoption always induces self-recursion —
  but that is an accident nothing documents. Separately, the module's
  "termination guard" claim covers only the iterative loop, not the recursion,
  and `RecursionLimit`'s message hardcodes "control-flow reconstruction" even for
  boolean-pass overflows.
- **Impact**: No demonstrated panic. Risk is maintenance-shaped: if the cap is
  raised or bypassed the `.expect` becomes live on untrusted `.pex`; and triage
  is misled by the wrong pass name in the error.
- **Related**: SCR-D3-NEW11-01 (same function; that fix also rejects most of
  these shapes), #1815, #1729
- **Suggested Fix**: Replace both `.expect`s with `return Ok(false)` so the
  invariant is local rather than inherited from the depth cap; narrow the doc;
  add a pass discriminant to `RecursionLimit`.

#### SCR-D4-NEW11-02: `OffsetMap::to_original` is an unindexed linear scan over an already-sorted vec, giving O(N·E) error remapping

- **Severity**: LOW · **Dimension**: Papyrus Lexer & Pratt Parser ·
  **Untrusted-Input**: Yes · **Status**: NEW
- **Location**: `crates/papyrus/src/lexer.rs:66-81`
- **Description**: Called once per reported error over a vec that is already
  sorted by construction. Measured clean quadratic (4× time per 2× input):
  4k → 5 ms, 32k → 305 ms. The two axes in isolation are each linear.
- **Impact**: Only reachable with a `.psc` carrying both many line continuations
  and many errors; `.psc` has no production consumer today.
- **Suggested Fix**: `partition_point` instead of the linear scan.

#### SCR-D5-NEW11-04: `two_state_activator::vmad_bool` silently falls back to the script default on a present-but-non-`Bool` VMAD value

- **Severity**: LOW · **Dimension**: Recognizer-Chain Soundness ·
  **Untrusted-Input**: No · **Status**: NEW
- **Location**: `crates/scripting/src/translate/recognizers/two_state_activator.rs:20-28,66-72`
- **Description**: `vmad_bool` collapses "no such property" and "property present
  but not `Bool`" into the same `None`, which `.or(bool_prop(...))` turns into the
  `.psc` default. That is the two-case collapse #2023 fixed for `bool_arg` and
  #1909 for `rumble::bool_prop`: a present-but-unreadable value must decline, not
  adopt the authored default.
- **Impact**: A `default2StateActivator` whose VMAD carries
  `isOpen`/`isAnimating`/`doOnce` under a non-`Bool` tag spawns in the wrong state
  with no diagnostic. Low likelihood (the CK writes bools as type 5).
- **Related**: #2023, #1909, #2289
- **Suggested Fix**: Give `vmad_bool` the `Option<Option<bool>>` three-case
  contract and propagate the outer `None` as a decline.

#### SCR-D6-NEW11-05: The SAVE-D6-01 rekey silently drops an inventory grant when the entity carries no `SceneAliasCandidate`

- **Severity**: LOW · **Dimension**: Scripting Runtime Systems ·
  **Untrusted-Input**: No · **Status**: NEW
- **Location**: `crates/scripting/src/scene.rs` — the
  `reference_form_ids.get(&entity)` guard added by `c4c30afd`
- **Description**: The rekey from `EntityId` to the stable authored
  `reference_form_id` is correct, but the lookup silently *drops* the grant when
  the resolved entity has no `SceneAliasCandidate`. Unreachable today (every
  alias-bindable entity is stamped), latent for the Phase 4+ Created-Object fill,
  which will produce entities with no authored REFR.
- **Related**: SAVE-D6-01 (fixed), SCR-D7-NEW11-03
- **Suggested Fix**: Log at `warn` on the miss rather than dropping silently, and
  revisit when Created-Object fill lands.

#### SCR-D6-NEW11-06: Alias match-CTDAs using `RunOn::QuestAlias` read the previous refresh's binding table, not the in-progress one

- **Severity**: LOW · **Dimension**: Scripting Runtime Systems ·
  **Untrusted-Input**: No · **Status**: NEW
- **Location**: `crates/scripting/src/scene.rs` — `resolve_alias_bindings`'s
  condition evaluation against `SceneActorBindings` rather than the in-progress
  `resolved` map
- **Description**: An alias whose match conditions reference a sibling alias
  filled earlier in the *same* refresh sees the stale table, so it evaluates
  against last refresh's binding (or none, on the first). Self-corrects on the
  next refresh.
- **Impact**: A one-refresh lag on cross-alias conditional fills. Bounded because
  refreshes are frequent and the state converges.
- **Suggested Fix**: Evaluate `RunOn::QuestAlias` against the in-progress
  `resolved` map, falling back to the committed table.

#### SCR-D6-NEW11-07: `cleanup.rs`'s stated drain contract contradicts the 10 markers that legitimately self-drain

- **Severity**: LOW · **Dimension**: Scripting Runtime Systems ·
  **Untrusted-Input**: No · **Status**: NEW
- **Location**: `crates/scripting/src/cleanup.rs` module doc; the rule it
  contradicts exists only as prose in `byroredux/src/save_io.rs`
- **Description**: The module doc states every transient marker is drained by
  `event_cleanup_system`. Ten markers legitimately self-drain unconditionally at
  the head of their own consumer (verified individually to have no early return
  before the drain). The real house rule — "drain at the head of your consumer,
  or register with cleanup" — is written down nowhere authoritative.
- **Impact**: Documentation/contract clarity. A future marker author reading
  `cleanup.rs` concludes registration is mandatory, or reading a self-draining
  consumer concludes it is optional, with nothing adjudicating.
- **Related**: #2270 (the undocumented "snapshot before iterate" house rule —
  same class)
- **Suggested Fix**: State both sanctioned patterns in `cleanup.rs`'s module doc
  and list which markers use which; fold into #2270's documentation sweep.

---

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#2542** (`feature-matrix.md` decompiler pass order) — **still OPEN and still
  unfixed**; re-confirmed at the drifted line `docs/feature-matrix.md:157`
  (`CFG→lift→control-flow→lower→short-circuit`, booleans last). The real order is
  `CFG→lift→short-circuit→control-flow→lower`. Reported here as
  SCR-D3-NEW11-03 for completeness; **do not open a second issue**.
- **#2541** (no test pins the `is_primary_synth` gate) — still OPEN. Re-verified
  this pass: **all 10 call sites are correctly gated** (grew from 9 as the file
  gained 440 lines), so it remains a pure test-coverage gap, not a live defect.
  Not re-filed. SCR-D7-NEW11-03 documents the un-covered eleventh, hand-rolled
  site in `exterior.rs`.
- **#2289** (decline-path test coverage on newer effect primitives) — still OPEN,
  unchanged; no new effect primitives landed this window. Not re-filed.
- **#2290** (`translate/source.rs` doc-rot) — still OPEN, unchanged.
- **#2270** (undocumented "snapshot before iterate" house rule) — still OPEN;
  SCR-D6-NEW11-03 and SCR-D6-NEW11-07 both point at it.
- **#2408** (`scene.rs` crossed 2000 LOC) — tech-debt-owned, not re-filed; its
  diffuse growth is why the marker-drain and lock-nesting sweeps were redone from
  scratch this pass rather than assumed.
- **#2432** (`an_unrecognized_pex_is_a_silent_miss` asserts nothing) — still
  OPEN, tech-debt-owned.
- **#2269 / #2539 / #2538 / #2286 / SAVE-D6-01** — all CLOSED; each independently
  re-verified this pass (verdicts below).

## Verdicts on the four closed fixes

| Fix | Verdict |
|---|---|
| **#2269** (`dc9ba0e5`, cinematic deferral) | **Genuinely fixed.** Deferral is real, not relocated; both cinematic mutations reach `apply()` after the guards drop in all three callers. No lost effect (the one in-scope `return` precedes any queueing), no unbounded queue, no new inversion. It did introduce SCR-D6-NEW11-02. |
| **#2539** (`6ad64ef6`, lock isolation) | **Fixed exactly as scoped; isolation NOT complete.** The two named resources are correctly snapshot/deferred, but `SceneActorBindings` is still read-acquired in-scope and 5 other resources + 12 components remain nested → SCR-D6-NEW11-03. |
| **#2538** (`90ae915c`, `Quest.Start()` mis-lowering) | **Fix is inert on real input, and the original finding overstated its impact.** The guard's key space never matches decompiled `.pex` (`::MQ101_var` vs `mq101`) → SCR-D5-NEW11-01. Separately, a dispatch-time fallback added in the *same* commit that created the ambiguity already made the runtime symptom self-correcting, so the prior report's "the quest silently never starts" was wrong at filing time. |
| **SAVE-D6-01** (`c4c30afd`, alias ledger `EntityId`) | **FIXED, correctly, with a real test.** Ledger rekeyed to the stable authored `reference_form_id`; `quest_alias_inventory_grant_ledger_survives_live_reload_with_new_entity_id` drives the id-churn shape with a decoy spawn + `assert_ne!`. Also disproved a suspected sibling leak: alias-injected faction membership does **not** survive save/load (`factions` is `serde(skip)` **and** `FactionRanks` is `REDERIVED_NOT_SAVED`). Residual nit filed as SCR-D6-NEW11-05. |

## Findings Count

**20 new: 0 CRITICAL / 2 HIGH / 10 MEDIUM / 8 LOW**, plus #2542 re-confirmed
unfixed.

By dimension: Dim 1 — 1 LOW. Dim 2 — 1 LOW. Dim 3 — 1 MEDIUM + 1 LOW (+#2542).
Dim 4 — 1 MEDIUM + 1 LOW. Dim 5 — 1 HIGH + 2 MEDIUM + 1 LOW. Dim 6 — 1 HIGH +
3 MEDIUM + 3 LOW. Dim 7 — 3 MEDIUM.

**Dimensions 1–4 broke a three-pass zero-finding streak over unchanged code.**
That is the pass's methodological headline: the findings came from diffing
against upstream Champollion, probing with pathological CFGs, and running the
corpus harness — not from re-reading Rust that three prior passes had already
read.

## Future-Phase Readiness

- **SCR-D5-NEW11-02 (`Reset`/`SetActive` collision, HIGH)** — highest priority.
  The narrow-receiver gate is mechanical; the durable fix is making the
  base-script method-name sweep a checked-in gate so the *next* primitive is
  validated against the real Papyrus API rather than its table siblings.
- **SCR-D6-NEW11-01 (`Activate` marker ordering, HIGH)** — a scheduling
  reorder or a route through the existing deferred queue. Cheap; the expensive
  part is the regression test that actually runs the consumer.
- **SCR-D3-NEW11-01 (boolean loop erasure)** — the one-line edge check is
  verified against every existing shape. Ship it with the loop stream as a
  regression test, and fix `boolean.rs`'s citation of the corpus rate.
- **The executing-fidelity-gate gap** — the structural enabler for both
  SCR-D3-NEW11-01 and SCR-D5-NEW11-01 is that the `.pex` side of the fidelity
  gate is `#[ignore]`-gated on game data and never runs. Consider checking in a
  small synthetic `.pex` fixture so at least one end-to-end decompile→recognize
  assertion executes in a default `cargo test`.
- **SCR-D7-NEW11-01 / -02 (VMAD populations unreachable)** — together these are
  the largest remaining reach gap in the attach path: 805 `NPC_` + 822 `ACHR` +
  42 base records on Skyrim, 382 + 516 + 442 on FO4. Both fixes are additive and
  mirror the already-shipped #2189 pattern.
- **SCR-D6-NEW11-02 (per-frame registry clone)** — move the bail ahead of the
  clone, or switch to an `Arc` snapshot. Load-time-only writers make the `Arc`
  sound.
- **M47.3 Phase 4+** — unchanged and correctly out of scope, with one
  correction: reference-collection aliases are *not* cleanly declining today
  (SCR-D6-NEW11-04); they half-work, which is worse than the documented
  deferral.
- **Condition resolvers** — unchanged guidance: unit-test-clean (19 catalog
  functions), still not re-verified against a live headless cell with real CTDA
  data.
- **Obscript/SCTX (Phase 5)** — unchanged, not built, correctly out of scope.
- **For the next pass**: `crates/pex/` and `crates/papyrus/` have now gone four
  passes without a functional change, but this pass shows "unchanged code" is not
  the same as "audited code" — the productive move was a new instrument, not
  another read. If those two crates remain frozen, the next high-yield probes are
  a differential fuzzer against Champollion's own output and a checked-in
  synthetic fidelity fixture, rather than a fifth re-read.

---
*Eleventh pass over this domain, run 2026-08-12 across 7 dimension agents
(max 3 concurrent). Dedup baseline: `gh issue list --repo
matiaszanolli/ByroRedux` (226 open) + per-issue state checks + direct
verification against `docs/audits/AUDIT_SCRIPTING_2026-08-07.md`. All four
prior-pass fixes (#2269/#2538/#2539/SAVE-D6-01) independently re-verified;
two are complete, one is partial, one is inert. Working tree verified clean
after all dimension probes.*
