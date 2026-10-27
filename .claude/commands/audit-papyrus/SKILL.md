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
  source of the decompile-rate claim), `pex_dump_fn.rs` (raw-bytecode disassembly of one failing function),
  `pex_corpus_shapes.rs` + `docs/r5/corpus-shape-survey.txt`,
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
are quiet between some passes (and busy in others — M47.2 churned `lower.rs` and the parser's script items);
if there are none, re-verify the guards below rather than re-reading source.

## Phase 2: Dimensions

### Dimension 1: `.pex` Reader & Opcode Decode (untrusted input — every finding is Untrusted-Input: Yes)
Paths: `crates/pex/src/{reader,opcode,model,lib,call_sites}.rs`
First step: `cargo test -p byroredux-pex`  (then re-read any guard named below that did not run)
- **`take(n)` is the single bounds gate** (`checked_add` + `<= len` → `UnexpectedEof`). No read path may
  index `self.data[..]` or `try_into().unwrap()` a short slice. Guard: `every_prefix_of_every_wire_valid_
  sample_is_rejected` (exhaustive-prefix over five builders: Skyrim LE/BE, extender-dependent BE, Starfield
  guards, and #4474's `build_sample_full_coverage` — FO4 debug-info skips, getter/setter bodies, non-empty
  `struct_infos`, `Value::Float`). A new reader branch no sample reaches is the gap to look for.
- **Allocation is bounded by something other than a wire count**: var-arg count `n` (up to `i32::MAX`) must
  not feed `Vec::with_capacity` (grow by `push`; `hostile_vararg_count_errors_instead_of_ooming`); other
  `with_capacity(count)` sites are `u16`-capped; `u32` flags/sizes are never capacities. **String copies are
  charged against `string_byte_budget` = `max(64 × file size, 16 MiB)`** in both `reader.rs` and
  `call_sites.rs` (`ScanBudget`; #4317 — a ~330 KB wire-valid file once exhausted 8 GB and aborted). Any new
  owned copy of a table string (a new model field, a new scan) must be charged; guards
  `string_copies_past_the_budget_are_refused`, `call_site_copies_are_bounded_by_the_string_budget`.
- **`OpCode::from_u8` `transmute`** (`unsafe`, memory-safety-critical): `MAX_OPCODE == 51` equals last
  discriminant + 1, enum `#[repr(u8)]` with contiguous `0..=50`, guard is `>=`; a compile-time
  `const _: () = assert!(OpCode::TryLockGuards as u8 == MAX_OPCODE - 1)` backs it (#4475). Pins:
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
  (`forward_jmpf_builds_an_if_diamond`, `forward_jmpf_with_unconditional_backedge_builds_a_loop`, and
  `backward_jmpt_builds_a_loop_edge` — a true `JmpT` pin since #4476; it was a misnamed `JmpF` test before).
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
  a fix that resumes past the block drops statements. The module doc now says "declined with an error, not
  advanced past" (#4115 `18ce09bff`); wording that drifts back is doc rot.
- **Bookkept Champollion departures** — `boolean.rs`'s two (plus a third, newer guard below). `lower.rs`'s `is` → object-typed
  `Cast` departure (#4477) is closed: opcode 36 lowers to `Expr::Is` (#5443 `24f199587`, `an_is_opcode_lowers_to_an_is_expression`).
  For the **`boolean.rs`** two — adjudicate benign-or-bug, evidence = corpus rate +
  fidelity gate, not speculation: (1) no debug-line guard (a same-named temp on a fall-through edge could be
  falsely collapsed into a fabricated `&&`/`||`); (2) termination guard (only re-process on a real merge —
  infinite loop hangs the decompiler). A declined collapse resumes `rebuild` at the operand range's END, not
  its first block (#5877 `32352592f` — every nested body was re-walked once per enclosing level, O(2^depth):
  a 656-byte `.pex` took 6.6 s; `deeply_nested_ifs_are_walked_once_not_exponentially`). `&&` true edge falls through, `||` false edge; operand must recompute
  the same condition variable; prec `&&`=7/`||`=8; `take_operand` leaves no dangling `None`.
  **Private-edge guard** (`944d0299e`): a fold deletes the operand block (and a non-exit rejoin), so it
  declines unless `incoming(operand) == 1` and, for an interior rejoin, `incoming(rejoin) == 2` — a shared
  `return` epilogue that an early-exit `jmp` also targets left a dangling edge and failed `reconstruct` (the
  last two corpus decompile failures: FO4 `StimBoxScript`, FO76 `Debug.TraceLog`; corpus now 100% on all four
  games). The in-edge counts are built once and kept exact by `link`/`unlink` at `collapse`'s three CFG
  mutations (#5879 `32352592f` — a per-fold whole-CFG scan was O(B²); `in_edge_counts_survive_and_or_and_epilogue_folds`
  compares them to a recount). Pins `or_whose_rejoin_is_a_shared_epilogue_is_left_uncollapsed`, `and_whose_rejoin_is_a_shared_epilogue_is_left_uncollapsed`. A new
  fold that deletes a block, or a new CFG mutation that skips `link`/`unlink`, is the regression.
- **Recursion**: `MAX_REBUILD_DEPTH = 1024` in `control_flow.rs` (`boolean.rs` derives it — not restated);
  both rebuilds return `RecursionLimit` (`rebuild_rejects_excessive_recursion_depth` exists in *both* files).
  It and `MAX_EXPR_DEPTH` (256) compose additively (~1,280) and independent edits erode the margin
  (#4113 still open, verified 2026-10-27) — check any change to either constant.
- **`lower.rs` totality**: `lower_expr`/`lower_stmt` never panic on any `NodeKind`; intentional lossy lowerings
  matter only if a recognizer keys on the lost info: statement-shaped node as sub-expression → `NoneLit`
  (should be unreachable), `is` → `Cast`, `lower_binary_op` default arm →
  `Eq` (a real unknown op silently becomes `==`). `StructCreate` is no longer lossy: it lowers to
  `Expr::NewStruct` (FO4 `new S`, the shape the `.psc` parser now emits too).
  **Locals**: `lower_local_decls` prepends one bare `VarDecl` per non-temp local ahead of the body
  (`0c6fe4d34` — without it every Champollion fragment prologue read as an assignment to an undeclared name and
  the #5638 fragment guard declined it; `user_locals_are_declared_ahead_of_the_body_and_temporaries_are_not`).
  "Temp" is Champollion `isTempVar` (`::temp` + a suffix, not ending `_var`, or `::nonevar`) — `lower.rs` now
  calls `node.rs`'s `is_temp_var` (#6262 `475004d25`; the private case-insensitive copy it replaced left a local the
  lift keeps as real, `::TEMP1` or a bare `::temp`, undeclared; a second predicate reappearing is the regression;
  `user_locals_are_declared_ahead_of_the_body_and_temporaries_are_not`) — and is
  NOT every `::` name: the compiler's `::mangled_<name>_<n>` user locals must be declared (#5880 `32352592f`,
  ~280 vanilla functions declined under the old `::` filter).
- **Script assembly**: FO4+ `struct_infos` lower to `ScriptItem::Struct` items first (members keep their stored
  defaults; `pex_corpus_smoke`'s expected-item-count adds `struct_infos.len()`); synthetic `::` variables
  dropped; variable and auto-property defaults lower to `initial_value` via `default_literal` (the backing
  variable's stored default); the object's `conditional`/`hidden`/`default` user
  flags (bit indices named by the file's own user-flag table, resolved ONCE per `Pex` by
  `UserFlagMasks::from_pex` — #5683 `d8b1d333c`, the old per-variable *has_user_flag* rescan was O(F×V), ~18 s on
  an 852 KB hostile file, `a_hostile_user_flag_table_decompiles_in_linear_time`; `flag_index < 32` guards the
  shift on untrusted bytes) lower into the same `ScriptFlags` the `.psc` header parses, a
  variable's `conditional` into `is_conditional` (#5398 `ce90b481c`, `object_user_flags_lower_into_script_flags`;
  struct members through the same table, `struct_member_user_flags_lower_into_variable_flags`)
  and a property's `const`/`mandatory`/`hidden`/`conditional` into `PropertyFlags` (#5685 `f7afb0e07`,
  `property_user_flags_lower_into_property_flags`) — from the property's user bits OR its auto backing
  variable's, plus the backing variable's `const_flag` → `CONST` (#5882 `958c26c4e`: vanilla tables never name
  `const` and `Auto Const`/`Auto Conditional` bits live on the backing variable, Champollion `writeProperty`;
  ~22.5k FO4 / 24.5k Starfield `Auto Const` once lowered bare). The backing variable is found via one
  per-object map, not a per-property scan (the hostile linear-time test covers 65,535 auto properties, #5878);
  **script scope = the empty-named state's functions**;
  every named state → a `State` item; exactly one (the first case-insensitive `is_auto_state` match against
  `auto_state_name`) is `is_auto` (#4473, `case_colliding_state_names_mark_exactly_one_auto`) — keying scope on the auto match inverted 983 vanilla scripts (#4319;
  `named_auto_state_stays_an_auto_state_and_the_empty_state_is_script_scope`). Event iff (`on`-prefixed AND
  `is_event_name`) OR `::remote_`-prefixed; `EVENT_NAMES` is a sorted lowercase union binary-searched
  (`list_is_sorted_for_binary_search`); a missing engine event demotes a handler to a function. The list is a
  documented **superset** of Champollion's frozen `EventNames.hpp`: 12 real engine events it omits
  (`OnAttach`, `OnStoryClearLocation`, ten Starfield) are hand-added in sorted position from a base-class
  corpus census (#4471 `0611cd116`; `the_census_engine_events_classify_as_events`, which also keeps vanilla
  typos like `oncelldetatch` out) — an addition needs the same census evidence, not a guess.
- **The decompile-rate claim measures robustness, not fidelity.** `pex_corpus_smoke` counts `Err` and
  panics as failures, tallies after `decompile_script`, and exits non-zero on any decompile `Err`, panic or shape
  mismatch, not only parse failures (#5883 `958c26c4e`) — verify that gate still holds. Its shape check must not share an
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
  `\` continuations are a logos skip rule in `lexer.rs` (#5881 `af4a68ad7` — no pre-pass; the `;` comment callback owns a trailing `\`); an operator ending a line still continues. Verify
  every other `peek()` on a statement boundary: #4472 (six sites, via `check_raw`) and #4763 (seven more —
  `Extends`, variable/function/group flags, property and local initializers) moved construct-continuation
  decisions to the raw stream, each pinned by an `*_on_the_next_line_*` test in `script.rs` plus
  `single_line_forms_of_the_4763_sites_still_parse`. Remaining `peek()`/`check()` sites should sit behind an
  explicit `skip_newlines()` or a bracket-bounded list. `expect()` / `expect_ident()` still skip newlines, so
  a keyword's **mandatory operand** goes through `expect_same_line` / `expect_raw` / `expect_ident_raw`
  (`parser/mod.rs`; #5021, `3b7ffa670`): declaration names, the `Extends` target, the `(` after a function
  name, `State` after `Auto`, the If/ElseIf/While condition start, and `New`'s type and `[` — #5331 `fd5aa4ead`,
  `parse_new_expr`; the `[` is optional only for an `Object` type, where a same-line `new S` is FO4's
  `Expr::NewStruct`, and every segment after a qualified name's `:` (`parse_qualified_ident` → `expect_ident_raw`,
  #5885 `3081d23f2`: ScriptName/Extends/Import/event sender/types; `a_namespaced_parent_on_the_next_line_is_still_not_the_parent`) — pinned by
  `a_mandatory_operand_on_the_next_line_is_an_error`, `auto_on_its_own_line_does_not_make_an_auto_state`,
  `extends_with_its_target_on_the_next_line_sets_no_parent`, `single_line_forms_of_the_5021_sites_still_parse`.
  Only closing keywords may keep the newline-skipping `expect`; a new declaration keyword that uses it is the
  regression.
- **Statement termination** (#5322 `a5c3f2428`): `expect_eol` (`parser/mod.rs`) used to accept ANY token after a
  statement, so a tail the Pratt loop stopped at re-parsed as a new statement (`If f is Actor` became a
  truthiness test plus a fabricated `VarDecl`) with zero errors. It now records a recovered `UnexpectedToken` for
  anything but Newline/EOF/DocComment and skips to line end via `skip_to_line_end`, which leaves block terminators
  (`EndIf`, `Else*`, `End*`) unconsumed so a glued `x = 1 EndIf` still closes its block
  (`glued_statement_tail_is_an_error_and_not_a_statement`; `EndStruct` / `EndGroup` stay too, #5444 `3efa1c024`,
  `glued_struct_and_group_terminators_stay_for_their_container`); `parse_expr` (debug console, offline preflight) ends in
  `require_input_end` (`parse_expr_rejects_trailing_tokens`). FO4 `is` is a keyword with an `Expr::Is` type-test
  node at cast precedence (`test_is_type_test`); downstream exhaustive matches treat it as transparent to the operand
  (the `.pex` side lowers `OpCode::Is` to the same `Expr::Is`, #5443; the VM evaluates it as the operand's truthiness).
- **Doc comments and FO4 names** (`e96d687ec`): Papyrus puts an item's `{…}` doc on the line AFTER its header, so
  `take_trailing_doc` (`parser/mod.rs`) consumes one right after each header's `expect_eol` (script header,
  property, group, struct + member, function/event; #5325 — a bodied function's doc once became its first
  "statement" and the function was dropped). `expect_eol` accepts a `{doc}` terminator only when it ends the
  line (#5886 `3081d23f2`, `a_same_line_doc_comment_cannot_share_its_line_with_the_next_item`). There is no
  leading-doc fallback: a top-level `{lead}` is discarded, and inside State/Group/accessor containers it is a
  parse error (#5890 `d148cd27b`, `a_leading_doc_comment_attaches_to_nothing`). Script name, `Extends` target and
  `Import` are qualified idents (`DLC01:Foo`); a remote event `Event Sender.Event(…)` (#5328) is normalised to
  the compiler's `Sender_Event` spelling — what the `.pex` frontend yields and the VM's remote matcher binds,
  while a consumer matching the script's own `OnDeath` does not (#5887 `3081d23f2`,
  `remote_event_handlers_parse_with_their_sender_in_the_name`). `Parent` is a legal name only as a struct-member
  declaration or after `.` (#5888, `parent_is_a_legal_member_name`; the F4SE corpus test's `KNOWN_GAPS` is now
  empty) — via `expect_member_name`, NOT the shared `keyword_as_ident` table: #5888 first put it there and a
  local/parameter named `Parent` then declared fine and parsed every use as `ParentAccess` (#6263 `a87650237`,
  `parent_is_not_a_local_or_parameter_name`; `parse_variable_body(struct_member)` carries the distinction). Pins
  `a_doc_comment_after_a_bodied_function_header_documents_it`, `namespaced_script_name_parent_and_import_keep_every_segment`.
- **Precedence/associativity**: `BinaryOp::precedence` Or=1, And=2, comparisons=3, Add/Sub/StrCat=4,
  Mul/Div/Mod=5, unary=6, cast=7/postfix=8 (`PREC_*` in `expr.rs`); left-assoc hinges on `op_prec <= min_bp →
  break`. (Bethesda's inverted CTDA OR/AND precedence is a *condition-evaluation* concern in
  `/audit-scripting` Dim 3 — `.psc` operators are standard.)
- **Line continuation** (#5881): a `\` followed by blanks / a `;` comment / `\n`, `\r\n` or lone `\r` is one skip rule, so
  spans index the caller's own text with no offset map (#5884 `af4a68ad7` — *preprocess*/*OffsetMap* are gone;
  `spans_after_a_continuation_are_original_offsets`, `ast_spans_stay_in_original_coordinates_after_a_continuation`);
  a trailing `\` at EOF is emitted, not swallowed. Unterminated `;/` block comments, `{` docs and string literals
  are `LexError`s, not a silent swallow to EOF (#5889 `af4a68ad7`, `unterminated_block_comment_string_and_doc_are_lex_errors`). Keywords are
  `ignore(case)` (logos 0.16 dropped *ascii_case*, `15a6b1d2d` — Unicode folding, which differs only for the few
  non-ASCII letters that fold onto ASCII, e.g. U+017F, U+212A, invalid Papyrus either way) and win over the
  `Ident` regex. CK flags matched **contextually** stay out of the lexer: `Default` (script header) and
  `Collapsed` (group) are identifiers checked in their flag position (#5398 `ce90b481c`,
  `default_and_collapsed_remain_identifiers`) — expressions accept only `Token::Ident`, so promoting either to a
  keyword breaks every variable of that name. Lone CR is a line terminator and CRLF one `Newline`
  (`bare_cr_is_a_newline_and_crlf_is_one_newline`, #4479 — a CR-only file once lexed to zero newlines); a `;`
  line comment likewise stops at the first `\r` *or* `\n` (`line_comment_ends_at_a_bare_cr`, #5019 `a75c7dc33` —
  it once searched only `\n`, swallowing a CR-only file to EOF with zero errors);
  `0x` and exponent shapes route through `MalformedNumber` to a `LexError`, not an adjacent token pair.
- **Recovery**: `parse_script` returns `Ok((Script, Vec<ParseError>))` for partial success and `Err` only for
  fatal; `skip_to_next_line` always consumes ≥1 token (no infinite loop); callers needing strict-fail check
  `result.1.is_empty()`. No `unwrap()` on `str::parse` of a malformed-but-lexable literal.
**Output**: `/tmp/audit/papyrus/dim_4.md`

## Phase 3: Merge

Combine `/tmp/audit/papyrus/dim_*.md` into `docs/audits/AUDIT_PAPYRUS_<TODAY>.md`: **Executive Summary**
(findings by severity; **untrusted-input robustness verdict** — can a hostile `.pex`/`.psc` panic, OOB, OOM,
or stack-overflow the cell loader / console — must be NO; **decompile-rate verdict** — re-measured, and what
it does and does not prove) · **Decompiler Soundness Matrix** (reader / cfg / lift+copy-prop / boolean /
control-flow / lower / `.psc` parser × bounds-safe, terminates, total, fidelity-tested; the three Champollion
departures adjudicated) · **Findings** (severity order, deduplicated). Cross-audit: recognizer/runtime →
`/audit-scripting`; `.pex` bytes reaching the extender preflight → `/audit-scripting` Dim 7.

## Phase 4: Cleanup

`rm -rf /tmp/audit/papyrus`; tell the user the report is ready; suggest
`/audit-publish docs/audits/AUDIT_PAPYRUS_<TODAY>.md` (domain label `scripting`).
