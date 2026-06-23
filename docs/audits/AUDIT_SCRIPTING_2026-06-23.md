# Scripting Subsystem Audit — 2026-06-23

**Domain**: M30 / M47.0 / M47.1 / M47.2 — `.pex` decompiler (`crates/pex`),
`.psc` Papyrus parser (`crates/papyrus`), AST→ECS recognizer chain + ECS
scripting runtime (`crates/scripting`), and the cell-loader REFR-attach path
(`byroredux/src/cell_loader/references.rs`, `asset_provider.rs`).

**Depth**: deep, all 7 dimensions. First audit of this ~16k-LOC domain.
**Method**: every finding re-read against current code (including the
uncommitted M47.2 `fragment.rs` / `effects.rs` / `quest_stages.rs` work) and an
attempt made to disprove it. Dedup against the cached open-issue list
(`/tmp/audit/issues.json`); the only open scripting issues are the known
condition stubs (#1663–#1668, #1316), which are NOT re-filed.

## Executive Summary

**Shipped**: M30.2 `.psc` lexer+Pratt parser → AST; M47.0 ECS event hooks; M47.1
CTDA condition eval (OR-precedence + 7 functions, 6 stubbed); M47.2 `.pex` reader
+ 5-phase decompiler (Champollion port) + the compositional recognizer chain
(rumble + quest_stage_gate + the guard/effect primitive tables) + the dynamic
cell-loader attach path + XPRM trigger volumes.

**Deferred (not flagged)**: Obscript/SCTX frontend (Phase 5); the M47.1 condition
resolvers #1663–#1668; the QUST-VMAD fragment-section decoder (the fragment
dispatch engine exists but has no live population source — by design, no-guessing).

**Findings**: 16 total — **4 HIGH, 5 MEDIUM, 7 LOW**.

### Untrusted-input robustness verdict — NOT YET CLEAN
A hostile/corrupt `.pex` or `.psc` **can** take down the cell loader via two
reachable resource-exhaustion / stack-overflow holes:
- SCR-D1-01: a var-arg count up to `i32::MAX` pre-allocates tens of GB → OOM abort.
- SCR-D4-01: deeply nested `.psc` `If`/`While` statements overflow the parser
  stack (the `MAX_EXPR_DEPTH` cap covers expressions only).
Everything else on the untrusted paths is correctly bounded (`take` is the single
`.pex` read gate; the opcode `transmute` is sound; `translate_pex` is a clean
`None` on garbage). Fix the two above and the verdict flips to clean.

### 99.996% decompile-rate claim — VERIFIED HONEST (but narrow)
`pex_corpus_smoke.rs` genuinely runs `decompile_script` inside `catch_unwind` and
counts BOTH panics and `Err` against the reported percentage. It does NOT swallow
panics. **Caveat**: it measures "decompiled without panic/Err" only — it gives
ZERO protection against a wrong-but-non-panicking AST (e.g. SCR-D3-01's dropped
guard). The sole AST-correctness gate is the recognizer fidelity test.

### `.psc`-vs-`.pex` fidelity gate — VERIFIED, one-sided
`recognizes_da10_and_reproduces_hand_builder` pins true byte-equality of the
recognized component against the `da10_main_door` hand-builder — but only on the
`.psc` path. No test closes the loop on a decompiled DA10 `.pex` (SCR-D5-03).

### Decline-invariant — ONE LEAK
The load-bearing "decline on any unmodeled term" invariant is honored everywhere
EXCEPT the guarded shape of `quest_stage_gate` (SCR-D5-01), which emits a
component while silently dropping sibling statements. `effects::lower_fragment`,
`compose::classify_guard_atom`, the unconditional shape, and all hole-binding
declines are rigorously conservative.

### Doc-rot
`docs/feature-matrix.md` still says "Full Papyrus transpiler (M47.2) — ✗ ...
transpiler unstarted" (SCR-D8-01); three in-code comments lag the shipped boolean
pass and the live trigger emit site (SCR-D3-02, SCR-D6-03).

