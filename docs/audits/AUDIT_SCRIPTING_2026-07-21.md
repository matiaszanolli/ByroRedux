# Scripting Subsystem Audit — 2026-07-21

Seventh full pass over the M30/M47 Papyrus/.pex/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`, `_07-03.md`,
`_07-06.md`, `_07-16.md`). Seven dimensions ran as parallel agents against
`crates/pex/`, `crates/papyrus/`, `crates/scripting/`, and the engine-side
attach path (`byroredux/src/cell_loader/`, `byroredux/src/asset_provider/`).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux` (54 open
issues at audit time) plus the 2026-07-16 report. All seven findings from that
report were confirmed already filed and **closed** as issues #2023–#2029
before this pass began (verified via `gh issue list --state closed --search`,
not assumed) — every dimension agent was briefed to re-verify each fix against
*current* code rather than trust the prior report's prose, and to treat any
regression as a fresh finding.

This pass also lands on the same day as three feature commits
(`f967aa0f`/`a2bdbab8`/`0e580eeb` — QUST ALST/ALLS alias decode,
`97bc3b94`/`ece712ba` — `AddItem`/`MoveTo` object-targeting effects and quest
VMAD scripts-section wiring). Two of this pass's new findings
(SCR-D6-NEW3-04/05) are doc-rot introduced by those very commits — the same
"code shipped, docs lag" pattern #2029 fixed one file over, recurring in a
sibling file the fix didn't touch.

## Executive Summary

**What shipped** (all re-confirmed live, no regressions): M30.2 `.psc`
lexer/parser; M47.0 event-hook runtime; M47.1 condition evaluation (all 13
catalog functions implemented, correct safe-default sentinels); M47.2 `.pex`
reader + 5-phase decompiler + recognizer chain + dynamic VMAD attach path +
XPRM trigger volumes + the fragment-lowerer wired-and-live-verified dispatch
+ the QUST VMAD property-table fix + the `AddItem`/`MoveTo` object-targeting
effects (all landed 2026-07-21, this session); M47.3 Phase 0 QUST alias
(`ALST`/`ALLS`) decode (`crates/plugin`, out of this skill's crate scope but
noted for context).

**Deferred** (correctly, not flagged as defects): Obscript/SCTX frontend
(Phase 5); the M47.1 condition resolvers' live-headless-cell re-verification;
M47.3 quest-alias-fill runtime (the `Property`-resolution decline on an
alias-bound VMAD entry remains correct-by-design, not a bug).

