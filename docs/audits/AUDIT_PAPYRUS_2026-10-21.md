**HEAD**: 4a3e89d56 · **Baseline**: [AUDIT_PAPYRUS_2026-10-15.md](AUDIT_PAPYRUS_2026-10-15.md) (HEAD `5f7ff5920`) · **Audited**: Dim 3 (Boolean/Control-Flow/Lower and the corpus instruments: `32352592f`, `958c26c4e`, `24f199587`), Dim 4 (`.psc` Lexer & Pratt Parser: `af4a68ad7`, `3081d23f2`, `d148cd27b`) · **Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode), Dim 2 (Decompiler CFG & Lift)

# Papyrus Frontends Audit: 2026-10-21

## Delta since the baseline: 6 commits

`git log 5f7ff5920..HEAD -- crates/pex crates/papyrus`. All six fix findings from the 2026-10-15 report.

| Commit | Change | Dim | Verdict |
|---|---|---|---|
| `32352592f` (10-16) | #5877: after a declined collapse, the boolean pass resumes at the end of the operand range. #5879: in-edge counts are kept up to date with `link`/`unlink`. #5880: `lower_local_decls` uses an `isTempVar` predicate | 3 | #5877 and #5879 are **SOUND**: the A/B run gives identical ASTs on all 33,874 corpus files, and the hostile shapes now run in linear time. #5880 is correct on the corpus, but it adds a second, diverging copy of the predicate (D3-01) |
| `958c26c4e` (10-16) | #5882: `Auto Const` / `Auto Conditional` are read from the backing variable, and struct members get their user flags. #5883: the smoke gate exits non-zero on any decompile regression | 3 | SOUND. It matches Champollion `PscCoder::writeProperty`, and the gate holds |
| `24f199587` (10-19) | #5443: opcode 36 now lowers to `Expr::Is` instead of `Expr::Cast` | 3 | SOUND. The VM evaluates `Is` as the operand's truthiness, which is what the old object-typed `Cast` gave, so behaviour is unchanged |
| `af4a68ad7` (10-16) | #5881, #5884, #5889: line continuation is now a logos skip rule, `preprocess`/`OffsetMap` are removed, and unterminated `;/`, `{` and `"` are `LexError`s | 4 | SOUND. 3M fuzz inputs found no panic and no span outside the source or off a char boundary |
| `3081d23f2` (10-16) | #5885: segments after `:` are raw. #5886: a `{doc}` must end its line. #5887: remote events are named `Sender_Event`. #5888: `Parent` is accepted as a name | 4 | #5885–#5887 are SOUND. #5888 goes too far: `Parent` is accepted in every declaration position (D4-01). One doc table row was left stale (D4-02) |
| `d148cd27b` (10-16) | #5890: docs and comments | 4 | Mostly done. The stale rows are listed in D4-02 |

No commits touched `reader.rs`, `opcode.rs`, `model.rs`, `lib.rs`, `call_sites.rs`, or `decompile/{cfg,lift,node,control_flow,event_names,mod}.rs`.

## Build, test and corpus state: CLEAN

```
cargo test -p byroredux-pex -p byroredux-papyrus
  papyrus 145 unit + 4 round-trip · pex 86 unit + 1 doc-test (r5_fidelity: 1 ignored, needs data) · 0 failed
pex_corpus_smoke (release): Skyrim - Misc.bsa · Fallout4 - Misc.ba2 · Starfield - Misc.ba2 · SeventySix - MiscClient.ba2
  parse 33,874 / 33,874 · decompile → AST: ok 33,874 (100.0%) · err 0 · panic 0 · shape mismatch 0 · exit 0
```

This is the first run that includes FO76 (7,233 files). The baseline's 26,641 covered Skyrim SE, FO4 and Starfield only. Two opcodes never appear in the corpus: `nop` and `array_rfindelement`.

