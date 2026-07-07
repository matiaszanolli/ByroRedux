# Scripting Subsystem Audit — 2026-07-06

Domain: M30 `.psc` parser · M47.0 event hooks · M47.1 condition eval ·
M47.2 `.pex` reader + 5-phase decompiler + recognizer chain + dynamic
attach path + XPRM trigger volumes. ~17.9k LOC across `crates/pex`,
`crates/papyrus`, `crates/scripting`, plus the engine-side attach wiring.

Seven dimensions, deep depth. This is the **fifth** scripting pass (prior:
2026-06-23, -06-27, -07-02, -07-03). The skill's Phase-1 note that "no prior
scripting audit exists" is itself stale — the four prior reports were the
primary dedup source and almost every prior finding is now fixed in code.

## Executive Summary

### What shipped (verified live)
- **M30.2** `.psc` logos lexer + Pratt parser → shared `byroredux_papyrus` AST,
  with balanced expr/stmt recursion caps (#1270 / #1712) and progress-guaranteed
  error recovery (#1734).
- **M47.0** ECS event-hook runtime (markers, timers, cleanup) — every transient
  marker drained exactly once per frame; `event_cleanup_system` is the sole
  `Stage::Late` scripting system.
- **M47.1** CTDA condition evaluator with Bethesda inverted OR-precedence, the
  trailing-`or_next` OOB clamp (#1767), and real resolvers now shipped for
  GetActorValue/GetDistance/GetFactionRank/GetIsID/HasPerk (#1663–#1668, #1316
  all CLOSED) plus GetLevel/GetIsClass/GetIsRace/GetReputation.
- **M47.2** `.pex` reader (Champollion port) + 5-phase decompiler (CFG → lift +
  copy-prop → boolean-collapse → control-flow recon → AST lower) + the
  decline-on-unmodeled recognizer chain + the live cell-loader attach path +
  XPRM → `TriggerVolume` spawn. Both prior-unwired runtime systems
  (`quest_fragment_dispatch`, `recurring_update_tick_system`) are now scheduled
  (#1768).

### Deferred (correctly, not flagged as defects)
- **Obscript / `SCTX` frontend** (Oblivion/FO3/FNV) — `ScriptSource::Obscript` is
  a typed placeholder; the SCTX parser is M47.2 Phase 5, not built.
- **The wired fragment-lowerer dispatch (b2)** — `effects::lower_fragment` +
  `QuestStageFragments` + `quest_fragment_dispatch_system` exist and are
  unit-tested, but no `RECOGNIZERS` entry feeds a decompiled quest-fragment
  `.pex` into `lower_fragment` yet (the designed #1739 gap, documented in the
  `RECOGNIZERS` docstring). See SCR-D5-NEW-04 below — a soundness gap *inside*
  that unwired code, distinct from "it isn't wired."

### Findings by severity
| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH     | 1 | SCR-D5-NEW-03 |
| MEDIUM   | 2 | SCR-D4-NEW-01, SCR-D5-NEW-04 |
| LOW      | 2 | SCR-D4-NEW-02, SCR-D5-NEW-05 |

Dimensions 1 (`.pex` reader/opcode), 2 (CFG/lift), 3 (control-flow/boolean/lower),
6 (runtime systems) and 7 (engine attach) are **clean** — every checklist item
verified against live code and every named regression guard confirmed to exist
and cover its claim.

### Verdict 1 — Untrusted-input robustness (can a hostile/corrupt `.pex` or `.psc` panic, OOB, or OOM the cell loader?) — **CLEAN (NO)**
No crash/OOB/OOM path survives from raw `.pex` or `.psc` bytes:
- `.pex` reader: every primitive read funnels through `take()` (checked_add +
  `<= data.len()` → `UnexpectedEof`); no direct slice/`try_into().unwrap()`
  bypass. The `OpCode::from_u8` `transmute` is memory-safe — `#[repr(u8)]`,
  contiguous discriminants 0..=50, guard `>= MAX_OPCODE(51)`, pinned by
  `discriminants_match_on_disk_order` + `from_u8_round_trips_and_rejects_oob`.
  Var-arg / count `with_capacity` pre-allocation is bounded (SCR-D1-01 fix
  holds).
- Decompiler: jump-target bounds inclusive-checked; both `boolean.rs` and
  `control_flow.rs` `rebuild` cap recursion at `MAX_REBUILD_DEPTH=1024`
  (#1729, #1815); the `||`-skip now **fails closed** (`ControlFlowFailed`) rather
  than dropping a block.
- `.psc` parser: balanced `MAX_EXPR_DEPTH`/`MAX_STMT_DEPTH=256` caps; error
  recovery makes ≥1 token progress.
- Boundary: `translate_pex` degrades a decompiler `Err` **and** a `panic`
  (`catch_unwind`, #1816) to a clean `None`; the attach path is silent-miss on
  every missing-data branch.

Caveat (correctness, not robustness): SCR-D4-NEW-01 is a *wrong-AST* on the
`.psc` lexer, not a crash — and it is latent (the live attach path decompiles
`.pex` straight to AST, bypassing the lexer). Robustness verdict stands CLEAN.

### Verdict 2 — the 99.996 % (26640/26641) decompile-rate claim — **VERIFIED HONEST**
Independently re-read `crates/pex/examples/pex_corpus_smoke.rs`: it runs
`decompile_script` (not just `parse`) on every parseable `.pex` inside a real
`std::panic::catch_unwind(AssertUnwindSafe(...))`, tallying three buckets —
`decompiled_ok`, `decompiled_err` (`Ok(Err)`), `decompiled_panic` (`Err`). A
panic is counted as a failure, never swallowed as success. The rate's
denominator is *parseable* files (parse failures tracked separately) — the
correct framing for a decompile-success metric.

### Verdict 3 — the `.psc`-vs-`.pex` fidelity gate — **VERIFIED / BOTH SIDES CLOSED, but narrow**
Both halves pin byte-equality against the same `da10_main_door(0x0002_2f08)`
hand-builder:
- `.psc` side — `recognizes_da10_and_reproduces_hand_builder`
  (`quest_stage_gate.rs`);
- `.pex` side — `da10_pex_reproduces_hand_builder_byte_for_byte`
  (`crates/scripting/tests/pex_recognize_e2e.rs`, `#[ignore]`-gated on Skyrim SE
  data, #1740).
Both assert `owning_quest`, `target_stage`, `conditions.len()`, and per-condition
`function_index` / `param_1` / `param_2` / `comparand`. The gate is genuine.
**Blind spot:** DA10 (and every recognizer test) uses a *single-quest* predicate
set, so the gate does **not** exercise the multi-quest mis-attribution path —
see SCR-D5-NEW-03.

## Decompiler Soundness Matrix

| Pass | Bounds-safe? | Terminates? | Total (no panic)? | Fidelity-tested? | Notes |
|------|:---:|:---:|:---:|:---:|-------|
| reader (`reader.rs`/`opcode.rs`) | ✅ `take()` sole gate; transmute guarded | ✅ | ✅ | ✅ FO4/LE + Skyrim/BE + Starfield-guards round-trips (#1728) | — |
| cfg (`cfg.rs`) | ✅ inclusive jump bound; no `split(0)` | ✅ | ✅ | ✅ diamond/loop/bodyless guards | — |
| lift + copy-prop (`lift.rs`) | ✅ | ✅ | ✅ Cast-arm `unwrap` guarded by prior `matches!` | ✅ single-consumer fold guards | `is_final`/`is_temp_var` asymmetry intact |
| boolean (`boolean.rs`) | ✅ | ✅ each collapse shrinks graph; depth cap 1024 (#1815) | ✅ | ✅ + corpus/R5 | Documented Champollion departure (no debug-line guard) adjudicated **benign** — the one ambiguous shape compiles byte-identically for `a&&b` and the hand-written form, so the merge is semantically equivalent, not wrong. |
| control-flow (`control_flow.rs`) | ✅ `before_exit` degenerate → `fail()`; depth cap 1024 (#1729) | ✅ | ✅ | ✅ | Documented `||`-skip departure adjudicated **benign** — now **fails closed** (`ControlFlowFailed`, guard `conditional_predecessor_fails_closed`), never silently drops a block. |
| lower (`lower.rs`) | ✅ | ✅ | ✅ | ✅ | `lower_binary_op` default `=> Eq` arm proven unreachable (lift/boolean emit only the modeled op set); `build_handler` prefix is char-safe `name.get(..2)` (#1732), no non-ASCII panic. |

## Decline-Invariant Audit

| Decline point | Verified conservative? | Notes |
|---------------|:---:|-------|
| `translate_script` all-`None` → silent miss | ✅ | `find_map`, first-match-wins, order = per-script (`rumble`) before generic (`quest_stage_gate`). |
| `classify_guard_atom(atom, player)?` per-atom loop | ✅ | `?` propagates `None` the instant an atom is unclaimed; loop drops nothing. |
| `split_and` keeps `||` whole | ✅ | disjunction left as one unmatched atom → forces decline (`If a\|\|b` declines). |
| `lower_fragment` `_ => return None` | ⚠️ | Control flow / valued return decline correctly, **but** a non-quest `VarDecl`/`Assign` *initializer* side-effect is silently dropped without declining — **SCR-D5-NEW-04** (latent, unwired). |
| hole binding never defaults to form-0 | ✅ | `OwningQuest`/`Property`/`SelfRef` each decline on unbound. |
| `quest_stage_gate` single-statement body invariant | ✅ | #1719 outer + inner-body enforcement both present (5 sibling-decline guards). |
| `quest_stage_gate` per-predicate quest agreement | ❌ | **SCR-D5-NEW-03** — only the *first* predicate's quest is kept + cross-checked; extra predicates' quests are discarded and silently retargeted. |
| `rumble` literal-only extraction | ⚠️ | Coerces a non-literal property to its `.psc` default instead of declining — **SCR-D5-NEW-05** (harmless; auto-property initializers are literal-only). |
| `translate_pex` bad-bytes / panic → `None` | ✅ | `catch_unwind` (#1816) present, not reverted. |
| `CanonicalEvent::Unknown` long-tail bucket | ✅ | Treated as "no consumer", never a wildcard match. |

## Runtime Lifecycle Invariant Matrix

| Invariant | Status | Evidence |
|-----------|:---:|----------|
| Marker drain coverage (all 12 transient types) | ✅ | `event_cleanup_system` drains every emitted marker; cross-checked against every `insert` site. Prior SCR-D6-01 (OnTriggerEnter) / -02 (OnCellLoad) both fixed. |
| `cleanup` is last scripting system | ✅ | Sole `Stage::Late` system (main.rs); all emitters run Early/Update — no lag, no re-fire. |
| Two-phase lock-drop (timer / trigger / recurring) | ✅ | Explicit `drop()` (timer, recurring) or lexical scope (trigger) before the second `query_mut`; no two component-mut locks held at once. |
| `quest_fragment_dispatch` resource-lock scoping | ✅ | Three resource locks in one scoped block, no component lock across them. |
| Cascade bound | ✅ | `MAX_CASCADE=64`, WARN on overflow; only genuine transitions cascade. |
| CTDA OR-precedence + trailing-`or_next` clamp | ✅ | `block_end_inclusive = i.min(len-1)` (#1767); dedicated panic-guard tests. |
| Edge-triggered trigger seed | ✅ | `occupant_inside: Option<bool>` lazy-seed (#1817) — no spurious frame-1 fire when loaded inside. |
| Quest stage history retained | ✅ | `set_stage` inserts into `stages_done`; `GetStageDone` stays true across advances. |
| RecurringUpdate arm/re-arm/overshoot | ✅ | No zero-dt fire, single fire per interval, clean in-handler unregister. |
| Both prior-unwired systems scheduled | ✅ | `quest_fragment_dispatch` + `recurring_update_tick_system` now in `Stage::Update` (#1768). |
| Same-frame `QuestStageAdvanced` batching | ✅ | Batched insert (#1864) — no single-entity sink collision. |

## Findings (grouped by severity, deduplicated)

### HIGH

#### SCR-D5-NEW-03: `quest_stage_gate` drops the per-predicate quest — a multi-quest `GetStageDone` gate is emitted with the extra predicate silently re-targeted to the wrong quest
- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: Yes (reachable via a decompiled `.pex` or a `.psc` REFR script)
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:272-297` (`classify_if_condition`), consumed at `:246-260` (`match_guarded_if`) and `:83-97` (`recognize`)
- **Status**: NEW
- **Description**: `classify_if_condition` runs each `&&`-split atom through `classify_guard_atom`, which returns a `GuardMatch::StageDone { via, stage, expected }` carrying the *per-predicate* quest reference (`via` = `Property(name)` or `OwningQuest`, from `compose::quest_via` on that predicate's receiver). But the loop keeps only the **first** atom's `via` via `quest_via.get_or_insert(via)` (line 292) and discards the rest, with **no check that all predicates name the same quest**. The downstream cross-check (`match_guarded_if:251`, `quest_via != set_via`) compares only that first `via` to the `SetStage` receiver. `recognize` then resolves the single `owning_quest` and stamps it onto `param_1` of **every** emitted `Condition` (`:91`), while `GetStageDone` evaluation reads `param_1` as the quest FormID. A mixed gate — e.g. `If Self.GetOwningQuest().GetStageDone(37)==1 && MyOtherQuest.GetStageDone(5)==1` with `Self.GetOwningQuest().SetStage(40)` — is therefore **emitted, not declined**, with the `MyOtherQuest` predicate silently retargeted to the owning quest (and `MyOtherQuest`'s VMAD FormID never even resolved). This is exactly the silent game-logic corruption the decline invariant exists to prevent.
- **Evidence**: `via` provably differs per atom — `compose.rs:184-196` (`as_get_stage_done` → `quest_via(object)`) returns `QuestRef::Property("MyQuest")` for a property receiver and `QuestRef::OwningQuest` for `Self.GetOwningQuest()`. `classify_if_condition:292` uses `get_or_insert`, which is a no-op after the first insert. No test in the file covers cross-quest predicates (all use a single quest), so the gap is untested.
- **Impact**: A quest-gate that predicates on another quest's progress advances on the *wrong* quest's stage — either firing when it shouldn't or never firing. Silent, no fallback to mask it. Narrow trigger (needs a multi-predicate AND referencing ≥2 distinct quests where the SetStage receiver equals the first predicate's quest), but cross-quest `GetStageDone` gates do occur in vanilla Bethesda scripts. Blast radius is all games the recognizer runs on.
- **Related**: The `.psc`-vs-`.pex` fidelity gate (DA10) uses same-quest predicates, so it does not catch this. Sits alongside the (fixed) single-statement-body invariant #1719.
- **Suggested Fix**: In `classify_if_condition`, replace `get_or_insert(via)` with a compare-or-decline: if `quest_via` is already set and the new atom's `via != quest_via`, `return None`. That preserves the existing behavior for same-quest gates (DA10) and declines the mixed-quest case rather than mis-attributing it.

### MEDIUM

#### SCR-D4-NEW-01: Int/float literal regexes swallow a leading `-`, silently dropping adjacent subtraction
- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Yes
- **Location**: `crates/papyrus/src/token.rs:228,231-232`
- **Status**: NEW
- **Description**: `IntLit`/`FloatLit` regexes carry an optional leading minus
  (`-?[0-9]+`, priority 2/3 — above the `Ident` regex's priority 1). Under logos
  longest-match, a `-` immediately followed by a digit (no intervening space) is
  eaten into the literal as a negative sign, even when it is binary subtraction.
  The Pratt loop only treats `Token::Minus` as an infix operator, so it then sees
  two adjacent value tokens, breaks, and the second operand is dropped with no
  diagnostic.
- **Evidence**: Live probes (both dim-4 runs): `lex("a-10")` → `[Ident("a"),
  IntLit(-10)]`; `parse_expr("5-3")` → `Ok(IntLit(5))` — the `-3` silently
  vanishes. Only whitespace around the `-` (`a - 10`) parses as subtraction.
  Common no-space idioms (`arr[len-1]`, `x = a-1`) mis-parse. Two existing tests
  (`test_negative_int_literal`, `test_lex_int_literals`) currently lock in the
  behavior.
- **Impact**: Wrong AST — a silent, non-crashing mis-parse of any adjacent
  subtraction. **Latent** in the live engine: the production attach path
  decompiles `.pex` straight to the AST and never touches this lexer; `parse_script`/
  `parse_expr` callers today are test-only over curated scripts. It becomes live
  the moment a `.psc` frontend feeds real source (the Obscript/SCTX Phase-5 work
  or any direct `.psc` ingest).
- **Related**: Divergence from the reference Papyrus compiler, where `a-10` is
  subtraction. Not HIGH — it terminates, no crash/DoS.
- **Suggested Fix**: Remove `-?` from both literal regexes and rely on the
  already-present unary-minus prefix path (`Token::Minus` → `Expr::Neg`). Update
  the two tests that assert the merged sign.

#### SCR-D5-NEW-04: `lower_fragment` silently drops a non-quest binding's side-effect instead of declining
- **Severity**: MEDIUM (latent; becomes HIGH when the fragment lowerer is wired)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: Yes (would consume decompiled fragment `.pex` once wired)
- **Location**: `crates/scripting/src/translate/effects.rs:113-157` (`lower_fragment` / `bind_local`)
- **Status**: NEW
- **Description**: A `Stmt::VarDecl`/`Stmt::Assign` whose initializer is a
  side-effecting **non-quest** expression (e.g. `ObjectReference k =
  akActor.PlaceAtMe(...)`) is routed through `bind_local`, which records the name
  in `decl_locals` and continues — producing **no effect and no decline**. The
  initializer's side-effect (the spawn) is silently dropped, yet a following
  `Self.SetStage(20)` is still lowered. This contradicts the function's own
  module doc (`effects.rs:36-37`: a non-quest binding "is itself an unmodeled
  statement → decline") and the flat-sequence decline contract. `decl_locals`
  only guards the later *use* site (an assignment to a field/index at `:133-135`),
  never the binding's own side-effect.
- **Evidence**: `lower_fragment:122-137` handles `VarDecl`/`Assign` via
  `bind_local`; `bind_local:151-156` inserts into `decl_locals` and returns
  without emitting or declining. Only `Stmt::ExprStmt` reaches `classify_effect`,
  so a side-effecting RHS on an assignment is never evaluated as an effect. No
  guard test covers a non-quest side-effecting initializer.
- **Impact**: **Not reachable today** — `lower_fragment` is the unwired Phase-3
  fragment lowerer (the designed #1739 gap; the `RECOGNIZERS` table has no
  fragment entry), so there is no live corruption. But it is a genuine leak of
  the decline invariant *inside the one function whose entire contract is that
  invariant*. When the QUST `VMAD` fragment decoder wires `lower_fragment` into
  the boundary, a quest fragment that spawns/does side-effect work before a
  `SetStage` would advance the quest while silently discarding the spawn — HIGH
  impact at that point.
- **Related**: Distinct from #1739 ("the lowerer isn't wired") — this is a
  soundness defect within the lowerer. Future-Phase-Readiness item for b2.
- **Suggested Fix**: In `bind_local` (or its callers), decline (`return None`)
  when the initializer is neither a quest expression nor a side-effect-free value
  — i.e. treat a non-quest *side-effecting* initializer as an unmodeled statement,
  matching the documented contract. Add a guard test.

### LOW

#### SCR-D4-NEW-02: Out-of-range integer/float literals silently become `0`
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Yes
- **Location**: `crates/papyrus/src/token.rs:79-93` (`parse_int`/`parse_float` → `unwrap_or(0)`)
- **Status**: NEW
- **Description**: A lexable-but-out-of-range literal (`0xFFFFFFFFFFFFFFFF`, a huge
  decimal) overflows `i64`/`f64` and, via `unwrap_or(0)`, silently becomes
  `IntLit(0)`/`FloatLit(0.0)` with no lex error.
- **Evidence**: Checklist item 6 (no panic) itself **passes** — this is silent
  wrong-value, not a crash. No diagnostic is surfaced.
- **Impact**: Cosmetic/rare; latent (same test-only `.psc` exposure as
  SCR-D4-NEW-01). A malformed literal reads as `0` rather than erroring.
- **Related**: Same lexer, same latency as SCR-D4-NEW-01.
- **Suggested Fix**: On parse overflow, emit a lex error (or a saturating value
  with a recorded diagnostic) rather than `unwrap_or(0)`.

#### SCR-D5-NEW-05: `rumble` recognizer coerces a non-literal property to its `.psc` default instead of declining
- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/recognizers/rumble.rs` (`float_prop`/`bool_prop` → `unwrap_or(default)`)
- **Status**: NEW
- **Description**: The checklist expects `rumble` to decline on a non-literal
  auto-property value; instead `float_prop`/`bool_prop` return `None` and the
  caller `unwrap_or`s the `.psc` default — a coercion, not a decline.
- **Evidence**: The extraction path falls back to the default rather than
  returning `None` from `recognize` when a property's initial value isn't a
  literal.
- **Impact**: Harmless in practice — Papyrus auto-property initializers are
  literal-only (so the branch effectively fires only on an *absent* property),
  and the five extracted values are cosmetic rumble/shake tuning that don't
  change the behavior family. Logged only because it diverges from the stated
  "must decline, not coerce."
- **Related**: A defensible design choice for a cosmetic per-script recognizer;
  noted for invariant consistency.
- **Suggested Fix**: Either decline on a present-but-non-literal property value,
  or update the recognizer's contract note to document the intentional
  default-fallback for cosmetic parameters.

### Existing / correctly-tracked (NOT re-filed — dedup)
- **#1743** — `--scripts-bsa` override order is "first-listed wins" (mod-over-vanilla
  would want later-wins). Open, correct; `asset_provider/script.rs`. Dim 7.
- **#1769** — VMAD attach dedup is case-sensitive; Papyrus names are
  case-insensitive. Open, correct; `cell_loader/references/attach.rs` raw `&str`
  HashSet. Dim 7.

### Confirmed-fixed prior-audit findings (re-verified in place, no regression)
SCR-D1-01 (var-arg pre-alloc), SCR-D1-02/#1728 (BE + guards round-trips),
D3-NEW-01/#1732 (char-safe handler prefix), SCR-D3-01/#1738 & SCR-D3-02 (doc-rot),
SCR-D2-01/#1815 & #1729 (boolean + control-flow recursion caps), SCR-D4-01/#1712
(stmt depth cap), SCR-D4-02/#1734 (recovery skips to next line not EOF),
SCR-D5-01/SCR-D5-NEW-01/#1719+#1766 (single-statement body, outer + inner),
SCR-D5-NEW-02/#1816 (catch_unwind), SCR-D5-03/#1740 (DA10 `.pex` parity),
SCR-D6-01/#1727 & SCR-D6-02 (marker drains), SCR-D6-NEW-01/#1767 (trailing-OR
clamp), SCR-D6-NEW-02/#1768 (both systems scheduled), SCR-D6-NEW-03 (Globals
symmetric rebuild), SCR-D6-NEW-04 & SCR-D6-NEW-02/#1817 (occupant_inside lazy
seed + doc), SCR-D7-01/#1737 (per-REFR VMAD resolved), SCR-D7-02/#1742 (trigger
rotation frame), SCR-D7-NEW-01/#1864 (batched same-frame advances).

## Future-Phase Readiness

Invariants this pass pinned for the not-yet-live work:
- **Fragment lowerer (b2, #1739)** — before wiring `lower_fragment` into
  `RECOGNIZERS`, close **SCR-D5-NEW-04** (a non-quest side-effecting binding must
  decline, not drop) and add its guard test. The flat-sequence control-flow /
  valued-return declines are already conservative; the binding side-effect is the
  one hole. The fidelity gate should gain a fragment-`.pex` → effect-spawn parity
  test paired with the wiring.
- **Obscript / SCTX frontend (Phase 5)** — if the SCTX parser reuses the
  `byroredux_papyrus` lexer or shares the literal-regex approach, fix
  **SCR-D4-NEW-01** (literal-minus) and **SCR-D4-NEW-02** (overflow → 0) first;
  they are latent only because the `.psc` lexer isn't on the live path today, and
  a real `.psc`/SCTX ingest makes both live wrong-AST bugs.
- **Multi-quest gates** — the fidelity gate (DA10) covers only single-quest
  predicate sets; **SCR-D5-NEW-03** is the untested cross-quest path. A guard test
  for a mixed-quest AND-conjunction (asserting decline) should accompany the fix.
- **Condition resolvers (#1663–#1668, closed)** — now implemented; a future
  audit should re-verify the safe-default sentinels and `RunOn` decline-on-
  unresolvable-target behavior against real CTDA data once a live cell exercises
  them.

---
*Dimension worksheets: `/tmp/audit/scripting/dim_{1..7}.md` (ephemeral).
Dedup baseline: 42 open issues + the four prior `AUDIT_SCRIPTING_*` reports.*