---

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested | Notes |
|------|-------------|------------|------------------|-----------------|-------|
| reader (`reader.rs`) | ✅ via `take` | ✅ | ⚠️ OOM on var-arg count (SCR-D1-01) | FO4-LE only (SCR-D1-02) | transmute sound |
| cfg (`cfg.rs`) | ✅ `checked_target` inclusive | ✅ | ✅ no `split(0)` underflow | ✅ | jmpf/jmpt polarity correct |
| lift + copy-prop (`lift.rs`) | ✅ `a[n]` bounded by arg_count | ✅ (fold shrinks scope) | ✅ Cast unwrap short-circuit-guarded | ✅ | single-adjacent-consumer model |
| boolean (`boolean.rs`) | ✅ | ✅ (merge shrinks graph) | ✅ | corpus | no debug-line guard = documented departure |
| control-flow (`control_flow.rs`) | ✅ | ✅ | ✅ | ✅ | ⚠️ silent guard-drop on `\|\|`-decline (SCR-D3-01) |
| lower (`lower.rs`) | ✅ | ✅ | ✅ total | ✅ | default binop arm unreachable for well-formed lift |

The two documented Champollion departures adjudicated: **no-debug-line-guard in
`boolean.rs`** = benign (validated against the corpus rate; structural signal is
load-bearing). **The `||`-skip in `control_flow.rs`** = a real wrong-AST hazard
when the boolean pass declines to collapse (SCR-D3-01), mitigated downstream by
the recognizer decline net but not failing closed.

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|-----------|--------|
| Marker drain coverage | ❌ OnTriggerEnterEvent (HIGH, SCR-D6-01) + OnCellLoadEvent (MED, SCR-D6-02) undrained |
| Two-phase lock-drop (timer / trigger / recurring) | ✅ explicit `drop()` before 2nd acquire |
| Fragment dispatch 3-resource lock | ✅ single scoped block |
| Cascade bound (MAX_CASCADE=64 + no-op skip) | ✅ |
| Edge-trigger seed (`occupant_inside`) | ✅ |
| CTDA OR-precedence | ✅ |
| Quest stage history (`stages_done`) | ✅ |

---

## Findings (by severity)

### SCR-D1-01: Var-arg count pre-allocates up to i32::MAX elements before EOF
- **Severity**: HIGH
- **Dimension**: PEX Reader & Opcode Decode · **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/reader.rs:474-481` (`read_instructions`)
- **Status**: NEW
- **Description**: The var-arg path accepts `Value::Integer(n) if n >= 0` then `Vec::with_capacity(n as usize)`. `n` is attacker-controlled up to `i32::MAX` (~2.1B). `Value` is an enum carrying a `String` (≥24 B), so `with_capacity(2^31)` requests tens of GB and aborts (OOM) *before* the per-element `self.value()?` reads can hit `take`'s EOF guard.
- **Evidence**: `Value::Integer(n) if n >= 0 => { let mut v = Vec::with_capacity(n as usize); for _ in 0..n { v.push(self.value()?); } v }`
- **Impact**: A ~30-byte hostile `.pex` in a modded `--scripts-bsa` aborts the process at cell load. Untrusted-input DoS.
- **Suggested Fix**: Use `Vec::new()` + push (geometric growth EOFs at the first read past buffer), or cap with a sane ceiling. The u16-counted readers are already benign (≤65535).

### SCR-D4-01: No recursion-depth guard on nested statements — stack overflow from untrusted .psc
- **Severity**: HIGH
- **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
- **Location**: `crates/papyrus/src/parser/stmt.rs:88-136` (`parse_if_stmt`/`parse_while_stmt` → `parse_block` → `parse_stmt`)
- **Status**: NEW
- **Description**: The `MAX_EXPR_DEPTH=256` cap (#1270) guards *expression* recursion only. Block-statement recursion (`If`/`While` bodies) has NO depth guard. A `.psc` with a few thousand nested `If`/`While` overflows the parser stack (abort, no catchable error) — the same stack-overflow class #1270 was opened to close, on the statement axis.
- **Impact**: A hostile/corrupt `.psc` aborts the process. Lower live exposure than `.pex` today (recognizers consume `.pex`-decompiled AST; the `.psc` parser drives reference scripts + future SCTX), but it is an untrusted-byte parser.
- **Suggested Fix**: Add a `stmt_depth` counter mirroring `expr_depth`, capped, returning a `StatementTooDeep` error.

### SCR-D5-01: quest_stage_gate guarded shape emits a component while silently dropping sibling statements
- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:166-218` (`extract_stage_gate`, shape 1/2)
- **Status**: NEW
- **Description**: The unconditional shape (shape 3, `single_set_stage`) correctly requires the body be EXACTLY one statement (`declines_unconditional_with_extra_statements` proves it). The **guarded** shape does NOT verify the body contains only the guarded `If` — it `for stmt in body { ... continue }` and returns on the first recognizable `If`, ignoring all sibling statements. A handler with `If guard / SetStage / EndIf` followed by (or preceded by) e.g. `Self.Disable()` matches, emits `QuestAdvanceOnActivate`, and silently drops the sibling. The docstring (lines 30-33, 158-159) explicitly promises to decline "a body that carries logic beyond the advance" — only shape 3 keeps it.
- **Evidence**: lines 174-205 iterate the body and `return Some(StageGate{..})` on the first matching `If` with no `body.len()==1` check.
- **Impact**: Silent partial lowering of a vanilla scripted REFR — the quest advance fires but co-located effects vanish, no fallback. The load-bearing decline-invariant leak (same class as a wrong NIFAL `Material`).
- **Suggested Fix**: After `peel_player_gate`, require the post-peel body to be exactly one statement (the guarded `If`), mirroring shape 3. Add a `declines_guarded_with_extra_statements` test.

