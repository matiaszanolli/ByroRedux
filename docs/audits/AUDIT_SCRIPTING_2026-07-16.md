# Scripting Subsystem Audit — 2026-07-16

Sixth full pass over the M30/M47 Papyrus/.pex/ECS scripting domain (prior
reports: `AUDIT_SCRIPTING_2026-06-23.md`, `_06-27.md`, `_07-02.md`,
`_07-03.md`, `_07-06.md`). Seven dimensions run as parallel agents against
`crates/pex/`, `crates/papyrus/`, `crates/scripting/`, and the engine-side
attach path (`byroredux/src/cell_loader/`, `byroredux/src/asset_provider/`).

**Correction to this audit's own kickoff premise**: the invoking skill's
Phase 1 assumed no prior scripting audit exists. That's stale — five prior
reports exist, the most recent only 10 days old. This pass therefore ran as
a delta/re-verification audit against `AUDIT_SCRIPTING_2026-07-06.md`
(confirmed via `gh issue list`) rather than a from-scratch sweep, per actual
repo state.

**A second, more important premise correction surfaced during this pass**:
git history shows the 2026-07-06 report's own commit (`977eb95a`, merged
2026-07-07 20:27) landed *after* three same-day fix commits
(`b3d63a2b`, `f63a701e`, `8a70b81a`, all 2026-07-06). So that report was
already stale relative to the code the moment it was committed — all five
of its open findings (#1905–#1909) were fixed before the report itself
merged, and `8a70b81a` additionally wired the previously-hypothetical QUST
VMAD fragment decoder into the live cell-load path (845 scripted quests →
742 lowered fragments on real Skyrim data). This audit re-verified all of
that live rather than trusting the prior report's "Future-Phase Readiness"
framing.

## Executive Summary

**What shipped** (all re-confirmed live, no regressions): M30.2 `.psc`
lexer/parser; M47.0 event-hook runtime; M47.1 condition evaluation (all 13
catalog functions now fully implemented, no longer stubs — see Dimension 6);
M47.2 `.pex` reader + 5-phase decompiler + recognizer chain + dynamic VMAD
attach path + XPRM trigger volumes; and, newly confirmed live this pass, the
**QUST VMAD fragment decoder** (`8a70b81a`) — `quest_fragment_dispatch_system`
is no longer a structural no-op, it is now populated from real ESM data and
both live-dispatched and save-persisted.

**Deferred** (correctly, not flagged as defects): Obscript/SCTX frontend
(Phase 5); multi-quest fidelity-gate coverage beyond DA10 (informational
gap, not a bug).

**Findings this pass**: 7 new (0 CRITICAL / 1 HIGH / 3 MEDIUM / 3 LOW).
5 prior findings (#1905–#1909) independently re-confirmed FIXED across
Dimensions 4, 5, and 6. 4 prior tracked-issue regression guards
(#1737, #1742, #1864, plus the #1727/#1767/#1768/#1817 lifecycle set)
re-confirmed intact with zero drift. 2 issues (#1743, #1769) re-confirmed
still open and accurately described — not re-filed.

**Untrusted-input robustness verdict — CLEAN (NO crash/panic/OOB/OOM)**,
with one caveat: Dimension 2 found a reproducible **algorithmic-complexity**
gap (not memory-unsafe, not a crash) — a crafted `.pex` function at the
format's own 65535-instruction ceiling can stall the decompiler's
copy-propagation pass for ~1.25s of single-threaded CPU time with no error,
panic, or size cap anywhere upstream. This is new: no prior audit pass
benchmarked this path empirically.

**The 99.996% (26640/26641) decompile-rate claim — VERIFIED HONEST**
(re-confirmed independently this pass). `pex_corpus_smoke.rs` calls
`decompile_script` inside `catch_unwind` for every parseable `.pex`, tallies
`Ok`/`Err`/panic as three disjoint buckets, and the denominator is
parseable files with panics counted as failures — not silently dropped or
inflated.

**The `.psc`-vs-`.pex` fidelity gate — VERIFIED, both sides closed, still
narrow**. `recognizes_da10_and_reproduces_hand_builder` (`.psc`-side) and
`da10_pex_reproduces_hand_builder_byte_for_byte` (`.pex`-side, `#[ignore]`-
gated on Skyrim SE data, #1740) both still assert byte-equality against the
DA10 hand builder. Coverage remains single-quest-predicate only.

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|
| Reader (`crates/pex/src/reader.rs`) | Yes — `take()` is the sole gate, no bypass found | Yes — var-arg loop grows via `Vec::push`, EOFs at buffer end, never `with_capacity(hostile_n)` | Yes | Yes, 3 dialects (FO4/LE, Skyrim/BE, Starfield+guards) round-trip |
| CFG (`cfg.rs`) | Yes — inclusive jump-target bound correct, `split(0)` structurally unreachable | Yes | Yes | Yes, named guard tests intact |
| Lift + copy-prop (`lift.rs`) | Yes | Yes, but **O(n²)** — see SCR-D2-NEW-01 | Yes | Yes |
| Boolean (`boolean.rs`) | Yes | Yes — re-process loop strictly shrinks block count, `MAX_REBUILD_DEPTH=1024` caps recursion | Yes — degenerate `operand_key==rejoin_key` case absorbed by control_flow's fail-closed catch-all, not a panic (SCR-D3-NEW-01, informational) | Yes |
| Control-flow (`control_flow.rs`) | Yes | Yes, same recursion cap | Yes — fails closed (`ControlFlowFailed`) on the deliberate `||`-skip case rather than dropping a guard silently | Yes |
| Lower (`lower.rs`) | Yes | Yes | Yes — every `NodeKind` matched, no `_ => panic!`; the `lower_binary_op` default-arm-to-`Eq` concern is confirmed structurally unreachable (only modeled op strings are ever produced upstream) | Yes |

Both documented Champollion departures (no debug-line guard in `boolean.rs`;
the deliberate `||`-skip in `control_flow.rs`) are re-adjudicated **benign**
— the fail-closed catch-all (#1732) is now confirmed load-bearing for two
independent gaps (the originally-documented one, plus the newly-traced
`operand_key==rejoin_key` degenerate case), which is worth knowing before
anyone next touches that catch-all, but neither is exploitable today.

## Decline-Invariant Audit

The recognizer-chain decline invariant (`crates/scripting/src/translate/`)
is **sound with one open gap**. Chain ordering, `split_and`'s
disjunction-stays-whole guarantee, all three `QuestRef` hole-binding decline
paths, `CanonicalEvent`'s safe long-tail bucket, and the
`translate_pex`/`populate_quest_fragments_from_pex` panic-catching parity
all verified clean. Three previously-flagged leaks in this invariant
(mixed-quest gate retargeting, non-quest side-effecting bindings, rumble's
non-literal property coercion) are all **confirmed fixed**, each with a
dedicated regression test. One **new HIGH** gap was found in a sibling table
those fixes didn't touch: `effects.rs`'s `SetObjective{Displayed,Completed,
Failed}` primitives still collapse "argument present but non-literal" into
"argument absent → default `true`" — see SCR-D5-NEW2-01 below. Unlike the
already-fixed rumble case, Papyrus call arguments aren't grammar-restricted
to literals, and the target state is both live-dispatched and
save-persisted.

## Runtime Lifecycle Invariant Matrix

| Invariant | Status |
|---|---|
| Marker drain coverage (all 12 transient types) | CLEAN — `event_cleanup_system` is the sole `Stage::Late` scripting system, drains every emitted marker type |
| Two-phase lock-drop (`timer_tick_system`, `trigger_detection_system`, `recurring_update_tick_system`, `quest_advance_system`, rumble-demo emission) | CLEAN — no two component/resource-mut locks held concurrently in any system checked |
| `quest_fragment_dispatch_system` resource-lock scoping | CLEAN, and now load-bearing on real data (not just unit tests) since `8a70b81a` wired real quest fragments |
| Cascade bound (`MAX_CASCADE=64`) | CLEAN, WARN on overflow, no-op transitions filtered |
| Edge-trigger seed (`occupant_inside: Option<bool>`) | CLEAN, lazy-seed prevents frame-1 false enter |
| CTDA OR-precedence | CLEAN, trailing-OR clamp + empty-list contract intact |
| Condition stubs (#1663–#1668, #1316) | **No longer stubs** — all 13 catalog functions fully implemented with correct Bethesda safe-default sentinels; `RunOn` declines (doesn't default to Subject) on every unresolvable target type. Verified by 27 passing unit tests; not yet re-verified against a live headless cell run. |

## Findings

### HIGH

#### SCR-D5-NEW2-01: `SetObjective{Displayed,Completed,Failed}` effect primitives collapse "present-but-non-literal argument" into "absent → default `true`" — on the now-live fragment path
- **Severity**: HIGH
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: Yes — reachable via any decompiled quest-fragment `.pex` through the now-wired `populate_quest_fragments_from_pex` → `lower_fragment` path.
- **Location**: `crates/scripting/src/translate/effects.rs:227-259` (`prim_set_objective_displayed`/`_completed`/`_failed`), `bool_arg` helper at `:297-299`.
- **Status**: NEW
- **Description**: `bool_arg(args, idx)` returns `Some(as_num(...)? != 0.0)`, collapsing two distinct cases into one `None`: the argument slot is genuinely absent (Papyrus optional-arg omission — default should apply) vs. present but not a literal (a local bool variable, `Not(...)`, a copy-propagated temp — a term the primitive can't evaluate). All three call sites do `bool_arg(args, 1).unwrap_or(true)`, so a present-non-literal argument silently becomes `true` exactly as if omitted. This is the identical defect class `f63a701e`/#1909 fixed in `rumble.rs::float_prop`/`bool_prop` one file over — but that fix touched only the guard-side coercion, not this sibling effect-side table.
- **Evidence**: `as_num` (`compose.rs:76-84`) only matches `IntLit`/`FloatLit`/`BoolLit`/`Cast`; any other `Expr` (notably `Expr::Ident`, the shape a local variable takes) returns `None`, which `bool_arg` also turns into `None` — indistinguishable from "argument doesn't exist." No test exercises a non-literal 2nd argument to any `SetObjective*` call.
- **Impact**: A fragment statement like `Self.SetObjectiveCompleted(20, bWasSuccessful)` — ordinary, unconstrained Papyrus, unlike auto-property initializers which the grammar restricts to literals (the reasoning that kept the rumble case at LOW does not transfer here) — is emitted as `completed: true` regardless of the real runtime value. `QuestObjectiveState` is live-dispatched by `quest_fragment_dispatch_system` and persisted in save data (`byroredux/src/save_io.rs`), so this silently and permanently corrupts quest-journal state. Reachable the moment a real quest fragment with a non-literal completion flag decompiles through the now fully-wired `--scripts-bsa` path.
- **Related**: Sibling defect to the just-fixed #1907 (same file) and #1909 (`rumble.rs`) — same "present-non-literal collapses into absent-default" shape, in the one table those fixes didn't touch.
- **Suggested Fix**: Give `bool_arg` the same `Option<Option<bool>>` contract `rumble.rs::bool_prop` now has: `None` when the index is out of range (genuinely absent), `Some(None)` when present but `as_num` fails (decline), `Some(Some(v))` when present and literal. Update the three call sites to `bool_arg(args, 1)?.unwrap_or(default)` and add a guard test asserting `lower_fragment` returns `None` for `Self.SetObjectiveCompleted(20, someVar)`.

### MEDIUM

#### SCR-D2-NEW-01: `rebuild_expression`'s restart-to-zero copy-propagation scan is O(n²) — a crafted `.pex` function stalls the decompiler for over a second, no size cap anywhere upstream
- **Severity**: MEDIUM (algorithmic-complexity DoS, not a crash/OOB/OOM — doesn't meet this domain's HIGH-minimum bar, which is reserved for panic/OOB/unbounded-alloc)
- **Dimension**: Decompiler CFG & Lift
- **Untrusted-Input**: Yes — reachable from raw `.pex` bytes via `translate_pex` on the live cell-loader VMAD-attach path.
- **Location**: `crates/pex/src/decompile/lift.rs:316-346` (`rebuild_expression`)
- **Status**: NEW
- **Description**: After every successful single-consumer fold, the scan index resets to `0` and re-scans the (now one-shorter) list from the start — a faithful port of Champollion's C++ iterator-invalidation workaround, but `O(n²)` in the number of statements whenever fold targets aren't clustered at the front. No file in `crates/pex` or `crates/scripting` bounds instruction count below the wire format's own `u16` field (max 65535) — `grep` confirms zero hits for any `MAX_INSTRUCTIONS`/size-cap/timeout.
- **Evidence**: Empirically benchmarked (standalone harness, not guessed): 500 fold pairs → 1.045ms; 21845 pairs (65535 instructions, the hard per-function ceiling) → 1.255s. Timing roughly quadruples on every doubling — textbook O(n²).
- **Impact**: `translate_pex` wraps decompilation in `catch_unwind` (#1816), which guards panics but not a slow-but-successful computation. On the live cell-loader path this runs synchronously; a single hostile REFR script (or several, since one `.pex` commonly carries multiple functions each independently able to approach the ceiling) can stall cell load for multiple seconds to tens of seconds without ever erroring. The same scan re-runs on larger merged scopes from `control_flow.rs`/`boolean.rs`, compounding the total cost.
- **Suggested Fix**: Resume the scan at `i.saturating_sub(1)` instead of `0` after a fold (the producer's fold target is always adjacent — `count_constant_id` only ever looks at `scope[i+1]` — so this preserves fold order and drops the pass to O(n)); or add a `MAX_BLOCK_INSTRUCTIONS` cap mirroring the existing `MAX_REBUILD_DEPTH` precedent. Prefer the O(n) fix — it also speeds up legitimate large scripts. Add a regression test asserting bounded time/iteration count on an adversarial fixture.

#### SCR-D4-NEW2-01: A single out-of-range literal anywhere in a `.psc` file now hard-fails the entire `parse_script`, discarding every other item — granularity regression from the #1908 fix
- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Untrusted-Input**: Yes (latent today — see Impact)
- **Location**: `crates/papyrus/src/lib.rs:20-30` (`parse_expr`), `:62-72` (`parse_script`) — `if !lex_errors.is_empty() { return Err(...) }`
- **Status**: NEW
- **Description**: `parse_script`'s own doc comment promises a script parses partially on recoverable errors. That holds for parser-level errors (a malformed function is dropped, siblings still parse) but not lex-level errors: `lex_errors` is collected across the whole preprocessed source, and if non-empty, `parse_script` returns `Err` immediately — before the tolerant per-item-recovering parser ever runs. Pre-#1908, an out-of-range literal never produced a lex error (silent `0`), so this whole-file gate rarely tripped. Post-#1908 (which correctly turned that silent-`0` into a real lex error, fixing SCR-D4-NEW-02), the same literal now always trips the whole-file gate — a single bad literal in one function of an otherwise-valid multi-hundred-line script now yields zero AST for the entire file, not just the offending item.
- **Evidence**: Live-verified via a temporary probe test (reverted, `git diff` clean): a 2-function script with one out-of-range literal in `Function A` returns `Err` with `Function B` never parsed at all, despite being valid and unrelated.
- **Impact**: Latent today — the live cell-loader attach path decompiles `.pex` directly and never calls `parse_script`/`parse_expr`; today's callers are curated test fixtures. But it undermines this dimension's stated resilience contract and will be live the moment a real `.psc` or Obscript/SCTX frontend feeds this parser — a strictly worse failure mode for modded-content ingest than either the pre-fix silent-`0` bug or the intended per-item-recoverable model.
- **Related**: Direct side effect of the #1908 fix (`token.rs` `parse_int`/`parse_float` → `Result`); not a regression of #1906/#1908 themselves — both remain correctly fixed — but a gap in how far that fix threaded through the pipeline.
- **Suggested Fix**: Route lex errors through the same per-item recovery path as parse errors — either convert each `LexError` into a synthetic placeholder token so `skip_to_next_line` naturally isolates the damage, or scope lex-failure to the containing line so `parse_script` drops only the offending item. Add a regression test asserting a multi-function script with one bad literal still returns `Ok` with the unaffected functions present.

#### SCR-D7-NEW2-01: A SCOL/PKIN outer REFR's own VMAD is replicated onto every expanded synthetic child instead of attaching once
- **Severity**: MEDIUM
- **Dimension**: Engine Attach Path & Trigger-Volume Wiring
- **Untrusted-Input**: Yes (reachable via any plugin placing a VMAD-scripted REFR whose base form is SCOL/PKIN with no cached merged model)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:365-373` (the `synth_refs` expansion loop), consumed at `:463-469`, `:479-487`, `:784-792` — all pass the same `placed_ref.script_instance.as_ref()` (the outer REFR's own VMAD) for every synthetic child.
- **Status**: NEW
- **Description**: `expand_pkin_placements`/`expand_scol_placements` (`byroredux/src/cell_loader/refr.rs:357-450`) fan one placed REFR out into N synthetic child placements. The REFR's own VMAD (a per-instance Skyrim+ Papyrus attachment) is a property of the single outer REFR, but the expansion loop threads it unchanged into `attach_script_for_refr` for every synthetic child. Each child is a distinct ECS entity, so a VMAD-scripted SCOL/PKIN REFR's canonical behavior (including the `OnCellLoadEvent` that follows a successful attach) is instantiated N times instead of once. This mirrors the deliberate, correct sharing already special-cased for texture overlays (#584 — correct because a visual re-skin is identical per piece), but VMAD attachment is behavioral, not visual, and has no equivalent rationale.
- **Evidence**: `refr.rs:490-505` shows SCOL parts fanning from one `base_form_id`; `mod.rs:469/484/789` all read the outer `PlacedRef.script_instance`, never re-scoped per synthetic child (unlike `child_form_id`, which correctly varies). No test exercises a VMAD-carrying SCOL/PKIN REFR.
- **Impact**: N independent copies of the same recognized behavior spawn per decorative piece rather than once per logical object. Harmless for an idempotent effect (`SetStage` to the same target N times is a no-op after the first) but would fire once per piece for a non-idempotent side effect (item grant, spawn, sound), and the cell-load init hook fires N times instead of once. Narrow trigger — requires SCOL/PKIN with no merged model plus REFR-level VMAD, a combination not observed in vanilla content — hence MEDIUM.
- **Related**: Distinct from #1737 (REFR-vs-base-record VMAD source precedence); distinct from the deliberate, correct `refr_overlay` sharing (#584) this finding contrasts against.
- **Suggested Fix**: Attach the outer REFR's own `script_instance` only to the first synthetic child (or a dedicated non-rendering placement-root entity), passing `None` for the remaining N-1 children — mirroring how `door_pos` already special-cases "first of N" elsewhere in the same loop. Alternatively gate REFR-own-VMAD attach on `synth_refs.len() == 1` and trace-log when a multi-piece expansion carries a VMAD.

### LOW

#### SCR-D1-NEW-01: Four of six `PexError` variants have zero test coverage anywhere in the repo
- **Severity**: LOW
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/reader.rs:150-161` (`value` → `BadValueType`), `:139-148` (`string_index` → `BadStringIndex`), `:463-497` (`read_instructions` → `BadOpcode`, `BadVarArgCount`)
- **Status**: NEW
- **Description**: `BadMagic`/`UnexpectedEof` are covered; the var-arg huge-positive-count path is covered (but only the success arm, not the `_ => Err(BadVarArgCount)` reject arm). A repo-wide grep finds `BadValueType`/`BadOpcode`/`BadVarArgCount`/`BadStringIndex` referenced only at their construction sites and enum definition — no test constructs `.pex` bytes that trigger any of the four. Manual review confirms all four implementations are currently correct; this is a coverage gap, not an active defect.
- **Impact**: None today. These are exactly the four decode paths a future opcode-table edit or `Value` enum change would most likely silently break, and none would be caught by `cargo test` or the corpus smoke harness (which only exercises well-formed game `.pex`, never these reject-on-malformed branches).
- **Suggested Fix**: Add four hand-built-`.pex` regression tests — a value-type tag of 6, an opcode byte of `MAX_OPCODE`, a string-table index one past the table length, and a var-arg count of `Value::Integer(-1)` — each asserting the specific `PexError` variant, mirroring the existing `hostile_vararg_count_errors_instead_of_ooming`/`rejects_bad_magic` pattern.

#### SCR-D3-NEW-01: `boolean.rs::collapse` doesn't special-case `operand_key == rejoin_key`, but `control_flow.rs`'s fail-closed catch-all absorbs the fallout — documented, not exploitable
- **Severity**: LOW (informational / defense-in-depth documentation)
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Untrusted-Input**: Yes (only reachable via hand-crafted/adversarial `.pex`, never real compiler output — a real compiler never emits a `jmpf`/`jmpt` with an empty skip span)
- **Location**: `crates/pex/src/decompile/boolean.rs:166-226` (`collapse`), consumed by `control_flow.rs:96-210`
- **Status**: NEW
- **Description**: A `CodeBlock` with equal `on_true`/`on_false` targets makes `operand_key == rejoin_key`; `collapse` unconditionally removes `operand_key` before looking up `rejoin`, so the lookup returns `None` and `current`'s edges are never updated, leaving a stale conditional block. Traced by hand and confirmed the fallout is safe: `control_flow::reconstruct` later hits the final `else` arm and fails closed (`ControlFlowFailed`) rather than panicking, hanging, or emitting a wrong AST.
- **Impact**: None observed. Filed so a future audit doesn't have to re-derive the trace from scratch — the fail-closed catch-all (#1732) is now load-bearing for two independent gaps, worth knowing before it's next touched.
- **Suggested Fix**: None required. Optional hardening: an explicit `if operand_key == rejoin_key { return Ok(false); }` guard would make the impossibility self-evident in the code.

#### SCR-D6-NEW2-01: Fragment-dispatch docs (module header + scheduler comment) describe the now-live QUST-fragment pipeline as an unwired no-op
- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment.rs:11-26` (module doc, "Population (pending)"); `byroredux/src/boot.rs:604-607` ("no-op until the QUST-VMAD fragment decoder lands, #1739"); `crates/plugin/src/esm/records/script_instance.rs:46` ("fragment decode is a later phase")
- **Status**: NEW
- **Description**: `8a70b81a` wired `populate_quest_fragments_from_pex`/`_from_script` into the cell loader and is validated end-to-end on real Skyrim data (845 scripted quests → 742 lowered fragments). The commit updated `translate/mod.rs` and the design doc to say "landed," but missed `fragment.rs`'s own module doc, and two more sites (`boot.rs`'s scheduler comment, `script_instance.rs`'s module doc) were never on the update list and are stale for the same reason.
- **Impact**: Purely informational — no behavior is wrong. But a future contributor reading `fragment.rs`'s header (the natural first stop before touching `quest_fragment_dispatch_system`) would reasonably conclude the dispatcher is dead code and skip regression-testing it — the exact reasoning the 2026-07-06 report itself used to downgrade the now-fixed #1907 from HIGH to MEDIUM. It is wired now.
- **Related**: Not a re-file of #1907 (already fixed) — purely the three doc sites lagging the commit that fixed it.
- **Suggested Fix**: Update `fragment.rs:11-26` to "Population (shipped, #1739/8a70b81a)"; drop or update the `boot.rs` "no-op until … lands" comment; update `script_instance.rs:46` to point at `parse_quest_fragments` in the same file.

## Confirmed-fixed prior-audit findings (re-verified in place, no regression)

**From the 2026-07-06 report, all five closed and re-confirmed sound (not
just present)**: #1905/SCR-D5-NEW-03 (quest_stage_gate mixed-quest
retarget — `classify_if_condition` now does compare-or-decline, dedicated
regression test `declines_mixed_quest_conjunction`), #1906/SCR-D4-NEW-01
(lexer swallowing leading `-` — `-?` removed from all three literal
regexes), #1907/SCR-D5-NEW-04 (`lower_fragment`/`bind_local` dropping a
non-quest side-effecting binding — now declines via `is_side_effect_free`),
#1908/SCR-D4-NEW-02 (out-of-range literal silently becoming `0` — now a lex
`Err`), #1909/SCR-D5-NEW-05 (`rumble` coercing non-literal properties —
`float_prop`/`bool_prop` now return `Option<Option<T>>` to distinguish
absent from present-non-literal).

**From all prior reports, still intact with zero drift** (all files in
scope had zero git commits since 2026-07-06 except the ones listed above):
SCR-D1-01/#1710, SCR-D1-02/#1728 (var-arg + BE/guards round-trips),
SCR-D2/D3 regression-guard rosters (jump bounds, jmpf/jmpt polarity, Cast
heuristic, copy-prop single-consumer, pass order, recursion caps,
SCR-D3-01/#1732, SCR-D3-02/#1738), SCR-D4-01/#1712 (stmt depth cap),
SCR-D4-02/#1734 (recovery skips to next line), SCR-D5-01/03/#1719+#1740+
#1766 (single-statement body, DA10 fidelity gate both sides),
SCR-D5-NEW-02/#1816 (`catch_unwind` on decompile panics — verified still
present and now mirrored identically in the fragment-population path),
SCR-D6-01/#1727, SCR-D6-02 (marker drains — all 12 types, cross-checked
against every `insert` site), SCR-D6-NEW-01/#1767 (trailing-OR clamp),
SCR-D6-NEW-02/#1768 (both systems scheduled, now exercised by real data),
SCR-D6-NEW-03 (Globals symmetric rebuild), SCR-D6-NEW-04/#1817
(occupant_inside lazy seed), SCR-D7-01/#1737 (per-REFR VMAD),
SCR-D7-02/#1742 (trigger rotation frame), SCR-D7-NEW-01/#1864 (batched
same-frame quest advances).

## Existing / correctly-tracked (NOT re-filed — dedup)

- **#1743** (SCR-D7-03) — `--scripts-bsa` override order is "first-listed
  wins" (mod-over-vanilla would want later-wins). Still open, description
  still accurate; `asset_provider/script.rs`.
- **#1769** (D7-NEW-01) — VMAD attach dedup (`attach.rs`'s `seen: HashSet<&str>`)
  is case-sensitive; the decode layer itself is correctly case-insensitive,
  the bug is isolated to this one dedup set. Still open, still accurate.

## Future-Phase Readiness

- **Fragment lowerer (`8a70b81a`, now live)**: the "becomes HIGH once wired"
  caveat the 2026-07-06 report attached to #1907 has already materialized
  and been resolved — the fix landed the same day, just ahead of the
  keystone commit that made it live. The **new** open item on this path is
  SCR-D5-NEW2-01 (`SetObjective*` bool-arg coercion) — this should be
  treated with the same urgency #1907 was, since the path is confirmed live
  today, not latent.
- **Untrusted-input resource exhaustion**: SCR-D2-NEW-01 is the first
  finding in six audit passes to identify a non-crash, non-panic denial-of-
  service vector on the `.pex` decode path. Worth adding a benchmark-backed
  regression test (bounded time/iteration count) alongside the fix, since
  this class of bug is invisible to both `cargo test` and the corpus smoke
  harness (neither measures wall-clock time).
- **Obscript/SCTX frontend (Phase 5)**: unchanged guidance from prior
  reports — if the SCTX parser reuses the `.psc` lexer, the granularity
  regression in SCR-D4-NEW2-01 (whole-file failure on one bad lex token)
  should be fixed first, since a real `.psc`/SCTX ingest path would make it
  a live, frequently-triggered bug rather than a latent one.
- **Condition resolvers (#1663–#1668, #1316, all closed)**: now fully
  implemented and unit-tested (27 tests, Dimension 6). Re-verification
  against a live headless cell with real CTDA data (rather than unit tests
  alone) remains outstanding for a future pass, per the no-parallel-engine-
  launch policy.
- **Multi-piece REFR expansion (SCOL/PKIN)**: SCR-D7-NEW2-01 is the first
  finding to examine VMAD attachment through the synthetic-child expansion
  angle. Worth a design note before FO4-precombine-adjacent work touches
  this loop again, since the "share is correct" precedent (#584,
  `refr_overlay`) doesn't extend to behavioral (VMAD) properties.

---
*Dimension worksheets: `/tmp/audit/scripting/dim_{1..7}.md` (ephemeral, removed
after finalization). Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux`
(28 open issues at audit time) + `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`.*