**Dim 1/2 guard spot-check** (no delta). Every named guard is present and passing:
- `every_prefix_of_every_wire_valid_sample_is_rejected`
- `hostile_vararg_count_errors_instead_of_ooming`
- `string_copies_past_the_budget_are_refused`
- `call_site_copies_are_bounded_by_the_string_budget`
- `discriminants_match_on_disk_order`
- `from_u8_round_trips_and_rejects_oob`
- `metadata_matches_champollion{,_full_table}`
- `backward_jmpt_builds_a_loop_edge`
- `rebuild_expression_is_linear_up_to_the_wire_format_ceiling`
- `a_temp_chain_past_the_depth_cap_is_declined_not_aborted`

`OpCode::from_u8` still checks `>= MAX_OPCODE` before the `transmute`, keeps its SAFETY comment, and is backed by `const _: () = assert!(OpCode::TryLockGuards as u8 == MAX_OPCODE - 1)`.

**Probes**: one throwaway crate at `/mnt/data/tmp/suite-1021-papyrus/`, built with its own `target/`. It holds `git archive` copies of `crates/{pex,papyrus,bsa}` at HEAD and a `pex_old` crate, which is HEAD with only `decompile/boolean.rs` set back to `32352592f^`. It uses the repo's `Cargo.lock`. The binaries:
- `probe`: old-vs-new decompile A/B over the four archives.
- `census`: the temp-variable predicate disagreement census.
- `remote`: a census of `::remote_` handler names.
- `hostile`: timing for nested-if and `||`-chain shapes.
- `fuzz`: lexer and parser fuzzing, with span invariants checked.
- `cases` / `c5445`: targeted parser probes.

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 3 | 3 (#4113, #4115, #5445) |

All 14 findings from the 2026-10-15 report are fixed: #5877–#5890, plus #5443. That includes the baseline's HIGH exponential boolean pass and both of its quadratic holes.

### Untrusted-input robustness verdict: MET, for crash classes and now for CPU time

- **Crash classes.** I found no panic, out-of-bounds read, OOM or stack overflow from a hostile `.pex` or `.psc`.
  - 3,000,000 `.psc` fuzz inputs (two seeds) gave 0 panics and 0 bad spans. Each run took at most 922 µs, through `lex`, `parse_script` and `parse_expr`.
  - "Bad span" means a token or error span that is reversed, past EOF, or not on a char boundary. That invariant is new: since #5884 removed the offset map, spans index the caller's text directly.
  - The `.pex` reader has no delta, and its exhaustive-prefix guard passes.
- **CPU time.** The baseline's three holes are closed.
  - Crafted nesting no longer costs exponential time: 200 nested `if`s decompile in 1.5 ms, and 1,000 nested decline in 3.9 ms with `ExpressionTooDeep`.
  - A 30,000-link short-circuit chain declines in 22 ms with `RecursionLimit`.
  - The pins `deeply_nested_ifs_are_walked_once_not_exponentially`, `in_edge_counts_survive_and_or_and_epilogue_folds` and `a_hostile_user_flag_table_decompiles_in_linear_time` (now with 65,535 auto properties) pass.
  - The slowest vanilla decompile is 19 ms (`companionaffinityeventquestscript.pex`).

### Decompile-rate verdict: re-measured at 33,874 / 33,874. It proves robustness, not fidelity

- **The gate holds.** `pex_corpus_smoke` now exits non-zero on any decompile `Err`, panic or shape mismatch (#5883). I checked the code at `pex_corpus_smoke.rs:299-311`, and this run exited 0.
- **The shape check is still independent.** It restates the item-count rule and calls nothing in `lower.rs`.
- **What #5877 adds.** Because #5877 changed the boolean pass's walk order, I ran a two-implementation A/B, which is stronger evidence than the rate. The pre-fix and post-fix boolean passes produce byte-identical `Debug` ASTs on all 33,874 files. So the O(2^n) → O(n) change did not alter any vanilla decompile.
- **What the rate does not prove.** Fidelity is still the `.psc`-vs-`.pex` gate owned by `/audit-scripting` Dim 1. The rate says nothing about operator typing, for example: `IAdd`/`FAdd` and `IDiv`/`FDiv` lift to the same operator string. That is the root of SCR-2026-10-21-02, already reported by the scripting audit and not re-reported here.

## Decompiler Soundness Matrix

| Stage | Bounds-safe | Terminates (and how fast) | Total (no panic) | Fidelity-tested |
|---|---|---|---|---|
| Reader (`reader.rs`, `opcode.rs`, `call_sites.rs`) | YES: `take(n)` is the single gate; string budget; `u16`-capped capacities; guarded `transmute` | YES, linear | YES | Exhaustive-prefix test over five builders |
| CFG (`cfg.rs`) | YES: `checked_target`; no `split(0)` | YES | YES | `jmpf`/`jmpt` pins |
| Lift + copy-propagation (`lift.rs`, `node.rs`) | YES: memoised depth checked against `MAX_EXPR_DEPTH` | YES, linear (#2024 pin) | YES | Depth-cap pins; corpus |
| Boolean (`boolean.rs`) | YES: `MAX_REBUILD_DEPTH` | YES, **now linear** (#5877 resume-at-range-end; #5879 O(1) edge guard) | YES | Private-edge pins; in-edge recount test; **33,874-file old/new A/B identical** |
| Control flow (`control_flow.rs`) | YES: `RecursionLimit` | YES, linear | YES; the leftover `||` shape fails closed (#1732) | `conditional_predecessor_fails_closed` |
| Lower (`lower.rs`) | YES: `flag_index < 32`; backing-variable map | YES, linear (#5683 and #5878 pins) | YES: every `NodeKind` arm returns a value | `Is`, `Auto Const`, struct-member flags and local-declaration pins; D3-01 is the one divergence |
| `.psc` lexer and parser | YES: two 256 depth caps; no `unwrap` on literals; unterminated input is a `LexError` | YES: `skip_to_next_line` consumes ≥1 token; fuzz worst 922 µs | YES: 3M fuzz inputs | Newline-terminator pins; corpus gate (`FO4_PSC_DIR`, `KNOWN_GAPS` empty) |

**Champollion departures, adjudicated:**
1. **`boolean.rs`, no debug-line guard.** Benign. The edge shape (fall-through to the rejoin, plus the private-edge counts) does the work the line check would. The corpus has 0 errors, and the A/B shows the #5877 rewalk change is invisible on vanilla content.
2. **`boolean.rs`, termination guard.** Benign. A block is re-processed only after a real merge, and the new resume target is strictly forward (`r > current && r <= end`).
3. **`is` lowered to `Cast` (#4477).** CLOSED by #5443.

A new, *unbookkept* departure is D3-01 below: the second `isTempVar`.

## Findings

### LOW

#### PEX-D3-2026-10-21-01: `lower.rs` has its own copy of `isTempVar` that disagrees with `node.rs` and with Champollion, and its doc comment says the opposite
- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes (`.pex` local names), but nothing in vanilla content reaches it
- **Location**: `crates/pex/src/decompile/lower.rs:296-302` (the private `is_temp_var`, added by `32352592f` for #5880). Compare `crates/pex/src/decompile/node.rs:473-476`, the `pub(crate) is_temp_var` that `lift.rs:327` uses.
- **Status**: NEW (introduced by `32352592f`)
- **Description**: The same Champollion predicate now exists twice, with different rules:

  | Copy | Rule |
  |---|---|
  | `node.rs` (used by the lift) | `len > 6 && starts_with("::temp")` (case-sensitive) `&& !ends_with("_var")`, or `::nonevar` (caseless). This matches Champollion `PscDecompiler.cpp:20-24` exactly |
  | `lower.rs` (used for local declarations) | Lowercases the name first and drops the length check. Its doc says this is "Champollion's `isTempVar` … case-insensitive, as the rest of the format is", but Champollion's `::temp` test is case-sensitive and requires length > 6 |

  Champollion uses its one predicate both to decide what to declare (`:1357`) and what to emit as an assignment (`:1752`), so in Champollion the two always agree. Here they can split. A local named `::Temp1` or exactly `::temp` is treated as a **real variable** by the lift, which keeps its `::Temp1 = …` assignments. But `lower.rs` treats it as a **temporary** and does not declare it. The body then assigns and reads an undeclared name. That is the same failure #5880 just fixed for `::mangled_*`: the #5638 fragment guard declines the function, or a write-only local gets bound as a script variable.
- **Evidence**: The `census` probe ran both predicates over every function local in all 33,874 corpus files. They disagreed on **0** names. The corpus `::` locals are lowercase `::tempN`, `::NoneVar`/`::nonevar` and 280 `::mangled_*`. Only a non-CK compiler or hand-built bytes can produce a disagreeing name.
- **Impact**: None on vanilla content. On crafted or third-party-compiled `.pex` files, a function is wrongly declined or mis-bound. It is also duplicate logic, which the user's global rules prohibit, and a bookkeeping gap: the skill's "check the two still agree" holds only because of the corpus.
- **Related**: #5880 (closed), PEX-D3-2026-10-15-04
- **Suggested Fix**: Delete the `lower.rs` copy and call `super::node::is_temp_var`, which is already `pub(crate)` and Champollion-exact. That keeps one predicate for both the lift and the declaration pass.

#### PEX-D4-2026-10-21-01: #5888 accepts `Parent` in every declaration-name position, so a local or parameter named `Parent` is declared fine but every use of it parses as `Expr::ParentAccess`, with 0 errors
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Untrusted-Input**: Yes (`.psc`)
- **Location**: `crates/papyrus/src/parser/mod.rs:421-427`. `keyword_as_ident` maps `Token::KwParent` to `"Parent"` for every `expect_ident` caller. Compare `crates/papyrus/src/parser/expr.rs:187`, where `parse_prefix` turns `KwParent` into `ParentAccess`.
- **Status**: NEW (introduced by `3081d23f2`, the #5888 fix; #5888 itself is closed and the struct-member case works)
- **Description**: #5888 needed `Parent` as a struct member name (F4SE `ObjectReference.psc`: `struct ConnectPoint { string parent }`) and as a `.member` name. The fix put it in the shared `keyword_as_ident` table. That table serves every name position that goes through `expect_ident`: locals, parameters, properties, functions, and qualified script, `Extends` and type names.

  The other keywords in that table (`Auto`, `Hidden`, `Native`, …) have no meaning as an expression, so allowing them as names is harmless. `Parent` does: in primary-expression position it is `ParentAccess`. So a name can now be declared and then never referred to. Every reference silently becomes the parent-script accessor.
- **Evidence**: The `cases` probe parsed `ScriptName T / Function F() / Int Parent = 1 / Parent = 2 / EndFunction`. The result is `[VarDecl(Parent), Assign { target: ParentAccess, … }]` with **0 errors**. A parameter would behave the same: `Function F(Actor Parent)` followed by `Parent.Kill()` calls the parent script's `Kill`.
- **Impact**: No vanilla or F4SE source declares `Parent` anywhere except that struct member. A grep of the on-disk FO4 `Scripts/Source` finds `parent` only in comments, apart from `ObjectReference.psc`. For any other `.psc` (mods, the extender preflight, the debug console), the result is a silent wrong AST instead of an error. It is the same kind of gap as #5021 and #5885: wrong tree, 0 diagnostics.
- **Related**: #5888 (closed), #5328
- **Suggested Fix**: Accept `KwParent` only where #5888 needs it, in struct-member declaration names and the name after `.`, and remove it from the shared `keyword_as_ident`. Alternatively, record a recovered error when a local or parameter is declared as `Parent`. Pin it with the probe's `Int Parent` case.

#### PEX-D4-2026-10-21-02: doc rot left by #5887 and #5884, plus a stale `Expr` table
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Untrusted-Input**: No
- **Location**:
  - `docs/engine/papyrus-parser.md:152`: the `Event` row says "a remote handler's `name` is `Sender.EventName`". Since #5887 (`3081d23f2`) the name is `Sender_EventName`, and the same file's `:133-136` already says so, so the file contradicts itself.
  - `docs/engine/papyrus-parser.md:160`: the `Expr` row lists no `Is` (#5322; since #5443 the `.pex` side lowers to it too) and no `NewStruct` (FO4 `new S`). Both are variants in `crates/papyrus/src/ast.rs:276,285`.
  - `crates/papyrus/tests/r5_round_trip.rs:2`: the module doc says the pipeline is "lex → preprocess → parse_script". `preprocess` was deleted by `af4a68ad7` (#5884).
- **Status**: NEW (the first line is from `3081d23f2`; the `#5890` sweep in `d148cd27b` missed the other two)
- **Impact**: Documentation only. The `Event` row is the one a consumer writing a name matcher would read.
- **Related**: #5890, #5887, #5884 (all closed)
- **Suggested Fix**: Change `:152` to `Sender_EventName`, add `Is` and `NewStruct` to the `Expr` row, and drop "preprocess" from the round-trip doc.

### Existing (open) issues, re-confirmed against HEAD

- **#4113**: `MAX_EXPR_DEPTH` (256, `lift.rs:380` aliasing the parser constant) and `MAX_REBUILD_DEPTH` (1024, `control_flow.rs:44`; `boolean.rs:61` derives it) are still independent caps. Unchanged.
- **#4115**: the `control_flow.rs:28` module doc still says the leftover `||`-shape block is "left unmerged and advanced past". The code fails closed (#1732).
- **#5445**: the `as`/`is` type operand is still taken from the next line. The `c5445` probe parsed `x = a as⏎Int` and `b = a is⏎Actor` with 0 errors.

### Checked and not reported

- **#5877 equivalence argument** (backs the A/B result). A collapse leaves the in-edge count of every surviving block unchanged: unlinking `src` and the rejoin and linking the adopted block cancel out. A declined collapse changes nothing, except on the exit-anchor path, and there the resume target is the exit. And each block's decision depends only on its own operand (walked by its own recursion first) and its rejoin. So walking the operand range a second time could never find a collapse the first walk missed.
- **Remote handlers with a namespaced sender** (`Event DLC01:Q.OnX` → `DLC01:Q_OnX`). I found no corpus evidence of how the compiler spells these: 0 of the 929 FO4/Starfield `::remote_` handlers have a namespaced sender. The VM's `handler_in` matches by `_event` suffix, so both spellings bind. Not a finding.
- **A `{doc}` inside a function body**, either on the same line as a statement or on its own line after one. The whole function is dropped and its later statements are re-read as top-level items. This is pre-existing item-level recovery, it reports 2 errors (not silent), and the CK grammar forbids the input ("can only appear on the line following a script header, property … or function definition").
- **`\` followed by a multi-line `;/` block comment**: it reports 2 errors and is not silent. No CK rule covers this case.
- **AutoReadOnly lowering**: `lower_property` emits a getter, not an `AUTO_READ_ONLY` flag, for a compiled `AutoReadOnly` property. The VM's `getter_constant` / `property_constant` (`crates/scripting/src/fragment/vm/script_model.rs:192-238`) read that getter shape, so behaviour is correct. Not a finding.
- **`StrCat` lifted to `+`**: this follows the source spelling. Typing it correctly belongs to the VM.

## Cross-audit notes

- `/audit-scripting`: the lift erases the int/float split (`IAdd`/`FAdd` → `+`, `IDiv`/`FDiv` → `/`), which is the frontend half of SCR-2026-10-21-02. Not re-reported.

Next step: `/audit-publish docs/audits/AUDIT_PAPYRUS_2026-10-21.md` (domain label `scripting`).