### SCR-D6-01: OnTriggerEnterEvent emitted but NOT drained — quest re-advances every frame after entry
- **Severity**: HIGH
- **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/cleanup.rs:27-38` (drain list omits it) vs `crates/scripting/src/trigger.rs:132` (emit) + `crates/scripting/src/papyrus_demo/quest_advance.rs:254-258` (consume)
- **Status**: NEW
- **Description**: `trigger_detection_system` inserts `OnTriggerEnterEvent` (live M47.2 emit site). `event_cleanup_system` drains 9 marker types but NOT this one — despite trigger.rs:90-93 claiming it does (root cause: the stale `lib.rs:62` comment that says the emit site is "deferred to Rapier"). The marker persists indefinitely, so `quest_advance_system` re-consumes it every frame. For an unconditional / player-only advance (empty `ConditionList` → `evaluate` true) the quest `SetStage` + a fresh `QuestStageAdvanced` re-fire EVERY frame forever, flooding journal/fragment consumers and burning MAX_CASCADE each frame. A DA10-gated advance self-limits but still re-evaluates each frame.
- **Impact**: The canonical `default*Trigger` quest-volume family re-fires every frame after the player crosses a volume. Silent, all-Skyrim+/FO4 trigger blast radius. The textbook "marker not drained → re-fires every frame" hazard.
- **Suggested Fix**: Add `drain_component::<OnTriggerEnterEvent>(world)` to `event_cleanup_system`; extend `cleanup_removes_all_event_types` to assert it (and OnCellLoadEvent/OnEquipEvent).

### SCR-D1-02: No Skyrim-BE / Starfield-guards round-trip test on an untrusted parser
- **Severity**: MEDIUM
- **Dimension**: PEX Reader & Opcode Decode · **Untrusted-Input**: Yes (coverage of)
- **Location**: `crates/pex/src/lib.rs:115-273` (`build_sample` is FO4-LE only)
- **Status**: NEW
- **Description**: The only round-trip writer test exercises the FO4 little-endian dialect. The big-endian Skyrim path (different field gating, BE int/float decode) and the Starfield guards path have NO round-trip regression. A field-order/endian regression in those arms passes CI silently (the corpus smoke needs game data + a manual run).
- **Suggested Fix**: Add a BE-Skyrim and a Starfield-with-guards round-trip to the writer test.

### SCR-D3-01: control_flow `||`-skip silently drops a conditional block's statements when the boolean pre-pass declines to collapse
- **Severity**: MEDIUM
- **Dimension**: Decompiler Control-Flow / Boolean / Lower · **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/control_flow.rs:169-181`
- **Status**: NEW
- **Description**: When `reconstruct` reaches a conditional block whose `before` is itself conditional, no While/If/If-Else arm fires; the `else` is empty and `take_scope(current)` is NEVER called (the drains live inside the arms), so the block's lifted statements (incl. the condition) are silently discarded and `it` advances via `next_key`. The docstring frames this as the `||` case the boolean pre-pass handles — but `boolean::take_operand` declines when the operand needs >1 statement, reaching this branch with a real guard. Invisible to the corpus smoke (panic/Err only). Mitigated downstream: a dropped guard usually fails the recognizer decline net (→ silent miss), but a leak that still matches a recognizer would be a wrong lowering (HIGH-class) — hence fail-closed is the safer default.
- **Suggested Fix**: In the conditional-with-conditional-predecessor branch, return `ControlFlowFailed` (force a clean upstream decline) rather than silently dropping the block.

