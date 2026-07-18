---
description: "Deep audit of the M30/M47 scripting domain — .pex decompiler (Champollion port), .psc Papyrus parser, AST→ECS recognizer chain, ECS scripting runtime, and the cell-loader attach path"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Scripting Subsystem Audit (M30 / M47.0 / M47.1 / M47.2)

Audit the three scripting crates plus their engine-side wiring for
correctness across the full compiled-Papyrus pipeline: untrusted `.pex`
bytecode decode (`crates/pex/`), the 5-phase decompiler that lifts that
bytecode back to the shared Papyrus AST, the `.psc` source parser
(`crates/papyrus/`), the AST→ECS recognizer chain whose load-bearing
invariant is *decline-on-any-unmodeled-term* (`crates/scripting/src/translate/`),
the ECS scripting runtime (events / timers / conditions / triggers / quest
stages), and the cell-loader REFR-attach path that resolves a scripted REFR's
VMAD-named `.pex` and runs the recognizer chain.

This domain (~16k LOC) has six prior audit passes in `docs/audits/AUDIT_SCRIPTING_*.md`
— read the most recent one first (Phase 1 below). The
decompiler is the **highest bug-density area**: it parses untrusted bytecode
and runs five tree-rewriting passes, so dimensions are weighted toward it
(three of seven). Its correctness story rests on a corpus-decompile
smoke harness and the `.psc`-vs-`.pex` fidelity gate — point findings there,
not at speculation.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, and finding format. See
`.claude/commands/_audit-severity.md` for the severity scale. Do NOT duplicate
those here.

## Scope

**Crates** (21-crate sanity check in `_audit-common.md`; `pex` is the newest):
- `crates/pex/src/` — `.pex` reader + 5-phase decompiler. Files: `crates/pex/src/opcode.rs`,
  `crates/pex/src/reader.rs`, `crates/pex/src/model.rs`, `crates/pex/src/lib.rs`,
  and `crates/pex/src/decompile/` (`mod`, `cfg`, `lift`, `control_flow`, `boolean`,
  `lower`, `node`, `event_names`).
- `crates/papyrus/src/` — `.psc` lexer (logos) + Pratt parser → AST. Files:
  `crates/papyrus/src/token.rs`, `crates/papyrus/src/lexer.rs`, `crates/papyrus/src/ast.rs`,
  `crates/papyrus/src/span.rs`, `crates/papyrus/src/error.rs`, `crates/papyrus/src/lib.rs`,
  and `crates/papyrus/src/parser/` (`mod`, `expr`, `stmt`, `script`).
- `crates/scripting/src/` — ECS-native runtime + recognizer chain. Runtime:
  `crates/scripting/src/events.rs`, `crates/scripting/src/timer.rs`,
  `crates/scripting/src/cleanup.rs`, `crates/scripting/src/condition.rs`,
  `crates/scripting/src/trigger.rs`, `crates/scripting/src/quest_stages.rs`,
  `crates/scripting/src/fragment.rs`, `crates/scripting/src/recurring_update.rs`,
  `crates/scripting/src/registry.rs`, `crates/scripting/src/lib.rs`. Recognizer
  chain: `crates/scripting/src/translate/` (`mod`, `source`, `archetype`, `compose`,
  `effects`, `tables`, `recognizers/{mod, quest_stage_gate, rumble}`). Reference
  scripts: `crates/scripting/src/papyrus_demo/`.

**Engine-side wiring** (Dimension 7 — outside the crates):
- `byroredux/src/cell_loader/references/mod.rs` — `attach_vmad_scripts` /
  `attach_script_for_refr` call `byroredux_scripting::translate_pex`; the
  `trigger_volume_from_primitive` builder spawns invisible `TriggerVolume`
  REFRs from `XPRM` primitives.
- `crates/plugin/src/esm/records/index.rs` — `base_record_script_instance`
  accessor (VMAD retained on ACTI/CONT/NPC/CREA base records).
- `crates/plugin/src/esm/records/script_instance.rs` — `ScriptInstanceData` /
  `ScriptInstance` (decoded VMAD).
- `byroredux/src/asset_provider/script.rs` — `build_script_provider` parses the
  repeatable `--scripts-bsa` flag; `extract_pex` resolves a VMAD script name
  to `.pex` bytes.

**Ground truth — read these before auditing**:
- `docs/engine/scripting.md` — the 50KB authoritative model (ECS-native VM
  replacement, recognizer-chain design, 136-event ECS mapping).
- `docs/engine/papyrus-parser.md` — M30 `.psc` parser + AST.
- `docs/engine/m47-0-design.md` — event-hooks runtime (the attach chain M47.2 extends).
- `docs/engine/m47-2-design.md` — the `.pex` decompiler + recognizer-chain spec,
  the `.psc`-vs-`.pex` fidelity gate, "no opcode semantics guessed" rule.
- `docs/engine/m47-2-recognizer-scaling.md` — corpus characterization
  (26,641 `.pex`; handler vs fragment populations; decline-the-tail thesis).
- The crate module docstrings: `crates/pex/src/lib.rs`,
  `crates/pex/src/decompile/mod.rs`, `crates/scripting/src/translate/mod.rs`.

**Doc-rot check**: `docs/feature-matrix.md:139` was already corrected (independent
of this skill) to reflect the shipped `.pex` recognizer slice; only line ~175
("What Doesn't Work Yet") still lists the *full* transpiler as deferred, which
remains accurate (the recognizer chain is a targeted slice, not a general
transpiler). Do not re-flag line 139 as stale — verify it still reads correctly
before reporting any doc-rot here.

**Corpus / fidelity instruments (point findings here, do not re-derive)**:
- `crates/pex/examples/pex_corpus_smoke.rs` — runs `byroredux_pex::parse` +
  `decompile::decompile_script` over every `.pex` in real game archives; the
  **source of the 99.996% (26640/26641) zero-panic decompile claim**. Verify the
  claim by re-reading the harness's success/failure tally logic — confirm it
  actually counts a decompile *panic* / `Err` as a failure (a harness that
  swallows panics would inflate the rate).
- `crates/pex/examples/pex_corpus_shapes.rs` + `docs/r5/corpus-shape-survey.txt` —
  the structural-fingerprint coverage instrument behind the recognizer-scaling doc.
- `crates/bsa/examples/r5_extract_pex_ba2.rs` — the `.pex` corpus extractor.
- `docs/smoke-tests/m47-triggers.sh` — engine-side spawn+attach gate on real
  Skyrim data (`--scripts-bsa`, the `M47.2 scripts:` cell-load summary line).

**Future-phase gaps (do NOT flag as missing unless scope says so)**:
- Obscript / `SCTX` frontend (Oblivion/FO3/FNV) — `ScriptSource::Obscript` is a
  typed placeholder; the SCTX parser is M47.2 Phase 5, not built.
