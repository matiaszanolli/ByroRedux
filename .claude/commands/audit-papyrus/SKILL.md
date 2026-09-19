---
description: "Deep audit of the Papyrus compiler frontends — .pex reader + 5-phase decompiler (Champollion port, crates/pex) and the .psc lexer/Pratt parser (crates/papyrus): untrusted-input bounds, decompiler soundness, depth caps"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Papyrus Frontend Audit (`crates/pex` + `crates/papyrus`)

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for shared protocol.

Audit the two frontends that turn Papyrus bytes into the shared `byroredux_papyrus::ast::Script`:
untrusted `.pex` bytecode → decode (`crates/pex`) → 5-phase decompiler, and `.psc` source → lexer + Pratt
parser (`crates/papyrus`). Both feed the recognizer chain in `/audit-scripting` (which owns
`translate_pex`'s clean-`None` contract and the panic net); this skill owns everything up to the AST.
**Highest bug-density area of the scripting domain** (untrusted input + five tree-rewriting passes).
Findings here are *silent wrong-AST* (a recognizer then matches it → wrong ECS behavior on vanilla
content) or *process aborts* (OOM / stack overflow — `catch_unwind` cannot recover either, so a panic net
is no substitute for a bound).

**Architecture**: Orchestrator; each dimension is a Task agent (max 3 concurrent).

## Scope

- `crates/pex/src/`: `reader.rs`, `opcode.rs`, `model.rs`, `lib.rs` (`parse`, `PexError`,
  `string_byte_budget`), `call_sites.rs` (`Pex::call_sites`, the extender-preflight scan — consumed by
  `/audit-scripting` Dim 7), `decompile/{mod,cfg,lift,boolean,control_flow,lower,node,event_names}.rs`.
- `crates/papyrus/src/`: `token.rs`, `lexer.rs`, `ast.rs`, `span.rs`, `error.rs`, `lib.rs`,
  `parser/{mod,expr,stmt,script}.rs`.
- Instruments: `crates/pex/examples/pex_corpus_smoke.rs` (decompiles every `.pex` in real archives; the
  source of the decompile-rate claim), `pex_corpus_shapes.rs` + `docs/r5/corpus-shape-survey.txt`,
  `crates/bsa/examples/r5_extract_pex_ba2.rs` (corpus extractor).
- Ground truth: crate docstrings (`crates/pex/src/lib.rs`, `decompile/mod.rs`), `docs/engine/{papyrus-parser,
  m47-2-design}.md` ("no opcode semantics guessed"). Reference decompiler: Champollion (`OPCODES`,
  `PscCoder::writeStates`); UESP Papyrus Assembly spec for operand order.
- Not here: hkx (`/audit-parsers`), ESM VMAD decode (`/audit-esm`), recognizers/runtime (`/audit-scripting`).

## Parameters / Extra Fields / Severity

`--focus` default all 4 · `--depth shallow` (bounds + depth-cap contracts) | `deep` (trace each pass's tree
rewrite). Finding fields: **Dimension**: PEX Reader | Decompiler CFG & Lift | Decompiler Boolean/Control-Flow/
Lower | Papyrus Lexer & Parser · **Untrusted-Input**: Yes | No (Yes for any path consuming raw `.pex`/`.psc`).
Escalations over `_audit-severity.md`: panic / OOB / unbounded alloc or recursion reachable from untrusted
bytes → **HIGH** (CRITICAL if memory-unsafe — the `transmute` in `opcode.rs`); decompiler emits a *wrong* AST
that a recognizer accepts, or copy-propagation / boolean-collapse folds into the wrong consumer → **HIGH**
(silent, all-game blast radius); doc rot → LOW.

## Phase 1: Setup

`mkdir -p /tmp/audit/papyrus`; dedup `gh issue list --repo matiaszanolli/ByroRedux --limit 300 --json number,title,state,labels > /tmp/audit/papyrus/issues.json`;
read the newest `docs/audits/AUDIT_SCRIPTING_*.md` (Dims 1-4 findings; pre-split reports use the old
8-dimension numbering) and `docs/engine/m47-2-design.md` §"Frontends in detail"/"Risks & mitigations" to
separate designed declines from defects. Run `cargo test -p byroredux-pex -p byroredux-papyrus`.
Delta-first: `git log --since=<last report> --format='%h %cs %s' -- crates/pex crates/papyrus` — these crates
often have **zero** commits between passes; then re-verify the guards below rather than re-reading source.

## Phase 2: Dimensions

### Dimension 1: `.pex` Reader & Opcode Decode (untrusted input — every finding is Untrusted-Input: Yes)
Paths: `crates/pex/src/{reader,opcode,model,lib,call_sites}.rs`
First step: `cargo test -p byroredux-pex`  (then re-read any guard named below that did not run)
- **`take(n)` is the single bounds gate** (`checked_add` + `<= len` → `UnexpectedEof`). No read path may
  index `self.data[..]` or `try_into().unwrap()` a short slice. Guard: `every_prefix_of_every_wire_valid_
  sample_is_rejected` (exhaustive-prefix; replaces hand re-verification) — confirm it covers all three
  dialects (`parses_a_handbuilt_{fo4,skyrim_be,starfield_pex_with_guards}`).
- **Allocation is bounded by something other than a wire count**: var-arg count `n` (up to `i32::MAX`) must
  not feed `Vec::with_capacity` (grow by `push`; `hostile_vararg_count_errors_instead_of_ooming`); other
  `with_capacity(count)` sites are `u16`-capped; `u32` flags/sizes are never capacities. **String copies are
  charged against `string_byte_budget` = `max(64 × file size, 16 MiB)`** in both `reader.rs` and
  `call_sites.rs` (`ScanBudget`; #4317 — a ~330 KB wire-valid file once exhausted 8 GB and aborted). Any new
  owned copy of a table string (a new model field, a new scan) must be charged; guards
  `string_copies_past_the_budget_are_refused`, `call_site_copies_are_bounded_by_the_string_budget`.
- **`OpCode::from_u8` `transmute`** (`unsafe`, memory-safety-critical): `MAX_OPCODE == 51` equals last
  discriminant + 1, enum `#[repr(u8)]` with contiguous `0..=50`, guard is `>=`. Pins:
  `discriminants_match_on_disk_order`, `from_u8_round_trips_and_rejects_oob`,
  `metadata_matches_champollion{,_full_table}` (verify they cover every discriminant, not spot values).
- **`arg_count` table** drives operand consumption: a wrong row desyncs the whole stream silently. Spot-check
  var-arg opcodes (`callmethod`/`callparent`/`callstatic`/guard ops) and high-arity rows
  (`array_findstruct` 5, `array_getallmatchingstructs` 6) against Champollion/UESP.
- `string_index` → `.get(idx)` / `BadStringIndex` (never `self.strings[idx]`); `value()` accepts tags 0..=5,
  `Integer` sign-reinterprets; endianness from magic (`0xFA57C0DE` LE / `0xDEC057FA` BE) with the provisional
  LE in `new()` never leaking into a multibyte read; Skyrim-vs-FO4+ field gating (`is_skyrim()`: `const_flag`,
  `struct_infos`, property-group/struct-order tables; Starfield-only `guards`) reads in FileReader order and
  the consume-and-discard skips match the writer's byte counts.
- `read_binary` is all-or-`Err` (no partial `Pex`); `call_sites` stops with a diagnostic when its budget is
  exhausted rather than truncating silently.
**Output**: `/tmp/audit/papyrus/dim_1.md`

### Dimension 2: Decompiler — CFG & Opcode→Node Lift (copy-propagation is the AST-correctness core)
Paths: `crates/pex/src/decompile/{cfg,lift,node}.rs`
First step: `cargo test -p byroredux-pex -- decompile::cfg decompile::lift`
- **Jump targets**: `checked_target` accepts `0 <= ip+offset <= count` (inclusive — `count` is the exit
  anchor); non-integer offset → `BadJumpOffset`, OOB → `JumpOutOfRange`, non-{ident,bool,int} condition →
  `BadJumpCondition`. `CodeBlock::split(at)` is never called with `at == 0` (underflow).
- **`jmpf`/`jmpt` polarity**: `jmpf` jumps when FALSE (true-edge = fall-through); a flip inverts every `If`
  (`forward_jmpf_builds_an_if_diamond`, `backward_jmpt_builds_a_loop_edge`).
- **`rebuild_expression`**: a temp-producing node folds into the *single immediately-next live* consumer
  (`count_constant_id`: 0 → advance, 1 → inline, >1 → `ExpressionRebuildFailed`); folding into a non-adjacent
  consumer reorders side effects. It runs over a linked live-index chain, not restart-at-0/`Vec::remove`
  (O(n²), #2024 — `rebuild_expression_is_linear_up_to_the_wire_format_ceiling`). `is_final` vs `is_temp_var`
  asymmetry (`::temp` prefix, `_var` suffix) is a deliberate Champollion port — unifying them is a regression.
  `replace_constant_id`'s `debug_assert!(slot.is_none())` is debug-only: a release >1-match would silently
  drop the producer.
- **Depth is memoized on `Node`** (`Node::depth`, `recompute_depth`) and checked at every fold against
  `MAX_EXPR_DEPTH` (= the papyrus parser constant), so repeated re-folds by the boolean/control-flow passes
  cannot reset the ledger (the SIGABRT class); guards `a_temp_chain_past_the_depth_cap_is_declined_not_aborted`
  / `..._inside_the_depth_cap_still_folds`. Any new in-place tree mutation must call `recompute_depth`.
- `create_node`: `Cast` downgrades to `Copy` only when source is `None`, same-typed identifiers, or
  `::nonevar` (case-insensitive) — a wrong downgrade turns a narrowing cast into an identity copy. Operand
  order for `CallMethod`/`CallStatic`/`CallParent` (result/object/method indices) is what recognizers key
  on (`SetStage`, `GetStageDone`); cross-check UESP. `id_of` on a literal → `ExpectedIdentifier`, never
  `unwrap()` outside a checked branch. Bodyless/native functions decompile to an empty body.
**Output**: `/tmp/audit/papyrus/dim_2.md`

### Dimension 3: Decompiler — Boolean, Control-Flow, Lower & the Corpus Instruments
Paths: `crates/pex/src/decompile/{boolean,control_flow,lower,event_names,mod}.rs`, `crates/pex/examples/pex_corpus_smoke.rs`
First step: `cargo test -p byroredux-pex -- decompile::`; with game data: `cargo run --release -p byroredux-pex --example pex_corpus_smoke -- <Skyrim - Misc.bsa> <Fallout4 - Misc.ba2>`
- **Pass order is load-bearing**: `lower::decompile_body` runs cfg → lift → `rebuild_boolean_operators` →
  `reconstruct` → `lower_body`; the boolean pass precedes control-flow so `&&`/`||` chains collapse first.
- **`control_flow.rs`**: shape classification (While: `last.next == current`; If: `last.next == exit`; If/Else;
  jmpt inversion negates + swaps edges). A conditional-predecessor `last` (`||` shape the boolean pass should
  have collapsed) **fails closed** (`ControlFlowFailed`, #1732; `conditional_predecessor_fails_closed`) —
  a fix that resumes past the block drops statements. The module doc's "advanced past" wording is stale
  (#4115 open).
- **`boolean.rs`** two deliberate Champollion departures — adjudicate benign-or-bug, evidence = corpus rate +
  fidelity gate, not speculation: (1) no debug-line guard (a same-named temp on a fall-through edge could be
  falsely collapsed into a fabricated `&&`/`||`); (2) termination guard (only re-process on a real merge —
  infinite loop hangs the decompiler). `&&` true edge falls through, `||` false edge; operand must recompute
  the same condition variable; prec `&&`=7/`||`=8; `take_operand` leaves no dangling `None`.
- **Recursion**: `MAX_REBUILD_DEPTH = 1024` in `control_flow.rs` (`boolean.rs` derives it — not restated);
  both rebuilds return `RecursionLimit` (`rebuild_rejects_excessive_recursion_depth` exists in *both* files).
  It and `MAX_EXPR_DEPTH` (256) compose additively (~1,280) and independent edits erode the margin
  (#4113 open) — check any change to either constant.
- **`lower.rs` totality**: `lower_expr`/`lower_stmt` never panic on any `NodeKind`; intentional lossy lowerings
  matter only if a recognizer keys on the lost info: statement-shaped node as sub-expression → `NoneLit`
  (should be unreachable), `is` → `Cast`, `StructCreate` → `New` size 0, `lower_binary_op` default arm →
  `Eq` (a real unknown op silently becomes `==`).
- **Script assembly**: synthetic `::` variables dropped; **script scope = the empty-named state's functions**;
  every named state → a `State` item, `is_auto` when it matches `auto_state_name` case-insensitively
  (`is_auto_state`) — keying scope on the auto match inverted 983 vanilla scripts (#4319;
  `named_auto_state_stays_an_auto_state_and_the_empty_state_is_script_scope`). Event iff (`on`-prefixed AND
  `is_event_name`) OR `::remote_`-prefixed; `EVENT_NAMES` is a sorted lowercase union binary-searched
  (`list_is_sorted_for_binary_search`); a missing engine event demotes a handler to a function.
- **The decompile-rate claim measures robustness, not fidelity.** Verify `pex_corpus_smoke` counts `Err` and
  panics as failures and tallies after `decompile_script`. Its shape check must not share an
  implementation rule with the code under test — the #3017 check and `decompile_script` shared the wrong
  auto-state rule and both agreed on 983 mis-assembled scripts. Fidelity evidence is the `.psc`-vs-`.pex`
  gate (owned by `/audit-scripting` Dim 1: `recognizes_da10_and_reproduces_hand_builder` +
  `da10_pex_reproduces_hand_builder_byte_for_byte`), not the corpus rate.
**Output**: `/tmp/audit/papyrus/dim_3.md`

### Dimension 4: `.psc` Lexer & Pratt Parser (untrusted: `.psc` files and console/debug text)
Paths: `crates/papyrus/src/{token,lexer,ast,error,lib}.rs`, `crates/papyrus/src/parser/{mod,expr,stmt,script}.rs`
First step: `cargo test -p byroredux-papyrus -- depth chain the_two_parser_depth_caps_stay_equal`
- **Depth caps** (stack overflow *and* AST `Drop` recursion): `MAX_EXPR_DEPTH = 256` (`expr.rs`) and
  `MAX_STMT_DEPTH = 256` (`stmt.rs`), both `pub`, equality pinned by `the_two_parser_depth_caps_stay_equal`;
  downstream `MAX_CONDITIONAL_DEPTH` and `lift.rs`'s cap alias them. The Pratt loop **charges `expr_depth`
  for each node a postfix/binary chain wraps** (#4320 — the cap once counted only recursive calls, so
  `a.a.a…` of ~0.4 MB overflowed on Drop) and restores on **every** exit including error paths
  (`long_postfix_and_binary_chains_hit_the_depth_cap`, `realistic_chains_still_parse_and_release_their_depth`,
  `depth_resets_between_top_level_calls`). All recursive expression entry funnels through `parse_expr_bp`.
- **Newline is a terminator in the Pratt loop**: the loop uses `peek_raw()`, not the newline-skipping
  `peek()` (#4321 — a line opening with `(` was glued to the previous statement, a silent wrong AST). Explicit
  `\` continuations are joined by `lexer::preprocess`; an operator ending a line still continues. Verify
  every other `peek()` on a statement boundary (`stmt.rs`, `script.rs` item recovery use `peek_raw`).
- **Precedence/associativity**: `BinaryOp::precedence` Or=1, And=2, comparisons=3, Add/Sub/StrCat=4,
  Mul/Div/Mod=5, unary=6, cast=7/postfix=8 (`PREC_*` in `expr.rs`); left-assoc hinges on `op_prec <= min_bp →
  break`. (Bethesda's inverted CTDA OR/AND precedence is a *condition-evaluation* concern in
  `/audit-scripting` Dim 3 — `.psc` operators are standard.)
- **`preprocess`**: `\`+`\n`/`\r\n`/lone `\r` elided (2/3/2 bytes) with exact `OffsetMap` counts (a wrong count
  drifts every later span); a trailing `\` at EOF is emitted, not swallowed. Keywords are
  `ignore(ascii_case)` and win over the `Ident` regex.
- **Recovery**: `parse_script` returns `Ok((Script, Vec<ParseError>))` for partial success and `Err` only for
  fatal; `skip_to_next_line` always consumes ≥1 token (no infinite loop); callers needing strict-fail check
  `result.1.is_empty()`. No `unwrap()` on `str::parse` of a malformed-but-lexable literal.
**Output**: `/tmp/audit/papyrus/dim_4.md`

## Phase 3: Merge

Combine `/tmp/audit/papyrus/dim_*.md` into `docs/audits/AUDIT_PAPYRUS_<TODAY>.md`: **Executive Summary**
(findings by severity; **untrusted-input robustness verdict** — can a hostile `.pex`/`.psc` panic, OOB, OOM,
or stack-overflow the cell loader / console — must be NO; **decompile-rate verdict** — re-measured, and what
it does and does not prove) · **Decompiler Soundness Matrix** (reader / cfg / lift+copy-prop / boolean /
control-flow / lower / `.psc` parser × bounds-safe, terminates, total, fidelity-tested; the two Champollion
departures adjudicated) · **Findings** (severity order, deduplicated). Cross-audit: recognizer/runtime →
`/audit-scripting`; `.pex` bytes reaching the extender preflight → `/audit-scripting` Dim 7.

## Phase 4: Cleanup

`rm -rf /tmp/audit/papyrus`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_PAPYRUS_<TODAY>.md` (domain label `scripting`).