### SCR-D4-02: Error recovery skips to EOF, not to the next line — silently truncates the rest of the script
- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `crates/papyrus/src/parser/script.rs:625-633` (`skip_to_next_line`) × `mod.rs:71-81` (`peek` skips Newlines)
- **Status**: NEW
- **Description**: `skip_to_next_line` breaks on `matches!(tok, Token::Newline)`, but `peek()` skips all Newline tokens and never returns one, so the `Newline` arm is dead — the loop advances through every remaining token to EOF. After ONE recoverable top-level error, `parse_script` discards the ENTIRE rest of the file instead of resuming at the next line (defeating the documented partial-success recovery). Terminates (no infinite loop), but over-advances. No test catches it (`parse()` test helper panics on any recovered error).
- **Suggested Fix**: Use `peek_raw()` (does not skip Newlines) to find the line boundary; advance raw tokens until a raw `Token::Newline` is consumed. Add a two-item-first-malformed regression.

### SCR-D6-02: OnCellLoadEvent emitted but not drained — latent every-frame re-fire + one-frame-contract violation
- **Severity**: MEDIUM
- **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/cleanup.rs:27-38` vs `byroredux/src/cell_loader/references.rs:1461`
- **Status**: NEW
- **Description**: `attach_script_for_refr` emits `OnCellLoadEvent` on every scripted REFR; both its comment (references.rs:1458-1460) and events.rs:117-118 claim it is drained "so each script sees exactly one." cleanup.rs does NOT drain it. No consumer exists yet, so today it is only an undrained accumulating marker (per-entity leak + broken one-frame contract). The moment the promised OnCellLoad first-tick consumer lands it re-fires every frame.
- **Suggested Fix**: Drain `OnCellLoadEvent` (and `OnEquipEvent`) in `event_cleanup_system`.

### SCR-D7-01: per-REFR (Skyrim+) VMAD override scripts are never resolved
- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring · **Untrusted-Input**: Yes
- **Location**: `byroredux/src/cell_loader/references.rs:386,1556`
- **Status**: NEW
- **Description**: Both the trigger-volume path and `attach_vmad_scripts` resolve scripts ONLY via `base_record_script_instance(base_form_id)` — the base record's VMAD. Skyrim+ supports a per-REFR VMAD on the placed reference itself (uniquely-scripted placed objects/levers/quest items). That override VMAD is never read → those scripts attach nothing, a silent miss across that class of vanilla content.
- **Suggested Fix**: Consult the REFR's own decoded VMAD first, falling back to the base-record VMAD (override-then-base).

### SCR-D3-02: Stale "Known gap" doc-rot — control_flow.rs claims the boolean pass is unported
- **Severity**: LOW · **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Location**: `crates/pex/src/decompile/control_flow.rs:21-29`, `mod.rs:6-14`
- **Status**: NEW
- **Description**: control_flow.rs §"Known gap (intentional, for this commit)" states short-circuit boolean collapse "is not yet ported" / "lands in a following commit"; it HAS shipped (`boolean.rs` wired in `decompile_body`). `mod.rs:7-14` similarly tags passes 2-4 "(next)".
- **Suggested Fix**: Rewrite both docstrings; reference the residual SCR-D3-01 behaviour instead of "not yet ported."

### SCR-D5-02: fragment lowerer (effects.rs) implemented but unwired
- **Severity**: LOW · **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/effects.rs` vs `translate/mod.rs:34-39` (`RECOGNIZERS`)
- **Status**: NEW (designed Phase-3 gap)
- **Description**: `lower_fragment` + `EFFECT_PRIMITIVES` are complete and well-tested but no `RECOGNIZERS` entry calls them, so no decompiled quest-fragment `.pex` (69.5% of the corpus) is lowered via the live boundary — reachable only from its own tests. Matches the skill's "designed Phase-3 gap" note; not a correctness bug, but the feature is dead from the engine today.
- **Suggested Fix**: Wire a fragment recognizer into `RECOGNIZERS` when it lands; until then a doc note that the table is staged-not-wired.