- M47.1 condition resolvers (`GetActorValue`/`GetDistance`/`GetFactionRank`/`GetIsID`/
  `HasPerk`, the Global comparand, and the 6 stub branches from #1316) are
  **no longer stubs** — all 13 catalog functions are fully implemented with
  correct Bethesda safe-default sentinels (#1663–#1668, #1316, all closed
  2026-06-29→07-04; re-verified `AUDIT_SCRIPTING_2026-07-16.md` Dimension 6,
  27 passing unit tests). Re-verification against a live headless cell with
  real CTDA data (not just unit tests) remains outstanding — a gap, not a stub.
- The fragment lowerer (b2) as a *wired runtime dispatch* — `effects::lower_fragment`
  + `QuestStageFragments` + `quest_fragment_dispatch_system` exist, but the
  decompiled-`.pex`-fragment → `QuestStageFragments` *population* path may be
  partial; confirm before flagging as a bug vs. a designed Phase-3 gap.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,2,3`). Default: all 7.
- `--depth shallow|deep`: `shallow` = check API contracts + the decline/bounds
  invariants; `deep` = trace each decompiler pass's tree rewrite + the per-frame
  ECS lifecycle. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: PEX Reader & Opcode Decode | Decompiler CFG & Lift | Decompiler Control-Flow / Boolean / Lower | Papyrus Lexer & Pratt Parser | Recognizer-Chain Soundness | Scripting Runtime Systems | Engine Attach & Trigger Wiring
- **Untrusted-Input**: Yes | No (set Yes for any finding on a path that consumes raw `.pex` / `.psc` bytes — these escalate by the special rules below)

## Severity Notes for This Domain

Apply `_audit-severity.md` as written. Domain-specific escalations:

| Condition | Minimum Severity |
|-----------|-----------------|
| Panic / OOB index / unbounded alloc reachable from untrusted `.pex` or `.psc` bytes | HIGH (CRITICAL if it's memory-unsafe — see the `transmute` in `crates/pex/src/opcode.rs`) |
| Decompiler emits a **wrong** AST that a recognizer then matches (false-positive lowering → wrong ECS behavior on vanilla content) | HIGH (silent, all-game blast radius; same class as a wrong NIFAL `Material`) |
| Recognizer emits a component on an **unmodeled** condition/term instead of declining | HIGH (the load-bearing invariant; a quest advancing on the wrong predicate is silent game-logic corruption) |
| Copy-propagation / boolean-collapse soundness bug (folds a temp into the wrong consumer, mis-attributes an `&&`/`||` operand) | HIGH (corrupts the AST the recognizer reads) |
| Stack overflow via unbounded recursion in the parser or a decompiler tree walk | HIGH |
| ECS lock held across a second resource/component mutation (deadlock vector) | HIGH |
| Transient marker not drained / drained out of stage order (re-fires every frame, or fires a frame late) | HIGH |
| `feature-matrix.md` doc-rot, stale comments | LOW |

The decline-on-unmodeled invariant is the scripting analogue of NIFAL's
single-boundary rule: **a partial / approximate lowering is worse than no
lowering**, because an inert unrecognized script is safe but a wrongly-lowered
one corrupts game state with no fallback to mask it.

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/scripting`
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 300 --json number,title,state,labels > /tmp/audit/scripting/issues.json`
4. **Read the most recent `docs/audits/AUDIT_SCRIPTING_*.md` report** (sort by
   date — do not hardcode a filename here, it rots every cycle). Diff direction
   against it rather than re-litigating settled findings. In particular, the
   M47.1 condition-resolver stubs (#1663–#1668, #1316) that earlier reports
   tracked as open are now CLOSED and fully implemented — verify against the
   live `crates/scripting/` code before flagging any condition-resolver gap,
   don't assume the stub-era finding still applies.
5. Read the three crate module docstrings + `docs/engine/m47-2-design.md` §"Frontends
   in detail" and §"Risks & mitigations" to confirm what is *designed* to decline /
   defer vs. what is a real defect, before reporting any "missing handling" finding.

## Phase 2: Launch Dimension Agents

Ordered by bug risk: untrusted decode + the five decompiler passes first
(Dims 1–3), then the source parser, the recognizer invariant, the runtime
lifecycle, and the engine wiring.

### Dimension 1: `.pex` Reader & Opcode Decode (untrusted input)
**Entry points**: `crates/pex/src/reader.rs` (`Reader`, `read_binary`, `read_header`,
`read_string_table`, `read_debug_info`, `skip_property_groups`, `skip_struct_orders`,
`read_objects`, `read_struct_infos`, `read_variables`, `read_guards`, `read_properties`,
`read_states`, `read_named_functions`, `read_function`, `read_typed_names`,
`read_instructions`, `value`, `string`, `string_index`, `take`); `crates/pex/src/opcode.rs`
(`OpCode`, `from_u8`, `MAX_OPCODE`, the `OPCODES` table); `crates/pex/src/model.rs`
(`Pex`, `Object`, `Function`, `Instruction`, `Value`, `ScriptType`); `crates/pex/src/lib.rs`
(`parse`, `PexError`).
**Checklist**:
- **`take(n)` is the single bounds gate.** Every primitive read funnels through
  `take` (`checked_add` + `<= data.len()` filter → `UnexpectedEof`). Verify NO
  read path bypasses it (a direct `self.data[...]` slice, a `try_into().unwrap()`
  on a short slice). Untrusted-Input: every finding here is Yes.
- **The `OpCode::from_u8` `transmute`.** `crates/pex/src/opcode.rs` does
  `unsafe { transmute::<u8, OpCode>(byte) }` guarded by `byte >= MAX_OPCODE → None`.
  This is memory-safety-critical: confirm (a) `MAX_OPCODE == 51` exactly matches
  the last discriminant (`TryLockGuards = 50`), (b) the enum is `#[repr(u8)]` with
  *contiguous* discriminants 0..=50 (a gap would make a valid-range byte transmute
  to an invalid variant = UB), (c) the guard is `>=` not `>`. The
  `discriminants_match_on_disk_order` + `from_u8_round_trips_and_rejects_oob` tests
  pin this — verify they actually cover every discriminant, not just spot values.
- **`arg_count` drives operand consumption.** `read_instructions` reads exactly
  `op.arg_count()` fixed operands + (if `has_varargs`) a `Value::Integer(n >= 0)`
  count then `n` operands. The `OPCODES` table is the contract. Cross-check every
  row against the UESP Papyrus Assembly spec / Champollion `OPCODES` (the file
  claims a verbatim port) — a wrong arg count desyncs the entire instruction
  stream silently (subsequent opcodes read garbage operands). Spot-check the
  var-arg opcodes (`callmethod`/`callparent`/`callstatic`/`lock_guards`/
  `unlock_guards`/`try_lock_guards`) and the high-arity ones
  (`array_findstruct` = 5, `array_getallmatchingstructs` = 6).
- **`BadVarArgCount`**: a negative or non-integer var-arg count is rejected; the
  count is `n as usize` with `Vec::with_capacity(n as usize)`. Confirm a hostile
  but in-`i32`-range huge `n` (e.g. `i32::MAX`) can't pre-allocate gigabytes
  before `take` fails — `with_capacity(2^31)` is a reachable DoS even though the
  reads will EOF. (Same hazard on every `with_capacity(count)` fed by an
  attacker-controlled `u16`/`u32`: `read_string_table`, `read_objects`,
  `read_instructions`, `read_typed_names`, `read_struct_infos`. The `u16` ones cap
  at 65535 = benign; the `u32` user-flags / var-arg / object-size are the ones to
  scrutinize.)
- **`string_index` range check**: a `u16` index is `.get(idx).cloned()` →
  `BadStringIndex` on miss. Verify NO field reads a raw `u16` and indexes
  `self.strings[idx]` directly (panic on OOB).
- **`value()` type tag**: only 0..=5 accepted (`BadValueType` otherwise). Confirm
  the six arms match `ValueType` and that `Value::Integer(self.u32()? as i32)`
  sign-reinterprets (not truncates) — Papyrus ints are signed.
- **Endianness / dialect detection**: magic LE (`0xFA57C0DE`) vs BE
  (`0xDEC057FA`) sets `endian`; `script_type` derives from endian + `game_id`
  (4→Starfield, 3→FO76, else FO4; BE→Skyrim). `u32_opt(true)` reads the magic LE
  before endian is known — verify every *other* multi-byte read honors `self.endian`
  and that the provisional `Endian::Little` in `new()` can't leak into a read
  before `read_header` sets it.
- **Skyrim-vs-FO4+ field gating**: `is_skyrim()` skips `const_flag`, `struct_infos`,
  property-group / struct-order debug tables; Starfield-only `guards`. A misgated
  field shifts the whole stream. Verify `read_objects` reads fields in the exact
  FileReader order and that `skip_property_groups`/`skip_struct_orders` consume the
  same bytes the (FO4+) writer emits (the doc says "consume-and-discard to stay
  aligned" — a wrong skip count corrupts every following object).
- **No partial `Pex` escapes**: `lib.rs` claims the reader "never returns a
  half-built `Pex`". Confirm `read_binary` is all-or-`Err` (no `Ok` with a
  truncated `objects` Vec on a mid-object EOF).
- Regression guards: `parses_a_handbuilt_fo4_pex`, `parses_a_handbuilt_skyrim_be_pex`,
  `parses_a_handbuilt_starfield_pex_with_guards`, `rejects_bad_magic`,
  `rejects_truncation` (`crates/pex/src/lib.rs`); `metadata_matches_champollion`
  (`crates/pex/src/opcode.rs`). FO4/LE, Skyrim/BE, and Starfield-guards dialects
  all round-trip via `PexWriter::new_be()` + the two new tests (#1728) — the
  prior MEDIUM coverage gap (hand-built writer only exercising FO4/LE) is closed;
  a future writer regression that drops the BE or guards path re-opens it.
**Output**: `/tmp/audit/scripting/dim_1.md`

### Dimension 2: Decompiler — CFG Construction & Opcode→Node Lift (highest bug density)
**Entry points**: `crates/pex/src/decompile/cfg.rs` (`build_cfg`, `CodeBlock`,
`Cfg`, `split`, `split_block`, `find_block_for_instruction`, `checked_target`,
`condition_name`, `END`); `crates/pex/src/decompile/lift.rs` (`lift_function`,
`create_node`, `check_assign`, `rebuild_expression`, `count_constant_id`,
`replace_constant_id`, `build_var_types`); `crates/pex/src/decompile/node.rs`
(`Node`, `NodeKind`, `is_final`, `is_temp_var`, `child_nodes`, `child_nodes_mut`,
`SYNTH_IP`).
**Checklist**:
- **Jump-target bounds**: `checked_target` validates `0 <= ip+offset <= count`
  (inclusive — the exit anchor is one past last). `build_cfg` errors on a
  non-integer offset (`BadJumpOffset`) or OOB (`JumpOutOfRange`). Verify the
  inclusive bound is correct (a jump to `count` lands on the synthetic exit block,
  not OOB) and that `condition_name` rejects non-{ident,bool,int} conditions
  (`BadJumpCondition`). Untrusted-Input: Yes.
- **Block-split arithmetic**: `CodeBlock::split(at)` truncates to `[begin, at-1]`
  and emits tail `[at, end]`. `at` is always `ip+1` or a jump target ≥ 1, so
  `at-1` can't underflow — confirm `split` is never called with `at == 0` (the
  initial full block starts at 0 and the exit anchor pre-exists, so `ip+1` for the
  final instruction maps to the anchor without a split). A `split(0)` is an
  underflow panic.
- **`jmpf` vs `jmpt` edge polarity**: `jmpf` jumps when FALSE, so true-edge =
  fall-through (`ip+1`), false-edge = target; `jmpt` is mirrored. This is the
  load-bearing CFG semantic — a flipped polarity inverts every `If`. The
  `forward_jmpf_builds_an_if_diamond` + `backward_jmpt_builds_a_loop_edge` tests
  pin it; verify the `(on_true, on_false)` tuple in `build_cfg` matches.
- **Copy-propagation soundness (`rebuild_expression`)**: a non-final
  (temp-producing) node is folded into the *single* following statement that
  consumes its result via `count_constant_id`; **0 → skip, 1 → inline (and restart
  at `i=0`), >1 → `ExpressionRebuildFailed`**. This is the AST-correctness core.
  Verify: (a) the count is over the *immediately next* statement only
  (`scope[i+1]`) — Champollion's single-consumer model; folding into a
  non-adjacent consumer would reorder side effects; (b) `is_final` / `is_temp_var`
  asymmetry is intact (`is_final` treats any `::temp` prefix as non-final incl.
  `_var`-suffixed; `is_temp_var` excludes `_var`) — the file documents this as a
  deliberate Champollion port, so a "cleanup" that unifies them is a regression;
  (c) the `replace_constant_id` `slot.take()` substitutes exactly once (the
  `debug_assert!(slot.is_none())` only fires in debug — a release build with a
  >1-match that slipped past `count_constant_id` would silently drop the producer).
- **`create_node` opcode→node map**: each opcode maps to a `NodeKind`. Spot-check
  the precedence values passed (they're cosmetic for AST lowering but the file
  carries them); the **`Cast` heuristic** (lift.rs): a cast is downgraded to a
  `Copy` when source is `None`, or when both sides are same-typed identifiers
  (or src is `::nonevar`). Verify the same-type test uses `type_of` on *both* and
  the `::nonevar` case-insensitive exception — a wrong downgrade turns a real
  type-narrowing cast into an identity copy (recognizer reads the wrong type).
- **`CallStatic`/`CallMethod`/`CallParent` operand order**: result, object, method
  name are pulled from specific arg indices (`id(2)`/`val(1)`/`id(0)` for
  CallMethod; `val(0)`/`id(1)`/`id(2)` for CallStatic). A swapped index mis-names
  the called function — fatal for a recognizer that keys on the method name
  (`SetStage`, `GetStageDone`). Cross-check against the UESP opcode operand order.
- **`id_of` on a literal**: operands that must be identifiers (`id(n)`) error with
  `ExpectedIdentifier` on a literal. Verify the lift never `unwrap()`s
  `as_identifier()` outside a checked branch (the Cast arm does
  `src.as_identifier().unwrap()` — confirm it's guarded by the preceding
  `matches!(src, Value::Identifier(_))` short-circuit).
- **Bodyless / native functions**: `build_cfg` returns `entry == END` for zero
  instructions; `lift_function` yields empty scopes. Verify a native/abstract
  function (no body) decompiles to an empty body, not a panic.
- **`Vec::with_capacity(op.arg_count())`** in lift — bounded (≤ 6), benign.
- Regression guards: `temp_folds_into_its_single_consumer`,
  `chained_temps_fold_into_one_expression`, `call_with_inlined_argument`,
  `property_set_lowers_to_assign_of_property_access`,
  `cast_between_different_types_is_a_cast_not_a_copy`,
  `double_use_of_a_temp_is_an_error` (`crates/pex/src/decompile/lift.rs`);
  `bodyless_function_yields_empty_cfg`, `straight_line_is_one_block_plus_exit`,
  `forward_jmpf_builds_an_if_diamond`, `backward_jmpt_builds_a_loop_edge`,
  `jump_out_of_range_is_an_error`, `non_integer_jump_offset_is_an_error`
  (`crates/pex/src/decompile/cfg.rs`).
**Output**: `/tmp/audit/scripting/dim_2.md`

### Dimension 3: Decompiler — Control-Flow, Short-Circuit Booleans & AST Lowering
**Entry points**: `crates/pex/src/decompile/control_flow.rs` (`reconstruct`,
`Reconstructor`, `rebuild`, `before_exit`, `take_scope`);
`crates/pex/src/decompile/boolean.rs` (`rebuild_boolean_operators`, `BoolPass`,
`collapse`, `last_result`, `take_operand`, `combine`, `BoolOp`);
`crates/pex/src/decompile/lower.rs` (`decompile_script`, `decompile_body`,
`lower_expr`, `lower_stmt`, `lower_body`, `build_handler`, `lower_property`,
`lower_type`, `lower_binary_op`); `crates/pex/src/decompile/event_names.rs`
(`is_event_name`, `EVENT_NAMES`); pipeline order in
`crates/pex/src/decompile/mod.rs`.
**Checklist**:
- **Pass order is load-bearing**: `decompile_body` runs cfg → lift →
  **`rebuild_boolean_operators` (before)** → `reconstruct` → `lower_body`. The
  boolean pass MUST precede control-flow reconstruction (it collapses `&&`/`||`
  short-circuit chains into one conditional so the CF pass sees a clean diamond).
  Verify the order; a swap leaves `||` chains as the "unmerged conditional `last`"
  case in `control_flow.rs` (which the file documents it *skips* — see below).
- **Control-flow shape classification (`rebuild`)**: reads structure off block
  edges — While (body tail jumps back to the condition: `last.next == current`),
  simple If (`last.next == exit`), If/Else (else). The **jmpt inversion** negates
  the condition and swaps edges when `before == current`. Verify the
  while/if/if-else discriminants against the edge invariants and that
  `before_exit` returns the block containing `exit-1` (the degenerate `exit == 0`
  returns `END` → `fail()`).
- **The deliberate skip in `control_flow.rs`**: when `last` is *itself*
  conditional, the block is "left unmerged, advance by one" — this is the `||`
  short-circuit case the boolean pre-pass is supposed to have already collapsed.
  Confirm that with the boolean pass running first this branch is unreachable for
  well-formed input; if a script reaches it (boolean pass declined to collapse),
  the CF pass **silently drops a guard** — that's a wrong-AST hazard (a guarded
  effect becomes unguarded). Assess whether such a script errors, declines, or
  silently mis-decompiles.