**Findings this pass**: 9 new (0 CRITICAL / 1 HIGH / 4 MEDIUM / 4 LOW). All 7
prior findings (#2023–#2029) independently re-confirmed **fixed** across
Dimensions 1–7 — several re-verified by actually executing the fix against
constructed adversarial input (Dim 2's O(n²) perf regression test, Dim 4's
scratch-crate reproduction of the lex-error isolation fix, Dim 7's SCOL/PKIN
symmetry trace), not just reading the diff. Two still-open, correctly-tracked
issues (#1743, #1769) re-confirmed accurate — not re-filed.

**Untrusted-input robustness verdict — ALMOST CLEAN, one new caveat.** No
panic/OOB/unbounded-alloc was found anywhere in the reader, lexer, or parser
(Dims 1 and 4 both executed adversarial inputs directly, not just read code).
However, Dimension 2 found a genuine **wrong-AST** defect (D2-01, HIGH): a
hand-crafted `.pex` with a backward `jmpf`/`jmpt` target landing inside its
own originating block corrupts the CFG's conditional-branch structure —
attaching the condition/edges to a stale block and silently dropping a loop's
back-edge. This is **not reachable by real Bethesda-compiled `.pex`** (compiler
output only emits forward `jmpf`/`jmpt` targets; backward is always a plain
`jmp`, confirmed by the 99.996% corpus's shape survey) — it is a hardening gap
against adversarial/corrupted input on the live, synchronous VMAD-attach path,
not a live bug against real game or mod content today.

**The 99.996% (26640/26641) decompile-rate claim — VERIFIED HONEST**
(re-confirmed independently by two dimensions this pass, Dim 1 against the
reader's zero-panic parse of all 26641 real corpus files, Dim 3 against the
decompile tally itself: `catch_unwind`-wrapped, panics and `Err`s both counted
as failures, no swallowing/inflation).

**The `.psc`-vs-`.pex` fidelity gate — VERIFIED, both sides still closed.**
`recognizes_da10_and_reproduces_hand_builder` (`.psc`-side) re-run directly
this pass (`cargo test -p byroredux-scripting --lib`, passes);
`da10_pex_reproduces_hand_builder_byte_for_byte` (`.pex`-side,
`crates/scripting/tests/pex_recognize_e2e.rs`, `#[ignore]`-gated on Skyrim SE
data, #1740) confirmed still present. Coverage remains single-quest-predicate
only, unchanged from prior passes.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`crates/pex/src/reader.rs`) | Yes — `take()` sole gate, no bypass | Yes — var-arg loop never `with_capacity(hostile_n)` | Yes | Yes, 3 dialects round-trip; 26641/26641 real corpus zero-panic |
| CFG (`cfg.rs`) | Yes — inclusive jump-target bound correct, `split(0)` structurally unreachable | Yes | Yes, no panic | **Wrong-AST bug found** (D2-01, HIGH) — a backward `jmpf`/`jmpt` target landing inside its own block corrupts which block gets the condition/edges; adversarial-input-only, not reachable from real compiler output |
| Lift + copy-prop (`lift.rs`) | Yes | Yes — **#2024's O(n²) fix genuinely verified**: doubly-linked live-index chain, O(1) fold removal, single O(n) compaction; perf regression test re-run and passes | Yes | Yes |
| Boolean (`boolean.rs`) | Yes | Yes — re-process loop strictly shrinks block count, `MAX_REBUILD_DEPTH=1024` caps recursion (re-verified, both `boolean.rs` and `control_flow.rs` caps present and tested) | Yes — `operand_key==rejoin_key` degenerate case (#2028) still absorbed by control_flow's fail-closed catch-all, not a panic | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same recursion cap | Yes — fails closed (`ControlFlowFailed`) on the `||`-skip case (#1732), re-confirmed | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes — every `NodeKind` matched; `lower_binary_op` default-arm-to-`Eq` re-confirmed structurally unreachable | Yes |

The `.pex`→AST pipeline's bounds-safety and totality stories are unchanged
from the prior pass and remain sound. The **new** item this pass is D2-01,
which is a correctness (wrong-AST) finding in the CFG-construction stage,
distinct in kind from the prior pass's O(n²) DoS finding in the same
neighborhood (`lift.rs`, one file over) — see Findings below.

## Decline-Invariant Audit

The recognizer-chain decline invariant (`crates/scripting/src/translate/`)
remains **sound, no new leaks found**. Chain ordering, `split_and`'s
disjunction-stays-whole guarantee, all `QuestRef`/`ObjectRef` hole-binding
decline paths (including the local-alias-copy decline for object receivers),
`CanonicalEvent`'s safe long-tail bucket, the `AddItem`/`MoveTo` conservative-
arity declines, and the `translate_pex`/`populate_quest_fragments_from_pex`
panic-catching parity all re-verified clean this pass — most notably
**#2023/SCR-D5-NEW2-01** (`bool_arg`'s `Option<Option<bool>>` contract
distinguishing "argument absent" from "argument present but non-literal") is
confirmed fixed and load-bearing on the now-live `AddItem`/`MoveTo` dispatch
path. One test-coverage gap noted (not a defect): no fixture exercises
`resolve_property_form_id` with an `alias` value other than `-1`, so the
correct-by-design alias-bound decline branch is currently unpinned by a
regression test.

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage (12 transient types) | CLEAN — re-confirmed against every `insert` site |
| Two-phase lock-drop (`timer_tick_system`, `trigger_detection_system`, `recurring_update_tick_system`, `quest_advance_system`) | CLEAN — no two component/resource-mut locks held concurrently |
| `quest_fragment_dispatch_system` nested-lock safety (NEW component-lock nesting from `AddItem`/`MoveTo`) | **Investigated in depth, disproven as a live deadlock** — every system touching the same quest resources is registered `add_exclusive` (strictly sequential), confirmed via the scheduler and `boot.rs` wiring. Downgraded from a suspected HIGH to a MEDIUM documentation/process gap (SCR-D6-NEW3-03) — the safety is real but incidental, undocumented at the point of risk, and unguarded by any test or compile-time assertion. |
| Cascade bound (`MAX_CASCADE=64`) | Bound itself CLEAN (WARN on overflow, present). The **"only genuine transitions cascade" guarantee is broken** — see SCR-D6-NEW3-02 (MEDIUM, new this pass). |
| Edge-trigger seed (`occupant_inside: Option<bool>`) | CLEAN, lazy-seed intact |
| CTDA OR-precedence | CLEAN, trailing-OR clamp + empty-list contract intact |
| Condition safe-default sentinels (#1663–#1668, #1316) | CLEAN, all correct — **except** `RunOn::Reference` never resolves at all (SCR-D6-NEW3-01, MEDIUM, new this pass) — a "never succeeds" gap, not a "wrongly defaults" gap, so the decline-over-default contract itself is not violated |

## Findings

### HIGH

#### SCR-D2-NEW3-01: `build_cfg` attaches a `JmpF`/`JmpT` block's condition/edges to a stale block key when the jump target lands inside the same block being processed

- **Severity**: HIGH
- **Dimension**: Decompiler CFG & Lift
- **Untrusted-Input**: Yes — reachable directly from raw `.pex` bytes via `build_cfg`, on the live cell-loader VMAD-attach path (`translate_pex`). Not reachable from real Bethesda-compiler-generated `.pex` (see Impact).
- **Location**: `crates/pex/src/decompile/cfg.rs:213-243` (the `OpCode::JmpF | OpCode::JmpT` arm of `build_cfg`)
- **Status**: NEW
- **Description**: `block_key` is computed once at the top of each loop iteration (`cfg.rs:191`, `find_block_for_instruction(&blocks, ip)`), before either of this instruction's two `split_block` calls run. For `JmpF`/`JmpT`, both the fall-through split (`ip+1`) and the jump-target split (`target`) happen *before* the block's `condition`/`next`/`on_false` fields are set (`cfg.rs:224-242`). If `target` is a **backward** offset landing strictly between the current block's `begin` and `ip` (i.e. `begin < target < ip`), the target-split subdivides the *same* physical block the first split already shrank, a second time. `block_key` — computed before either split — now points at the leftover head piece, not the tail piece that actually contains instruction `ip`. The code writes `condition`/`next`/`on_false` onto the wrong (stale) block; the block that truly ends in the conditional jump is left with `condition: None`, silently losing its `on_false` (loop-back) edge.

  The sibling unconditional `OpCode::Jmp` arm does not have this bug: it sets `next` *before* performing the target-split (`cfg.rs:206`, ahead of the second split at `cfg.rs:208-211`); `CodeBlock::split` copies `self.next` into the new tail (`cfg.rs:82`), so the correct value propagates into whichever piece ends up containing `ip` even under a double-split. The `JmpF`/`JmpT` arm reorders this (both splits, *then* set fields) with no equivalent propagation trick, breaking it.
- **Evidence**: Reproduced by executing the actual code against a hand-built function (`0: assign x,y`; `1: assign z,w`; `2: jmpf t,-1` [target=1, `begin=0 < target=1 < ip=2`]; `3: return x`). Actual `build_cfg` output: block 0 (spans only instruction 0, unrelated to the jump) is marked `cond=Some("t")`, `on_false=1`; block 1 (which actually contains the real `jmpf` at instruction 2) is left `cond=None` with no `on_false` edge — the backward loop-edge has vanished. Reproduction was via a temporary in-tree test, run, then fully reverted (`git status` clean).
- **Impact**: A later pass (`control_flow::reconstruct`) would build an `If`/`While` gated on `t` around the *wrong* statement, while the block that really contains the loop test degenerates into unconditional straight-line code — the loop's back-edge is dropped entirely, silently changing the decompiled AST's control flow ("decompiler emits wrong AST" — this domain's HIGH-minimum bar). **Not reachable by real Bethesda-compiled `.pex`**: every existing CFG/boolean/control-flow test fixture shows `jmpf`/`jmpt` targets are always forward in real compiler output (backward is always a plain `jmp`), consistent with the 99.996% clean corpus decompile rate — this is purely a hardening gap against a hand-crafted or corrupted `.pex` (a hostile mod's VMAD script, or bit-flip corruption) reaching the live, synchronous cell-loader attach path.
- **Suggested Fix**: After performing both splits, don't reuse the possibly-stale `block_key` — re-resolve the block that actually contains `ip` via `find_block_for_instruction(&blocks, ip)` a second time (after both splits have settled) and write `condition`/`next`/`on_false` onto *that* key. Robust regardless of split ordering. Add a regression test (the repro above, plus a symmetric `jmpt` variant) asserting the block spanning `ip` is the one left conditional and that no block loses its `on_false` edge when a `jmpf`/`jmpt` target is backward-and-interior to its own originating block.

### MEDIUM

#### SCR-D6-NEW3-01: `RunOn::Reference` conditions always evaluate false — the resolver exists in the same file but is never called

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: Yes — driven by real CTDA data (any condition authored "Run on: Reference" in the Creation Kit).
- **Location**: `crates/scripting/src/condition.rs:258-268` (`ConditionContext::resolve`, `RunOn::Reference` arm)
- **Status**: NEW
- **Description**: The `RunOn::Reference` arm unconditionally returns `None` with a comment claiming the FormID→EntityId resolver "not yet wired." That resolver already exists in the same file — `resolve_entity_by_global_form_id` (`condition.rs:326-338`) — and is already used a few lines later in the `GetDistance` arm (`condition.rs:395`) to resolve `condition.param_1`, a FormID that goes through the identical parse-time remap as `condition.reference_form_id` (`crates/plugin/src/esm/records/condition.rs:359-360`). Since `evaluate_condition` returns `false` whenever `ctx.resolve()` returns `None`, every CTDA condition authored with `RunOn::Reference` silently and permanently evaluates false. It does not violate the decline-over-default contract (it never falls back to Subject), but it never succeeds either.
- **Evidence**: `condition.rs:258-268` (`RunOn::Reference => { ...; None }`) vs. the unused, working resolver at `condition.rs:326-338` and its proven-correct sibling call site at `condition.rs:395`.
- **Impact**: Any perk entry, dialogue INFO, quest stage, AI package, or magic-effect CTDA that targets a specific placed REFR (rather than Subject/Target/CombatTarget) never passes, silently gating off that branch of content on every game. No error above `trace` log level, so this is invisible without specifically instrumenting condition evaluation.
- **Suggested Fix**: `RunOn::Reference => resolve_entity_by_global_form_id(_world, condition.reference_form_id)` — drop the stale comment and the now-inaccurate unused-parameter naming. One-line fix; the resolver and remap plumbing are already correct and tested elsewhere.

#### SCR-D6-NEW3-02: Quest-fragment cascade's "genuine transition" guard compares against the wrong variable — can drop or duplicate cross-quest/multi-effect `SetStage` cascades

- **Severity**: MEDIUM (content-dependent; the failure modes below are silent missed content or duplicate item grants when specific stage-number/multi-effect shapes occur in real ESM data)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: Yes — driven by authored quest-stage numbers and fragment effect lists from real `.pex`/VMAD data.
- **Location**: `crates/scripting/src/fragment.rs:488-495` (`quest_fragment_dispatch_system`, cascade loop)
- **Status**: NEW
- **Description**: The cascade re-queue guard is `if adv.new_stage != stage { queue.push((adv.quest, adv.new_stage)); }`, where `stage` is the stage of the *currently-dispatching* fragment (from the outer `while let Some((quest, stage)) = queue.pop()`), not `adv.previous_stage` (the actual pre-image already carried on the event) and not scoped to `adv.quest == quest`. The doc comment's intent ("skip a no-op re-set of the same stage") is only correctly implemented for a fragment that re-sets its own currently-running stage — every other shape is a coincidental, wrong comparison:
  - **False negative**: quest A (dispatching at stage `S`) sets a *different* quest B to a stage number that happens to numerically equal A's own stage `S`. The guard compares `S != stage` where `stage == S` → false → B's genuine transition is silently never queued.
  - **False positive**: one fragment body issues two effects both resolving to the same `(quest, new_stage)`; the second's true no-op (`previous_stage == new_stage`) can still satisfy `adv.new_stage != stage` (comparing against the *original* dispatching stage) → that stage's fragment (e.g. an `AddItem`) re-runs a second time in the same cascade.
  - The correct check needs no outer-loop variables at all: `adv.previous_stage != adv.new_stage`.
- **Evidence**: `crates/scripting/src/fragment.rs:461-497` (full cascade loop); `crates/scripting/src/quest_stages.rs:112-118` (`set_stage` always returns the previous value and always inserts, even on a same-value re-set — the caller is the only place able to distinguish genuine vs. no-op). Existing tests (`dispatch_cascades_chained_set_stage`, `populate_from_script_binds_stages_to_the_right_fragments`) only exercise `adv.quest == quest` with one `SetStage` per fragment, where `stage` and `adv.previous_stage` coincide by construction — the bug is real but untested.
- **Impact**: Silent loss of a different quest's scripted side effects when stage numbers coincidentally collide across quests in the same cascade (plausible — quest stages cluster around small round numbers like 0/10/20 across many independently-authored quests) — or silent duplicate application of a stage's effects (duplicate item grants) when one fragment converges two effects on the same value. Both are content-correctness bugs with no crash and no log line.
- **Suggested Fix**: Replace `adv.new_stage != stage` with `adv.previous_stage != adv.new_stage`. Add regression tests for both the cross-quest stage-number collision case and the same-fragment double-`SetStage`-converging case, asserting the target fragment runs exactly once (or not at all, for the genuine no-op).

#### SCR-D4-NEW3-01: A parser-level error in one function inside a `State`/`Group`/`Struct` discards the entire container, not just the offending item

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Yes (latent — no live `.psc`/SCTX caller today; will matter once a real frontend feeds this parser)
- **Location**: `crates/papyrus/src/parser/script.rs:509-551` (`parse_state`), `:556-576` (`parse_struct`), `:579-619` (`parse_group`) — each parses its children with a bare `?` and no per-item catch; recovery only happens one level up, at `parse_script`'s top-level loop (`script.rs:77-85`)
- **Status**: NEW
- **Description**: `parse_script`'s top-level loop is the only place that catches a parser `Err` and recovers (`push_error` + `skip_to_next_line`). `parse_state`/`parse_group`/`parse_struct` parse their own children (functions/events, properties, members) with a bare `?`, so a syntax error in, say, the third function of a `State` block propagates all the way up and the **entire `ScriptItem::State`** — including functions before the error that parsed perfectly — is discarded, not returned as a partial `State` with just the bad function dropped. This is the same bug shape as the just-fixed #2025/SCR-D4-NEW2-01 (whole-file failure on one bad token), one container level deeper — the lex-level fix doesn't cover this parser-level gap.
- **Evidence**: Built a standalone scratch crate depending on `byroredux-papyrus` and ran `parse_script` on a 3-function `State` block where only the middle function has a genuine parser error (`int x = )`). Result: `ScriptItem::State("MyState")` never appears in the AST at all — the first function (zero errors) is gone entirely, and the third function survives only by accident (line-by-line resync happens to land on its `Function` keyword, re-parsing it as a **top-level** function outside the state, structurally wrong). Contrast with the lex-level fix (#2025), verified in the same session to isolate damage correctly even for a nested-`If`-inside-`Function` shape.
- **Impact**: For any script using `State` blocks (an idiomatic, common Papyrus pattern — the project's own `parse_full_rumble_on_activate_translation` fixture has three), one error in one state's function silently drops every other function in that state, with only cascading "unexpected token" noise pointing at it — no explicit "State X was dropped" diagnostic. The one function that resyncs onto a top-level position is also re-parented outside its `State`, which would corrupt a downstream state-membership-keyed recognizer. `parse_group`/`parse_struct` share the identical code shape (bare `?`, no catch) and are presumed to have the same gap.
- **Suggested Fix**: Give `parse_state`/`parse_group`/`parse_struct` their own per-child recovery loop mirroring the top-level one in `parse_script` — same fix shape as #1734/SCR-D4-02, one level deeper. Add a regression test, e.g. `parser_error_in_one_state_function_does_not_drop_sibling_functions_or_the_state`.

#### SCR-D6-NEW3-03: Fragment-dispatch's new nested-lock safety depends entirely on undocumented scheduler wiring in a different crate

- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No (structural/concurrency hygiene)
- **Location**: `crates/scripting/src/fragment.rs:180-236` (nested lock acquisition in `apply_effect`), `byroredux/src/boot.rs:572-608` (the only place the safety invariant is stated, and it predates the newer lock surface)
- **Status**: NEW
- **Description**: `apply_effect`'s `AddItem`/`MoveTo` arms (added this session) acquire `Inventory`/`GlobalTransform`/`Transform` component locks while the caller (`quest_fragment_dispatch_system`) still holds `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState` resource locks for the whole cascade loop. Investigated in depth — **not a live deadlock today**: the scheduler (`crates/core/src/ecs/scheduler.rs:458-495`) runs parallel systems first, then exclusive systems strictly sequentially, and every system touching these quest resources (`quest_fragment_dispatch`, `quest_advance_dispatch`, the demo dispatchers) is registered `add_exclusive` in `byroredux/src/boot.rs` — so no concurrent holder can ever form the other half of an ABBA cycle. But this safety property is enforced entirely by scheduler wiring in a different crate; `fragment.rs` itself has zero mention of "exclusive"/"parallel"/"Stage::" (confirmed by grep), and the `boot.rs` comment that does state the rationale predates (and doesn't account for) the newer component-lock nesting the `AddItem`/`MoveTo` effects introduced.
- **Evidence**: `crates/core/src/ecs/scheduler.rs:477-494` (parallel-then-sequential-exclusive per stage); `byroredux/src/boot.rs:580-582,608` (`add_exclusive` registration); zero hits for `grep -n "exclusive\|parallel\|scheduler\|Stage::" crates/scripting/src/fragment.rs`. No test or compile-time assertion pins `quest_fragment_dispatch_system` to the exclusive lane — every fragment test builds a bare `World` and calls the system function directly, never through the real scheduler, so an `add_to` vs. `add_exclusive` typo would pass every existing test.
- **Impact**: No live bug today. But the next contributor who parallelizes this system (a stated follow-up plan) or adds another object-targeting effect with its own component lock has no local signal in `fragment.rs` that doing so requires re-deriving this whole analysis. If it regresses, the failure mode is a genuine cross-thread ABBA deadlock (process hang) — HIGH once it happens.
- **Suggested Fix**: Add a doc comment directly on `apply_effect`/`quest_fragment_dispatch_system` stating the exclusive-scheduling dependency and listing every lock type it nests, not just the 3 resources. Optionally add a scheduler-level assertion/test that fails if this system is ever registered via `add_to`/`add_to_with_access` instead of `add_exclusive`.

### LOW

#### SCR-D1-NEW2-01: `metadata_matches_champollion` spot-checks only 7 of 51 opcodes

- **Severity**: LOW
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: No (test-coverage gap, not a live-data path)
- **Location**: `crates/pex/src/opcode.rs:177-193` (test), table at `:73-125`
- **Status**: NEW
- **Description**: The test individually asserts `arg_count()`/`has_varargs()` for only 7 of 51 `OPCODES` rows (`Nop`, `IAdd`, `CallMethod`, `CallParent`, `ArrayGetAllMatchingStructs`, `LockGuards`, `TryLockGuards`, plus one `.name()` check). `array_findstruct = 5` (explicitly called out by this audit's checklist) and 43 other rows (`struct_get`/`struct_set`, `propget`/`propset`, `jmpt`/`jmpf`, `cmp_lte`/`cmp_gte`, `unlock_guards`, …) have no direct unit-test pin, though the table itself was manually diffed against expected Champollion values and found correct, and the 26640/26641 real-corpus decompile rate is strong indirect corroboration.
- **Impact**: None today — the table is currently correct. A future edit to an untested row (reordering, a typo in an arg-count digit) would not be caught by `cargo test`; it would only surface as a silent instruction-stream desync on whichever real `.pex` files use that opcode.
- **Suggested Fix**: Extend the test (or add a sibling) to iterate the full 51-row table against a literal expected array, so any future edit is caught at compile-test time.

#### SCR-D6-NEW3-04: `quest_stages.rs` module header still describes fragment dispatch as future work

- **Severity**: LOW (doc-rot)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/quest_stages.rs:20-36`
- **Status**: NEW (same rot pattern as #2029, introduced in a sibling file that fix didn't touch)
- **Description**: The "What's deliberately NOT here yet" doc section still describes stage-fragment dispatch as "future work" whose loop "stays future work," even though `quest_fragment_dispatch_system` has shipped and is live-scheduled — `fragment.rs`'s own header was correctly updated in the #2029 fix, but this sibling module's header was not.
- **Impact**: Cosmetic only. A maintainer skimming `quest_stages.rs` first (a plausible, more-foundational entry point) would incorrectly believe fragment dispatch doesn't exist.
- **Suggested Fix**: Update the bullet to point at the now-shipped `fragment::quest_fragment_dispatch_system`, mirroring the language already fixed in `fragment.rs`'s header.

#### SCR-D6-NEW3-05: `quest_fragment_dispatch_system`'s own doc comment claims `MoveTo` still declines at the lowering stage — it doesn't anymore

- **Severity**: LOW (doc-rot)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:432-435`
- **Status**: NEW (introduced by this session's own feature commits, postdates the #2029 fix)
- **Description**: The doc comment claims object-targeting effects "still decline at the lowering stage," naming `MoveTo` as an example — but `translate/effects.rs` now lowers both `AddItem` and `MoveTo` call shapes into real `Effect` variants, and `apply_effect` applies both directly against the live ECS world (added in this same session, `97bc3b94`).
- **Impact**: Cosmetic — a maintainer reading only this comment would incorrectly believe `MoveTo` fragments are inert, when they mutate live `Transform` state.
- **Suggested Fix**: Narrow the sentence to the effects that are actually still gapped (e.g. `Enable`/`Disable`) and drop `MoveTo`/`AddItem` from the "still decline" list.

#### SCR-D7-NEW3-01: `quest_advance_system`'s "one signal per entity per frame" assumption is unenforced, currently true only by coincidence of two independent facts

- **Severity**: LOW (informational — not reachable today, no fix required)
- **Dimension**: Engine Attach Path & Trigger-Volume Wiring
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs:235-335`
- **Status**: NEW
- **Description**: `quest_advance_system` collects `(entity, activator/triggerer)` pairs from both `ActivateEvent` and `OnTriggerEnterEvent`, implicitly assuming a given entity never receives both in the same frame. This holds today only because (1) a `TriggerVolume` is only ever attached to a mesh-less REFR, so the mesh-bearing/mesh-less component sets are disjoint by construction, and (2) `ActivateEvent` has no live automatic emitter yet (only a debug console command) — the real "player activates a REFR" system (`boot.rs`'s "Stage 4") is unbuilt. The recognizer test `on_activate_wins_over_on_trigger_enter` proves a single script can legitimately define both handlers, so once Stage 4 lands, if it doesn't explicitly exclude `TriggerVolume`-bearing entities from activation eligibility, the disjointness assumption breaks and a single player action could double-fire `QuestStageAdvanced` (idempotent for the stage value, but a genuine double-application risk for a non-idempotent fragment effect like `AddItem`).
- **Impact**: None today — both preconditions independently hold. Purely forward-looking.
- **Suggested Fix**: No code change needed now. When Stage 4 lands, either exclude `TriggerVolume`-bearing entities from activation eligibility, or add a per-frame per-entity dedup in `quest_advance_system`. A cheap regression test for that future work: insert both event types on the same entity in one frame, assert exactly one `QuestStageAdvanced` marker results.

## Confirmed-fixed prior-audit findings (re-verified in place, no regression)

**From the 2026-07-16 report, all seven confirmed CLOSED via `gh issue list` and independently re-verified sound in current code (not just present)**:

- **#2023**/SCR-D5-NEW2-01 (`SetObjective*`/`AddItem` `bool_arg` present-non-literal collapse) — `bool_arg`'s `Option<Option<bool>>` contract confirmed intact and now load-bearing on the live `AddItem`/`MoveTo` dispatch path.
- **#2024**/SCR-D2-NEW-01 (O(n²) copy-propagation DoS) — genuinely fixed via a doubly-linked live-index chain (O(1) fold removal, O(n) compaction); traced for correctness (not just perf) and re-run against the 65535-instruction perf regression test.
- **#2025**/SCR-D4-NEW2-01 (whole-file lex-error failure) — genuinely fixed via a synthetic placeholder token keeping the stream contiguous; verified by direct execution against a scratch crate, and found to generalize *further* than its own regression test proves (nested-`If`-inside-`Function` case also isolates correctly).
- **#2026**/SCR-D7-NEW2-01 (SCOL/PKIN VMAD replication) — genuinely and *symmetrically* fixed (both expanders feed one shared gated loop, structurally impossible to be asymmetric); dedicated regression test passes.
- **#2027**/SCR-D1-NEW-01 (`PexError` variant coverage) — all four previously-uncovered variants now have dedicated reject tests.
- **#2028**/SCR-D3-NEW-01 (`operand_key == rejoin_key` degenerate collapse) — still declines/absorbed by the fail-closed catch-all, test passes.
- **#2029**/SCR-D6-NEW2-01 (fragment-dispatch doc-rot) — the three originally-flagged doc sites (`fragment.rs`, `boot.rs`, `script_instance.rs`) are all confirmed correctly updated. (Two **new** instances of the same rot pattern, introduced by commits *after* #2029 closed, are filed above as SCR-D6-NEW3-04/05.)

**From all prior reports, still intact with zero drift**: #1710/#1728 (reader round-trips), #1732/#1815/#1816 (control-flow fail-closed, recursion caps, `catch_unwind`), #1719/#1740/#1766 (DA10 fidelity gate both sides), #1727/#1767/#1768/#1817 (marker drains, trailing-OR clamp, both-systems-scheduled, occupant_inside lazy seed), #1737/#1742/#1864 (per-REFR VMAD, trigger rotation frame, batched same-frame advances), #1663–#1668/#1316 (condition resolvers, safe-default sentinels — except see SCR-D6-NEW3-01, a distinct new gap in a function this pass newly examined more closely, not a regression of these).

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#1743** (SCR-D7-03) — `--scripts-bsa` override order is "first-listed wins." Re-verified against current `asset_provider/script.rs`; description still accurate. Still open.
- **#1769** (D7-NEW-01) — VMAD attach dedup `HashSet<&str>` is case-sensitive. Re-verified against current `attach.rs:338`; description still accurate. Still open.

## Future-Phase Readiness

- **The CFG-construction correctness gap (SCR-D2-NEW3-01)**: the first *correctness* (not perf, not bounds) bug found in `cfg.rs` across seven audit passes. Worth a hardening pass across the decompiler's block-splitting logic generally — the `Jmp` arm's "set field before second split" ordering trick is fragile precisely because it's implicit; a structural fix (re-resolve the block after both splits settle) removes the whole class rather than patching this one instance.
- **`RunOn::Reference` (SCR-D6-NEW3-01)**: a one-line fix using plumbing that already exists and is already proven correct elsewhere in the same file — the cheapest fix-to-impact ratio in this report. Should be prioritized independent of severity ranking.
- **Cascade correctness (SCR-D6-NEW3-02)**: worth fixing alongside SCR-D6-NEW3-01 since both are in `condition.rs`/`fragment.rs`'s hot dispatch path and both are silent, no-crash content bugs — exactly the class hardest to catch without a targeted repro. Recommend adding both suggested regression tests in the same PR.
- **Parser-level container recovery (SCR-D4-NEW3-01)**: same fix shape as the already-shipped #1734/#2025 fixes, one container level deeper (`State`/`Group`/`Struct`). Low urgency today (no live `.psc` caller) but should land before any Obscript/SCTX frontend work begins, since that would make this a live, frequently-triggered bug rather than a latent one — same guidance the 2026-07-16 report gave for SCR-D4-NEW2-01's lex-level sibling.
- **Nested-lock documentation debt (SCR-D6-NEW3-03)**: the safety is real today; the debt is purely that `fragment.rs` can't be safely modified in isolation from `boot.rs`'s scheduler wiring without an engineer re-deriving this analysis. Cheap to fix now (a doc comment + optional scheduler assertion); expensive to debug later (a hung process with no obvious link back to this file) if the scheduling assumption silently breaks during a future parallelization pass.
- **Doc-rot recurrence (SCR-D6-NEW3-04/05)**: the second time in two consecutive passes that a shipped-feature commit updates its own module's docs but misses a sibling file describing the same feature. Worth a lightweight habit (grep sibling files for "future work"/"pending"/"not yet wired" phrasing whenever a fix note says "shipped") rather than a code fix.
- **Player-activation Stage 4 (SCR-D7-NEW3-01)**: purely forward-looking — flagged so whoever implements the real "player activates a REFR" system sees this before wiring it, rather than discovering the double-fire risk after the fact.
- **Condition resolvers, live-cell re-verification**: unchanged guidance from prior passes — 27 passing unit tests, still not re-verified against a live headless cell with real CTDA data.
- **M47.3 quest-alias-fill runtime**: unchanged — the `Property`-resolution decline on an alias-bound VMAD entry remains correct-by-design, not a gap to close within this skill's scope.

---
*Dimension worksheets: `/tmp/audit/scripting/dim_{1..7}.md` (ephemeral, removed
after finalization). Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux`
(54 open issues at audit time) + `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`
+ direct `gh issue view`/`--state closed --search` confirmation that
#2023–#2029 are all closed.*