### SCR-D5-03: no decompiled-.pex parity test for DA10
- **Severity**: LOW · **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:325-358`
- **Status**: NEW
- **Description**: The byte-equality fidelity gate runs `.psc` → AST → recognizer only. No test takes a DA10 `.pex`, runs `translate_pex`, and asserts the same hand-builder equality, so the decompiler→recognizer fidelity loop isn't closed by CI (the corpus smoke is panic-only).
- **Suggested Fix**: Add a `translate_pex` parity test using a DA10 `.pex` fixture (or the hand-built writer).

### SCR-D6-03: stale "deferred to Rapier sensor wiring" comments for OnTriggerEnterEvent
- **Severity**: LOW · **Dimension**: Scripting Runtime Systems
- **Location**: `crates/scripting/src/lib.rs:62`, `crates/scripting/src/events.rs:96-99`
- **Status**: NEW
- **Description**: Both claim OnTriggerEnterEvent has no engine emit site ("deferred to Rapier"); `trigger_detection_system` is the live M47.2 emit site. This stale comment is the root cause of SCR-D6-01 (the drain was never added).
- **Suggested Fix**: Update both comments to point at `trigger_detection_system`.

### SCR-D7-02: trigger-box rotation frame may not match the permuted half-extents
- **Severity**: LOW · **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/cell_loader/references.rs:1412-1436`
- **Status**: NEW (verify-not-confirmed)
- **Description**: Half-extents are permuted z-up→y-up (`[x,z,y]`) but `rotation` passes through verbatim; `TriggerVolume::contains` (Box) tests `rotation.inverse() * (p-center)` against the permuted extents. If `rotation` isn't in the same permuted frame, a rotated OBB trigger is wrong. The dedicated tests only use `Quat::IDENTITY` (permute invisible). Bethesda trigger boxes are mostly axis-aligned/vertical-axis-rotated, limiting exposure; I could not confirm the caller composes `rotation` in the permuted frame.
- **Suggested Fix**: Add a rotated-box trigger test end-to-end (placement → volume → `contains`) with a non-identity REFR rotation.

### SCR-D7-03: --scripts-bsa override order is "first-listed wins"
- **Severity**: LOW · **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/asset_provider.rs:613-621`, 641-643
- **Status**: NEW
- **Description**: `extract_pex` returns the first archive hit in flag order, so override archives must be listed BEFORE vanilla — the inverse of mod-manager load order (later = higher priority). Documented in the docstring (a contract, not a defect) but an ergonomic foot-gun.
- **Suggested Fix**: Document override-first prominently in CLI help, or reverse iteration so last-listed wins.

### SCR-D8-01: feature-matrix.md doc-rot — "transpiler unstarted"
- **Severity**: LOW · **Dimension**: (cross-cutting / docs)
- **Location**: `docs/feature-matrix.md` (the "Full Papyrus transpiler (M47.2)" row + the pending-work table row)
- **Status**: NEW
- **Description**: The matrix still reads "Full Papyrus transpiler (M47.2) — ✗ Foundation done; transpiler unstarted" and lists the transpiler as pending. The `.pex` decompiler, the recognizer chain, the corpus survey, and the engine attach path all shipped (Sessions 50-51).
- **Suggested Fix**: Update the row to reflect the shipped `.pex` decompiler + recognizer-chain + dynamic attach; mark fragment-population + Obscript as the remaining gaps.

---

## Decline-Invariant Audit

| Decline point | Verdict |
|---------------|---------|
| `split_and` keeps `\|\|` whole | ✅ conservative |
| `classify_guard_atom` per-atom `?` | ✅ no silent drop |
| `quest_stage_gate` unconditional shape (single_set_stage) | ✅ requires exactly-one |
| `quest_stage_gate` **guarded** shape | ❌ drops siblings (SCR-D5-01) |
| `quest_stage_gate` quest cross-check / hole binding | ✅ declines, never form-0 |
| `effects::lower_fragment` | ✅ `_ => return None` total |
| `translate_pex` bad bytes | ✅ clean `None` |
| `u16::try_from(stage)` overflow | ✅ declines, no wrap |

## Future-Phase Readiness
- **Obscript/SCTX (Phase 5)**: the recognizer chain is source-agnostic
  (`ScriptSource`); the same decline invariant + SCR-D5-01 fix will apply. The
  statement-depth guard (SCR-D4-01) should land before any SCTX frontend exposes
  the parser to more untrusted input.
- **Fragment lowerer (b2)**: engine + dispatch + decline are ready; needs the
  QUST-VMAD fragment-section decoder (no-guessing-blocked) to populate, then a
  `RECOGNIZERS` wire-up (SCR-D5-02).
- **Condition resolvers (#1663–#1668, #1316)**: safe-defaults verified correct;
  resolvers slot into `evaluate_function` without touching the OR-precedence core.