- **Boolean collapse soundness (`boolean.rs`)**: `&&` = true edge falls through
  (`block.on_true() == block.end + 1`), `||` = false edge falls through. The
  operand block must *recompute the same condition variable* (`take_operand`
  checks `result == cond`). The file documents **two deliberate departures from
  Champollion**: (1) NO debug-line guard (it relies on the structural signal
  alone — Champollion uses per-instruction source lines to reject cross-line
  merges); (2) a termination guard (only re-process on a real merge). Audit both:
  for (1), reason about whether a non-`&&`/`||` block that *happens* to recompute
  a same-named temp on its fall-through edge could be falsely collapsed (a
  false-positive merge fabricates a boolean operator that wasn't in the source —
  wrong AST). The file says this is "validated against the corpus decompile rate +
  the R5 fidelity gate" — point the finding at those instruments, not speculation.
  For (2), confirm the re-process loop strictly shrinks the graph (merges a
  non-exit rejoin) so it terminates — an infinite loop here hangs the decompiler.
- **`combine` precedence + assign preservation**: `&&` = prec 7, `||` = prec 8;
  an enclosing `Assign` is rebuilt around the combined op. Verify the operand
  unwrap in `take_operand` (`std::mem::replace(value, Constant(None))`) leaves no
  dangling `None` in the tree.
- **AST lowering totality (`lower.rs`)**: `lower_expr` / `lower_stmt` must be
  total (no panic on any `NodeKind`). Note the *intentional* lossy lowerings —
  flag them only if a recognizer keys on the lost info:
  (a) statement-shaped nodes appearing as sub-expressions → `Expr::NoneLit`
  (should be unreachable; if reachable it's a lift bug);
  (b) `is` type-test → `Cast` (no AST `is`);
  (c) `StructCreate` → `New` with size 0;
  (d) `lower_binary_op` default arm → `BinaryOp::Eq` (a comment says "shouldn't
  reach here" — a real unknown op silently becomes `==`, which would corrupt a
  condition; verify only the modeled op strings reach it).
- **Event-vs-function classification (`build_handler`)**: a name is an `Event`
  iff (`on`-prefixed AND `is_event_name`) OR `::remote_`-prefixed. `EVENT_NAMES`
  is a sorted lowercase union (Skyrim+FO4+Starfield) binary-searched by
  `is_event_name`. Verify the list stays sorted (the `list_is_sorted_for_binary_search`
  test guards it) — an unsorted entry makes `binary_search` miss it, demoting a
  real event handler to a plain function (recognizers that look for `OnActivate`
  as an `Event` would miss it). A *missing* engine event in the union is the same
  bug; spot-check that high-frequency events from the recognizer-scaling doc
  (`onactivate`, `onload`, `ontriggerenter`, `onhit`, `ontimer`, `oninit`,
  `onupdate`) are all present.
- **`decompile_script` assembly**: synthetic `::`-prefixed variables dropped;
  auto-state functions → script-scope items, named states → `State` items;
  property getter/setter bodies decompiled via `build_named_function`. Verify the
  auto-state match uses `state.name == object.auto_state_name` (a Skyrim
  empty-string auto-state vs FO4 named auto-state both handled).
- **The 99.996% claim**: this dimension owns verifying the corpus-smoke harness
  (`crates/pex/examples/pex_corpus_smoke.rs`) actually decompiles (not just
  parses) every `.pex` and counts panics/`Err` as failures. The README/docs claim
  26640/26641 — confirm the harness's `decompile_script` call is inside the
  success/failure tally and that a panic isn't caught-and-counted-as-success.
- **Recursion-depth caps**: both `control_flow.rs::Reconstructor::rebuild` and
  `boolean.rs::BoolPass::rebuild` thread a `depth` param capped at
  `MAX_REBUILD_DEPTH = 1024`, erroring `DecompileError::RecursionLimit` rather
  than overflowing the stack (control-flow: pre-existing #1729; boolean: #1815/
  SCR-D2-01, fixed by `7fdb694b`). Verify both still cap — a "cleanup" that drops
  the boolean-pass thread regresses #1815. Regression guards: the
  `rebuild_rejects_excessive_recursion_depth` test exists in **both**
  `control_flow.rs` and `boolean.rs` (same name, distinct files/tests).
- Regression guards: `simple_if_reconstructs`, `if_else_reconstructs_both_branches`,
  `while_loop_reconstructs`, `nested_and_becomes_nested_ifs`,
  `straight_line_has_no_control_flow_nodes` (`crates/pex/src/decompile/control_flow.rs`);
  `and_collapses_to_a_single_if_with_an_and_condition`,
  `or_collapses_to_a_single_if_with_an_or_condition`,
  `plain_if_is_untouched_by_the_boolean_pass`,
  `straight_line_with_a_call_is_unchanged` (`crates/pex/src/decompile/boolean.rs`);
  `an_on_activate_function_lowers_to_an_event`, `a_plain_function_stays_a_function`,
  `an_if_with_a_call_lowers_to_an_if_statement`, `auto_property_lowers_with_auto_flag`
  (`crates/pex/src/decompile/lower.rs`); `list_is_sorted_for_binary_search`,
  `known_events_match_case_insensitively` (`crates/pex/src/decompile/event_names.rs`).
**Output**: `/tmp/audit/scripting/dim_3.md`

### Dimension 4: Papyrus `.psc` Lexer & Pratt Parser (untrusted input)
**Entry points**: `crates/papyrus/src/lib.rs` (`parse_script`, `parse_expr`);
`crates/papyrus/src/token.rs` (logos `Token`, `ignore(ascii_case)` keyword
attrs, the `Ident` regex); `crates/papyrus/src/lexer.rs` (`preprocess`,
`OffsetMap`); `crates/papyrus/src/parser/expr.rs` (`parse_expr_bp`,
`parse_expr_bp_inner`, `MAX_EXPR_DEPTH`, `PREC_*`); `crates/papyrus/src/parser/mod.rs`
(`expr_depth`); `crates/papyrus/src/parser/stmt.rs`; `crates/papyrus/src/parser/script.rs`
(`skip_to_next_line`, item recovery); `crates/papyrus/src/ast.rs`
(`BinaryOp::precedence`); `crates/papyrus/src/error.rs` (`ExpressionTooDeep`).
**Checklist**:
- **Recursion-depth cap (`MAX_EXPR_DEPTH = 256`, #1270 / SAFE-DIM3-NEW-02)**:
  `parse_expr_bp` increments `expr_depth` at entry, returns
  `ExpressionTooDeep` at the cap, decrements at exit. This is the stack-overflow
  guard against pathological `((((…))))`. Verify (a) the increment/decrement is
  balanced on every return path including the error path (a missed decrement
  would falsely cap legitimate sibling expressions); (b) ALL recursive expression
  entry funnels through `parse_expr_bp` (no direct `parse_expr_bp_inner` recursion
  that bypasses the gate); (c) the *statement* parser (`stmt.rs`) has its own
  guard: `stmt_depth`/`MAX_STMT_DEPTH = 256` (#1712) mirrors `expr_depth` and
  caps nested `If`/`While` block recursion — verify it still resets between
  top-level calls and rejects pathological nesting (guards:
  `stmt_depth_cap_rejects_pathological_nested_if`,
  `stmt_depth_cap_rejects_pathological_nested_while`,
  `stmt_depth_cap_accepts_legitimate_nesting`,
  `stmt_depth_resets_between_top_level_calls`). Untrusted-Input: Yes.
- **Operator precedence + associativity (`ast.rs` `BinaryOp::precedence`)**:
  Or=1, And=2, comparisons=3, Add/Sub/StrCat=4, Mul/Div/Mod=5; unary=6, cast=7,
  postfix=8. Left-associativity hinges on the Pratt loop's `op_prec <= min_bp →
  break` (the `<=`, not `<`). Verify `a - b - c` → `(a-b)-c`
  (`test_left_associativity`) and `a + b * c` → `a + (b*c)`. Note: Papyrus's
  *runtime CTDA* OR-precedence quirk (Bethesda's inverted AND/OR) is a *condition
  evaluation* concern (Dim 6) — the `.psc` source operators here are standard.
- **Line-continuation preprocessing (`lexer.rs::preprocess`)**: a `\` immediately
  before `\n` / `\r\n` / lone `\r` is elided (2 / 3 / 2 bytes) and recorded in
  `OffsetMap` for span remap; any other `\` passes through. Verify the `\r`-only
  ("Mac classic") branch and that the `OffsetMap` byte counts (2/3/2) exactly
  match the elided bytes — a wrong count drifts every subsequent error span. Edge:
  a trailing `\` at EOF (no following newline) — confirm it's emitted, not
  swallowed (an OOB peek).
- **Case-insensitive keywords**: every keyword `#[token(..., ignore(ascii_case))]`;
  identifiers preserve case via the `Ident` regex (`priority = 1`). Verify a
  keyword-shaped identifier (e.g. a variable literally named `state`) is handled
  per Papyrus rules — logos keyword tokens win over the lower-priority `Ident`
  regex, so `state` always lexes as the keyword. Flag if that breaks any legal
  vanilla identifier (Papyrus reserves these, so it's likely correct — confirm
  against the grammar in `docs/engine/papyrus-parser.md` rather than assuming).
- **Error recovery**: `parse_script` returns `Ok((Script, Vec<ParseError>))` for
  partial success (collects per-item errors, `skip_to_next_line`, continues) and
  `Err` only for fatal failures (missing `ScriptName`, lex error). `parse_expr`
  bails on first error. Verify `skip_to_next_line` always makes progress (consumes
  ≥1 token) — a recovery point that doesn't advance is an infinite loop on a
  malformed item. Confirm callers that need strict-fail check `result.1.is_empty()`.
- **Integer/literal parsing**: hex (`test_hex_literal`), negative ints, floats —
  verify no `unwrap()` on `str::parse` that a malformed-but-lexable literal could
  panic on (lexer should reject before parse, but confirm the seam).
- Regression guards (sample — there are ~56): `depth_cap_rejects_pathological_parens`,
  `depth_cap_accepts_legitimate_nesting`, `depth_resets_between_top_level_calls`
  (`crates/papyrus/src/parser/expr.rs`); `test_left_associativity`,
  `test_precedence_mul_over_add`, `test_precedence_and_over_or`,
  `test_cast_precedence` (`crates/papyrus/src/parser/expr.rs`);
  `test_preprocess_line_continuation`, `test_preprocess_crlf_continuation`,
  `test_lex_case_insensitive_keywords` (`crates/papyrus/src/lexer.rs`);
  `parse_full_rumble_on_activate_translation` (`crates/papyrus/src/parser/script.rs`).
**Output**: `/tmp/audit/scripting/dim_4.md`

### Dimension 5: Recognizer-Chain Soundness (decline-on-unmodeled — the load-bearing invariant)
**Entry points**: `crates/scripting/src/translate/mod.rs` (`translate_script`,
`translate_pex`, `RECOGNIZERS`); `crates/scripting/src/translate/archetype.rs`
(`RecognizeCtx`, `Recognized`, `SpawnFn`, `Recognizer`);
`crates/scripting/src/translate/source.rs` (`ScriptSource`);
`crates/scripting/src/translate/compose.rs` (`split_and`, `classify_guard_atom`,
`GuardPrimitive`, `GUARD_PRIMITIVES`, `GuardMatch`, `quest_via`, `QuestRef`,
`prim_player_gate`, `prim_stage_done`); `crates/scripting/src/translate/effects.rs`
(`lower_fragment`, `classify_effect`, `EffectPrimitive`, `EFFECT_PRIMITIVES`,
`Effect`); `crates/scripting/src/translate/tables.rs` (`CanonicalEvent::from_papyrus`);
`crates/scripting/src/translate/recognizers/quest_stage_gate.rs` (`recognize`,
`extract_stage_gate`, `classify_if_condition`);
`crates/scripting/src/translate/recognizers/rumble.rs` (`recognize`).
**Checklist**:
- **The invariant**: a recognizer MUST return `None` (decline) on ANY unmodeled
  condition atom, effect statement, or unbindable hole — never emit a component
  built from a partial / approximated match. A false-positive lowering silently
  corrupts game logic (quest advances on the wrong predicate) with no fallback.
  This is the scripting analogue of NIFAL's no-fabrication rule.
- **Chain ordering (`mod.rs` `RECOGNIZERS`)**: per-script recognizers FIRST
  (`rumble`), generic families SECOND (`quest_stage_gate`), so a bespoke script
  isn't swallowed by a family match. `translate_script` is
  `RECOGNIZERS.iter().find_map(...)` — first match wins, all-`None` → silent miss.
  Verify the order matches the design (per-script before generic) and that adding
  a future generic recognizer can't shadow `rumble`.
- **Guard decline enforcement (`compose.rs` + `quest_stage_gate.rs`)**: the
  load-bearing decline is `classify_guard_atom(atom, player_param)?` inside the
  per-atom loop in `classify_if_condition` — the `?` propagates `None` the instant
  an atom isn't claimed by `GUARD_PRIMITIVES`. Verify (a) the loop does NOT skip /
  ignore an unmatched atom (no `if let Some(..) = ... { }` that silently drops a
  `None`); (b) **`split_and` deliberately does NOT split `||`** — a disjunction is
  left as one atom no primitive matches, forcing a decline. This is intentional
  conservatism (the file documents it). Confirm an `If a || b` condition declines
  rather than lowering only the `a` half.
- **Effect decline enforcement (`effects.rs::lower_fragment`)**: a flat-sequence
  model — `Stmt::ExprStmt(e) → classify_effect(&e.node, &env)?` and
  `_ => return None` for ANY control flow / valued return / var-decl. An
  assignment binds a quest-ref local (`quest_expr_ref(...)?`) or declines. Verify
  no statement type is silently accepted-as-noop except the explicit
  `Stmt::Return(None)`.
- **Hole binding**: `QuestRef::{OwningQuest, SelfRef, Property(name)}` must FULLY
  resolve. `OwningQuest` needs `ctx.owning_quest` (decline if `None` —
  `declines_when_owning_quest_unavailable`); `Property(name)` needs the VMAD
  `script_instance` to carry that property as a form-id (decline if unbound —
  `declines_when_quest_property_unbound`); `SelfRef` on a REFR is declined
  (quest scripts attach to a quest, not a REFR). Verify each binding failure
  declines, never defaults to form-id 0.
- **`quest_stage_gate` cross-check**: when the condition's quest and the
  `SetStage` target's quest disagree, the recognizer declines (don't advance the
  wrong quest). Verify `recognizes_da10_and_reproduces_hand_builder` (`.psc`-side,
  `quest_stage_gate.rs`) and `da10_pex_reproduces_hand_builder_byte_for_byte`
  (`.pex`-side, `crates/scripting/tests/pex_recognize_e2e.rs`, `#[ignore]`-gated
  on Skyrim SE game data, #1740) both assert byte-equality against
  `da10_main_door(...)` — together they are the full `.psc`-vs-`.pex` fidelity
  gate for this recognizer (the `.psc`-side test alone never touches
  `decompile_script`).
- **`rumble` per-script recognizer**: matches script name `defaultRumbleOnActivate`
  (case-insensitive) and extracts 5 auto-property float/bool initial values with
  `.psc` defaults; declines a non-literal property value and a different script
  name. Verify the literal-only extraction (a property initialized by an
  expression must decline, not coerce).
- **`CanonicalEvent::from_papyrus`** (`tables.rs`): a fixed lowercase-keyed
  catalog; unknown → `CanonicalEvent::Unknown` (a safe long-tail bucket, not an
  error). Verify the case-insensitive match and that `Unknown` callers treat it as
  "no consumer", never as a wildcard match.
- **`translate_pex` clean-`None` on bad bytes AND on panic**: `byroredux_pex::parse` /
  `decompile_script` `Err` → `log::debug` + `return None` (never a panic
  escaping into the cell loader). Guards: `translate_pex_on_empty_bytes_is_a_clean_none`,
  `translate_pex_on_garbage_bytes_is_a_clean_none`,
  `translate_pex_on_truncated_after_magic_is_a_clean_none`. A `decompile_script`
  **panic** is also caught via `catch_unwind` (`crates/scripting/src/translate/mod.rs`,
  #1816/SCR-D5-NEW-02) and degraded to the same `None` — verify the wrap is
  still present, not removed by a future refactor (no corpus `.pex` or
  characterized input currently triggers it — this is a safety net, not an
  active-bug regression test).
- Regression guards: `unrecognized_script_is_a_silent_miss` (`crates/scripting/src/translate/mod.rs`);
  `split_and_flattens_conjunction_keeps_disjunction_whole`, `unmodeled_atom_declines`,
  `stage_done_primitive_binds_holes`, `player_gate_primitive_matches_both_orders`
  (`crates/scripting/src/translate/compose.rs`); `declines_on_unmodeled_effect`,
  `declines_on_control_flow`, `empty_fragment_is_understood_as_noop`
  (`crates/scripting/src/translate/effects.rs`); the 14 `quest_stage_gate.rs`
  tests incl. `declines_unmodeled_condition_term`, `declines_handler_without_set_stage`,
  `declines_when_quest_property_unbound`, `declines_unconditional_with_extra_statements`
  (`crates/scripting/src/translate/recognizers/quest_stage_gate.rs`);
  `recognizes_rumble_and_extracts_psc_defaults`, `declines_a_different_script`
  (`crates/scripting/src/translate/recognizers/rumble.rs`);
  `canonical_event_unknown_for_long_tail` (`crates/scripting/src/translate/tables.rs`).
**Output**: `/tmp/audit/scripting/dim_5.md`

### Dimension 6: Scripting Runtime Systems — Lifecycle, Stage & Lock Ordering
**Entry points**: `crates/scripting/src/lib.rs` (`register`);
`crates/scripting/src/events.rs` (the marker structs);
`crates/scripting/src/timer.rs` (`timer_tick_system`, `ScriptTimer`);
`crates/scripting/src/cleanup.rs` (`event_cleanup_system`);
`crates/scripting/src/condition.rs` (`evaluate`, `evaluate_condition`,
`evaluate_function`, `ConditionFunction`, `ConditionContext`);
`crates/scripting/src/trigger.rs` (`trigger_detection_system`, `TriggerVolume`,
`TriggerShape`, `contains`); `crates/scripting/src/quest_stages.rs`
(`QuestStageState`, `QuestObjectiveState`, `set_stage`, `get_stage_done`);
`crates/scripting/src/fragment.rs` (`quest_fragment_dispatch_system`,
`QuestStageFragments`, `apply_effects`, `MAX_CASCADE`);
`crates/scripting/src/recurring_update.rs` (`recurring_update_tick_system`,
`RecurringUpdate`, `OnUpdateEvent`); `crates/scripting/src/registry.rs`
(`ScriptRegistry`).
**Checklist**:
- **Two-phase lock-drop discipline**: `timer_tick_system`,
  `trigger_detection_system`, and `recurring_update_tick_system` each Phase-1
  hold a `query_mut::<T>()`, collect a `Vec` of entities to act on, **`drop()` the
  lock**, then Phase-2 acquire a *different* `query_mut` to insert markers. Verify
  the explicit `drop()` precedes the second acquisition in every one — holding two
  component-mut locks at once forces the TypeId-sorted-acquisition contract and is
  a deadlock vector. `quest_fragment_dispatch_system` holds three *resource* locks
  (`QuestStageFragments` read + `QuestStageState` mut + `QuestObjectiveState` mut)
  — verify they're acquired in a single scoped block with no component lock held
  across them.
- **Marker single-frame semantics**: all transient markers
  (`ActivateEvent`, `HitEvent`, `TimerExpired`, `AnimationTextKeyEvents`,
  `OnUpdateEvent`, `OnTriggerEnterEvent`, `OnCellLoadEvent`, `OnEquipEvent`,
  `QuestStageAdvanced`, and the rumble/camera/UI command markers) MUST be drained
  by `event_cleanup_system` exactly once per frame. Verify `event_cleanup_system`
  drains EVERY marker type the runtime emits (cross-check the
  `cleanup.rs` drain list against every `world.insert` of a marker across the
  crate) — an undrained marker re-fires its consumer every frame; a marker
  emitted by a system that runs *after* cleanup lags a frame. `cleanup` must be
  the LAST scripting system in the schedule. Guards: `cleanup_removes_all_event_types`,
  `cleanup_preserves_non_event_components`.
- **Producer→consumer cross-stage ordering**: `quest_advance_system` (Dim 7)
  emits `QuestStageAdvanced`; `quest_fragment_dispatch_system` consumes it and may
  re-emit (cascade). Verify the cascade is bounded by `MAX_CASCADE = 64` with a
  WARN on overflow (an unbounded `SetStage`→fragment→`SetStage` loop hangs the
  frame) and that only *genuine* transitions cascade (a no-op re-set of the same
  stage is skipped — the `fragment.rs` cascade guard).
- **CTDA OR-precedence (`condition.rs::evaluate`)**: Bethesda's **inverted**
  precedence — consecutive `or_next`-flagged conditions form an OR block that
  binds *tighter* than the surrounding AND chain (`A AND B OR C AND D` =
  `A AND (B OR C) AND D`). The block scan walks while `conditions[i].or_next`,
  OR-combines the block with `.any()`, AND-combines blocks with early-return on a
  false block. Verify the block-boundary logic (the last condition of a block has
  `or_next == false`) and the empty-list → `true` contract. Guards:
  `or_precedence_quirk_a_and_b_or_c_and_d_groups_b_or_c`,
  `or_precedence_quirk_swap_test_a_true`, `and_chain_short_circuits_on_first_false`,
  `or_block_returns_true_when_any_member_true`, `empty_condition_list_returns_true`.
- **Condition stubs are KNOWN (#1663–#1668, #1316)**: `GetActorValue`/`GetDistance`/
  `GetFactionRank`/`GetIsID`/`HasPerk` return documented safe-defaults (the
  Bethesda "unknown-function safe-default" / "not in faction" = -1.0 sentinels).
  Do NOT re-file these. DO verify the *safe-default values* are correct (a wrong
  sentinel flips a condition) and that `RunOn` resolution declines (condition
  fails) on an unresolvable target rather than defaulting to subject.
- **Edge-triggered trigger detection (`trigger.rs`)**: `trigger_detection_system`
  fires `OnTriggerEnterEvent` ONLY on the outside→inside transition
  (`inside && !occupant_inside`), updates `occupant_inside` each frame, fires
  again on re-entry. The event lands on the **volume entity** with the triggerer
  in the marker field. Verify the seed contract (a player loaded already inside a
  volume must NOT spuriously fire on frame 1 — `occupant_inside` seeded true) and
  the `contains` math: Sphere = `(p-center).length_squared() <= r*r` with
  `half_extents.x` as radius; Box (OBB) = `rotation.inverse() * (p-center)` then
  per-axis `local.abs() <= half_extents`. Guards: `edge_triggered_not_level_triggered`,
  `re_entry_fires_again`, `sphere_contains_by_radius`, `obb_rotation_is_respected`,
  `aabb_contains_interior_and_rejects_exterior`.
- **Quest stage history (`quest_stages.rs`)**: `set_stage` updates `current_stage`
  AND inserts into `stages_done` (history retained across advances —
  `GetStageDone(37)` stays true after advancing to 40); `set_stage` returns the
  previous current; backward set is allowed; `reset` clears one quest only. Guards:
  `get_stage_done_retains_history_across_advances`,
  `set_stage_on_already_done_stage_remains_idempotent`, `reset_leaves_other_quests_intact`.
- **`recurring_update_tick_system`**: a fresh `RecurringUpdate` does NOT fire on
  the registering frame / zero dt; fires once per interval; re-arms after fire; a
  long-frame dt overshoot fires once (not a burst); `UnregisterForUpdate` inside a
  handler terminates cleanly. Guards: `fresh_subscription_does_not_fire_on_zero_dt`,
  `dt_overshoot_fires_only_once_per_tick`, `subscription_re_arms_after_fire`,
  `unregister_inside_handler_terminates_cleanly` (`crates/scripting/src/recurring_update/tests.rs`).
- **`ScriptRegistry`** (M47.0 static path, being retired in favor of the dynamic
  attach): case-SENSITIVE editor-id keys, re-register replaces. Verify no live
  call path still depends on the hardcoded `papyrus_demo::register_spawners` for a
  vanilla REFR (the demos should be test fixtures only — `m47-2-design.md` §"Engine
  integration" says the hardcoded registration is retired). Flag a surviving
  hardcoded-attach call site as a tech-debt / correctness mismatch.
**Output**: `/tmp/audit/scripting/dim_6.md`

### Dimension 7: Engine Attach Path & Trigger-Volume Wiring (engine-side)
**Entry points**: `byroredux/src/cell_loader/references/mod.rs` (`attach_vmad_scripts`,
`attach_script_for_refr`, `trigger_volume_from_primitive`, the invisible-trigger
REFR spawn path); `crates/plugin/src/esm/records/index.rs`
(`base_record_script_instance`); `crates/plugin/src/esm/records/script_instance.rs`
(`ScriptInstanceData`, `ScriptInstance`); `byroredux/src/asset_provider/script.rs`
(`build_script_provider`, `extract_pex`, the `--scripts-bsa` parse);
`crates/scripting/src/papyrus_demo/quest_advance.rs` (`quest_advance_system`,
`QuestAdvanceOnActivate`).
**Why this dimension**: the decompiler + recognizer chain (Dims 1–5) are the
*producer* of canonical components; the cell-loader attach path is the only live
*driver* that feeds them real VMAD + `.pex` from game data. None of the crate
dimensions covers it.
**Checklist**:
- **Silent-miss everywhere (graceful degradation)**: the attach path must NEVER
  panic on missing data — no `--scripts-bsa` (early out), VMAD absent
  (`base_record_script_instance` → `None` → return), `.pex` not in archive
  (`extract_pex` → `None`, trace-log, continue), parse/decompile fail
  (`translate_pex` → `None`, debug-log), recognizer miss (trace-log). Verify every
  branch is a `continue`/`return false`, not an `unwrap`/`expect`. Untrusted-Input:
  Yes (the `.pex` bytes come from a possibly-modded archive).
- **VMAD retention + accessor (`index.rs::base_record_script_instance`)**: checks
  ACTI/CONT/NPC/CREA base records in order, returns the first
  `script_instance.as_ref()`. Verify the record types covered match the
  VMAD-bearing set (a scripted base type not in the chain → its scripts never
  attach). Confirm the accessor is keyed by `base_form_id` (the REFR's base, not
  the REFR's own form id) and that a REFR's *own* VMAD (Skyrim+ supports per-REFR
  scripts) is also resolved — flag if only base-record VMAD is consulted (per-REFR
  override scripts would be dropped).
- **`.pex` resolution (`asset_provider.rs`)**: `extract_pex` normalizes a VMAD
  script name → `scripts\<name>.pex` (backslash, lowercase, `scripts\` prefix).
  `--scripts-bsa` is **repeatable**, first-archive-hit-wins (mod layering). Verify
  the path normalization matches the on-disk archive convention (a wrong prefix /
  separator → every `.pex` miss → zero scripts attach silently) and that the
  repeatable-flag layering order is mod-over-vanilla (later `--scripts-bsa`
  archives should override earlier — confirm the iteration order vs. the
  first-hit-wins semantics actually achieve that).
- **XPRM → `TriggerVolume` half-extent convention** (`trigger_volume_from_primitive`):
  XPRM `bounds` are Bethesda **z-up HALF-extents** (CK Primitive convention,
  consistent with `bhkBoxShape` half-extents) — the code must NOT divide by 2.
  Verify (a) no `/ 2.0`; (b) the z-up→y-up permute is `[x, z, y]` (bounds[0],
  bounds[2], bounds[1]) with `.abs()` (extents are magnitudes); (c) the REFR
  `scale` is baked in (world-space volume); (d) sphere uses `bounds[0]` as radius
  into `half_extents.x`; (e) shape dispatch `1 → Box`, `3 → Sphere`, other →
  `None` (line/portal/plane are non-containment). A wrong half/full or a wrong
  permute makes every trigger box the wrong size/shape → quests fire at the wrong
  position or never. Guards: `trigger_volume_from_box_primitive_permutes_and_scales`,
  `trigger_volume_from_sphere_primitive_uses_radius`.
- **Invisible (MODL-less) trigger REFR spawn**: a scripted trigger REFR with no
  mesh spawns an entity with `Transform`/`GlobalTransform`/`TriggerVolume` (no
  render component) and attaches its script. Verify the volume is built in
  **world space** (REFR position + rotation + scale composed once at load), so
  `trigger_detection_system` can test against the post-propagation player
  `GlobalTransform` without per-frame composition.
- **`quest_advance_system` unifies OnActivate + OnTriggerEnter**: both
  `ActivateEvent` (doors/levers, `activator` field) and `OnTriggerEnterEvent`
  (trigger volumes, `triggerer` field) converge on one `QuestAdvanceOnActivate`
  component via a combined `(entity, triggerer)` collect. The design relies on a
  given entity receiving only one signal (doors have no volume, volumes have no
  use interaction) — verify nothing can deliver both to one entity in one frame
  (double-advance). Confirm condition gating runs per `QuestAdvanceOnActivate`
  (`ConditionContext::for_subject` + `evaluate_condition_list`) and the
  `ActivatorGate::PlayerOnly` is honored. Guards: `trigger_enter_advances_quest`,
  `trigger_enter_respects_player_only_gate`,
  `activate_and_trigger_in_same_frame_both_advance`
  (`crates/scripting/src/papyrus_demo/quest_advance/tests.rs`).
- **The `M47.2 scripts:` cell-load summary**: the smoke gate
  `docs/smoke-tests/m47-triggers.sh` keys on the `N REFRs recognized, M trigger
  volumes spawned` line. Verify the counters are wired (recognized++ on a
  `translate_pex` Some, trigger_volumes++ on a volume spawn) so the smoke harness
  has a real signal — a counter that never increments makes the gate vacuous.
**Output**: `/tmp/audit/scripting/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/scripting/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_SCRIPTING_<TODAY>.md` with structure:
   - **Executive Summary** — what shipped (M30.2 `.psc` parser; M47.0 event
     hooks; M47.1 condition eval; M47.2 `.pex` reader + 5-phase decompiler +
     recognizer chain + dynamic attach path + XPRM trigger volumes) vs. deferred
     (Obscript/SCTX Phase 5; the M47.1 condition resolvers #1663–#1668; the wired
     fragment-lowerer dispatch). Findings count by severity. **Untrusted-input
     robustness verdict** (can a hostile/corrupt `.pex` or `.psc` panic, OOB, or
     OOM the cell loader — MUST be NO). **The 99.996% decompile-rate claim
     verdict** (is the corpus-smoke harness measuring what it claims). **The
     `.psc`-vs-`.pex` fidelity-gate verdict** (do `recognizes_da10_and_reproduces_
     hand_builder` AND `da10_pex_reproduces_hand_builder_byte_for_byte` (#1740)
     both actually pin byte-equality).
   - **Decompiler Soundness Matrix** — per pass (reader / cfg / lift+copy-prop /
     boolean / control-flow / lower): bounds-safe? terminates? total (no panic)?
     fidelity-tested? — with the two documented Champollion departures (no
     debug-line guard in `boolean.rs`; the deliberate `||`-skip in `control_flow.rs`)
     adjudicated as benign-or-bug.
   - **Decline-Invariant Audit** — every recognizer/composer/effect decline point
     × verified-conservative vs. leaks-a-partial-lowering.
   - **Runtime Lifecycle Invariant Matrix** — marker drain coverage; two-phase
     lock-drop per system; cascade bound; edge-trigger seed; CTDA OR-precedence.
   - **Findings** — grouped by severity (CRITICAL first), deduplicated.
   - **Future-Phase Readiness** — which invariants this audit pinned for Obscript
     (Phase 5), the fragment lowerer (b2), and the condition-resolver issues.
3. Remove cross-dimension duplicates: marker-drain coverage is owned by Dim 6
   (pointers from Dims 1–5 if they emit markers); the `translate_pex` clean-`None`
   contract is owned by Dim 5 (pointer from Dim 7); the half-extent convention is
   owned by Dim 7.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/scripting`
2. Inform user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_SCRIPTING_<TODAY>.md`
