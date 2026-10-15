**HEAD**: 5f7ff5920 (`crates/pex` + `crates/papyrus` identical to 9e11eb1fb) · **Baseline**: [AUDIT_PAPYRUS_2026-10-12.md](AUDIT_PAPYRUS_2026-10-12.md) (HEAD `5d31fd637`) · **Audited**: Dim 3 (Boolean/Control-Flow/Lower: the 944d0299e private-edge fold, five `lower.rs` commits, corpus re-measure, hostile-input timing), Dim 4 (`.psc` Lexer & Pratt Parser: e96d687ec, 3efa1c024, 20cbfd7b7, a03cc727f; fuzz and depth stress) · **Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode), Dim 2 (Decompiler CFG & Lift)

# Papyrus Frontends Audit: 2026-10-15

## Delta since baseline: 11 commits

`git log 5d31fd637..HEAD -- crates/pex crates/papyrus`:

| Commit | Change | Dim | Verdict |
|---|---|---|---|
| `944d0299e` (10-13) | `boolean.rs` private-edge guard: a short-circuit fold declines unless the operand (and an interior rejoin) has only the expected in-edges; `pex_dump_fn` example | 3 | Logic SOUND: in-edge counts are live, the exit anchor is excluded, and a declined fold reaches `reconstruct` as an equivalent `if`. Fixed the last corpus failure. Cost is O(blocks²) (finding D3-03) |
| `0c6fe4d34` (10-12) | `lower_local_decls`: declare user locals ahead of the body | 3 | Its `::` filter also drops the compiler's `::mangled_*` user locals (D3-04) |
| `a649b55b4` (10-12) | Variable and auto-property defaults → `initial_value` | 3 | Mapping SOUND for every value tag; the property lookup is O(P×V) (D3-02) |
| `d8b1d333c` (10-12) | #5683: `UserFlagMasks` once per `Pex` | 3 | SOUND (linear, `flag_index < 32` guard) |
| `f7afb0e07` (10-12) | #5685: property user flags → `PropertyFlags` | 3 | Incomplete: `Const`/`Conditional` live on the backing variable in real files (D3-05) |
| `20cbfd7b7` (10-13) | FO4 structs, `Expr::NewStruct`, array methods | 3, 4 | SOUND |
| `e96d687ec` (10-14) | #5325/#5328: trailing doc comments, namespaced headers, remote events | 4 | Terminates and is depth-bounded. Gaps in D4-03, D4-04 and D4-05 |
| `3efa1c024` (10-13) | #5444: glued `EndStruct`/`EndGroup` stay for their container | 4 | SOUND (keep-set covers every terminator that exists) |
| `a03cc727f` (10-15) | Deleted two zero-caller helpers | 4 | No lexing change; leaves an empty `impl Token {}` (D4-08) |
| `687d8353a`, `251d491ef` | Docs, a dropped dev-dep | — | n/a |

No commits touched `reader.rs`, `opcode.rs`, `model.rs`, `lib.rs`, `call_sites.rs` or `decompile/{cfg,lift,node,control_flow,event_names,mod}.rs`.

## Build, test and corpus state: CLEAN

```
cargo test -p byroredux-pex -p byroredux-papyrus
  papyrus 142 unit + 4 round-trip · pex 83 unit + 1 doc-test (r5_fidelity: 1 ignored, needs data) · 0 failed
pex_corpus_smoke (release)  Skyrim - Misc.bsa  Fallout4 - Misc.ba2  Starfield - Misc.ba2
  decompile → AST: ok 26,641 (100.0%)  err 0  panic 0
```

The baseline's single failure (`stimboxscript.pex`, the #1732 fail-closed shape) is gone: `944d0299e` declines the fold that used to delete a shared epilogue. Never-seen opcodes: `nop`, `array_rfindelement`.

**Dim 1/2 guard spot-check** (no delta): every named guard is present and green. They are `every_prefix_of_every_wire_valid_sample_is_rejected`, `hostile_vararg_count_errors_instead_of_ooming`, `string_copies_past_the_budget_are_refused`, `call_site_copies_are_bounded_by_the_string_budget`, `discriminants_match_on_disk_order`, `from_u8_round_trips_and_rejects_oob`, `metadata_matches_champollion{,_full_table}`, the three `jmpf`/`jmpt` CFG pins, `rebuild_expression_is_linear_up_to_the_wire_format_ceiling` and `a_temp_chain_past_the_depth_cap_is_declined_not_aborted`. `OpCode::from_u8` keeps its `>= MAX_OPCODE` check, its SAFETY comment and the `const _: () = assert!(TryLockGuards as u8 == MAX_OPCODE - 1)` backstop.

**Probes** (outside the repo, shared `CARGO_TARGET_DIR=/mnt/data/tmp/papyrus-probe-target`):
- Dim 3: crafted FO4-format `.pex` bytes through `parse` → `decompile_script`, a copy of HEAD with only `boolean.rs` reverted to `944d0299e^`, and census tools over Skyrim SE (14,026 `.pex`), FO4 (7,875), FO76 (7,233) and Starfield.
- Dim 4: 7M fuzz inputs (release, two seeds, and an overflow-checked debug build), 200k-long chain and nesting stress, near-cap combos on a 1 MiB stack, and the 30 on-disk FO4/F4SE `.psc` plus the r5 fixtures.

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 4 | 0 |
| LOW | 9 | 4 (#4113, #4115, #5443, #5445) |

### Untrusted-input robustness verdict: MET for crash classes (no panic, OOB, OOM or stack overflow). NOT MET for CPU time: one exponential and two quadratic holes

- **Crash classes**: no panic, out-of-bounds read, OOM or stack overflow was found from hostile `.pex` or `.psc`. 7M `.psc` fuzz inputs ran with 0 panics (worst 352 µs). 200k-long chains of every kind hit the 256 caps, and near-cap combos parse and drop on a 1 MiB stack. The `.pex` reader has no delta and its exhaustive-prefix guard is green.
- **CPU time**: `decompile_script` can be stalled by a small hostile file, and nothing (`catch_unwind`, the depth caps, the string budget) interrupts a CPU loop.
  - **PEX-D3-2026-10-15-01 (HIGH)**: the boolean pass is O(2^nesting). A 656-byte well-formed `.pex` takes 6.6 s; about 40 levels would take hours. This is inherited from Champollion and predates the baseline, which measured hostile timing only for the #5398 flag rescan.
  - **D3-02 (MEDIUM)**: the auto-property default lookup is O(P×V) (39 s at 1.77 MB). It is the #5683 class again, added after that fix.
  - **D3-03 (MEDIUM)**: the new private-edge guard costs O(blocks²) per function, about 47× the pre-guard boolean stage.
- Vanilla content is unaffected: the slowest vanilla decompile in the census is 18 ms.

### Decompile-rate verdict: re-measured, 26,641 / 26,641; it proves robustness, not fidelity

- **Rate**: 100%, up from 26,640, with 0 `Err` and 0 panics across Skyrim, FO4 and Starfield.
- **Robustness**: the harness counts `Err` and panic as failures, after `decompile_script`. Its shape check restates the assembly rules and calls nothing in `lower.rs`, so it shares no implementation with the code under test.
- **Exit status**: it does not gate decompile results; it exits 0 on decompile errors (D3-06).
- **Fidelity**: the rate says nothing about it. Fidelity is still the `.psc`-vs-`.pex` gate owned by `/audit-scripting`.
- **Fidelity gaps this pass found that the rate cannot see**:
  - `::mangled_*` locals left undeclared (D3-04; about 280 vanilla functions).
  - `Auto Const`/`Auto Conditional` property flags missing on 22.5k FO4 and 24.5k Starfield properties (D3-05).
  - Remote-event handler names that differ between the two frontends (D4-05).

## Decompiler Soundness Matrix

| Stage | Bounds-safe | Terminates (and in what time) | Total (no panic) | Fidelity-tested |
|---|---|---|---|---|
| Reader (`reader.rs`, `opcode.rs`, `call_sites.rs`) | YES: `take(n)` single gate; string budget; `u16`-capped capacities; guarded `transmute` | YES, linear | YES | Prefix-exhaustive over five builders |
| CFG (`cfg.rs`) | YES: `checked_target`, no `split(0)` | YES | YES | `jmpf`/`jmpt` pins |
| Lift + copy-propagation (`lift.rs`, `node.rs`) | YES: memoised depth vs `MAX_EXPR_DEPTH` | YES, linear (#2024 pin) | YES | Depth-cap pins; corpus |
| Boolean (`boolean.rs`) | YES: `MAX_REBUILD_DEPTH` | YES, but **O(2^nesting)** (D3-01) and O(B²) per function (D3-03) | YES | Private-edge pins; corpus |
| Control flow (`control_flow.rs`) | YES: `RecursionLimit` | YES, linear (jumps to body end) | YES; `||` residual fails closed (#1732) | `conditional_predecessor_fails_closed` |
| Lower (`lower.rs`) | YES: `flag_index < 32` | YES, but O(P×V) property defaults (D3-02) | YES: default arm unreachable from lift | Partial: drops `::mangled_*` locals (D3-04); property flags incomplete (D3-05) |
| `.psc` parser (`crates/papyrus`) | YES: two 256 caps, every chain link charged | YES: 7M fuzz inputs, worst 352 µs | YES | Partial: continuation applied inside comments (D4-01); spans not remapped (D4-02) |

**The Champollion departures, adjudicated**:
1. **No debug-line guard in `boolean.rs`**: BENIGN. Corpus at 100% and no fabricated `&&`/`||` seen.
2. **Termination guard in `boolean.rs`**: SOUND. It re-processes only after an interior merge. It bounds the iteration count, not the cost (D3-01).
3. **`is` → object-typed `Cast` in `lower.rs`**: still bookkept; tracked as #5443.
4. **NEW and not bookkept**: the `::` local filter (D3-04) departs from Champollion's `isTempVar`.

## Findings

### HIGH

#### PEX-D3-2026-10-15-01: the boolean pass re-walks every if/while body it has already recursed into, so its cost is O(2^nesting depth). A well-formed 656-byte `.pex` takes 6.6 s and a 40-deep one would take about 30 h

- **Severity**: HIGH (the skill escalates "unbounded … recursion reachable from untrusted bytes". This is unbounded CPU behind the recursion, and the depth cap does not bound it)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:183-201` (`BoolPass::rebuild`)
- **Status**: NEW. It has been there since the port; the recursion shape is unchanged since 7fdb694bd (#1815), and the probe reproduces it on the pre-944d0299e copy too.
- **Description**: For every conditional block whose last statement computes its condition and whose true edge (`&&`) or false edge (`||`) falls through, `rebuild` first recurses over the operand range (`self.rebuild(block.on_true(), block.on_false, depth + 1)`, line 186/190). If `collapse` then declines, it continues with `it = self.cfg.next_key(current)` (line 200), which is the first block of the range it just recursed over. Every `if`/`while` header matches this shape, so each nested body is walked once by the recursion and again by the enclosing loop. With nesting depth d, the work is W(d) = 2^d − 1. `MAX_REBUILD_DEPTH` (1024) caps only the stack depth. The blow-up is already hours at d ≈ 40. `control_flow::reconstruct` avoids the problem by setting `jump_to = Some(body_end)`, which skips the body after recursing into it. The boolean pass has no equivalent. Champollion's `rebuildBooleanOperators` (PscDecompiler.cpp:851-960) has the same walk, so this is inherited.
- **Evidence**: The probe emits N nested `if a == b` with one innermost statement: `N × (cmp_eq ::temp0 a b; jmpf ::temp0 → end)`, then `assign v 1; return`. It is well-formed compiler-shaped output and decompiles `Ok`. Release build:

  | N (nesting) | file | `decompile_script` |
  |---|---|---|
  | 8 | 314 B | 49 µs |
  | 16 | 466 B | 5.4 ms |
  | 20 | 542 B | 100 ms |
  | 24 | 618 B | 1.58 s |
  | 26 | 656 B | **6.65 s** |

  The time doubles with each extra level. Per-stage timing at N=24: cfg 5 µs, lift 12 µs, **boolean 1.55 s**, reconstruct 51 µs. Without the innermost statement, the same shape fails closed in `reconstruct`, but only after the exponential boolean pass has already run (N=22: 0.27 s → `ControlFlowFailed`).
- **Orchestrator re-run** (HEAD `5f7ff5920`, release): boolean stage 3.8 ms at N=16, 15 ms at N=18, 59 ms at N=20, 244 ms at N=22, doubling per level. In this re-run the probe's final decompile returned `Err` (`ControlFlowFailed`) rather than `Ok`; the exponential time is spent in the boolean stage before either outcome, so the finding stands as stated.
- **Impact**: A mod `.pex` of under 1 KB stalls the thread that decompiles it, for minutes to effectively forever. `decompile_catching_panics` (`translate_pex`) and the fragment populate paths cannot interrupt a CPU loop, and nothing else catches it: no panic, no allocation, no error. Vanilla content is fine; the slowest vanilla decompile in the census is 15–18 ms (`cwscript.pex`, `companionaffinityeventquestscript.pex`). Hand-written or generated mod code nested about 25 deep becomes noticeably slow.
- **Related**: #1815 / #2667 (the depth cap and why it exists), and the skill's termination-guard bullet. The iterative termination argument still holds; this is a separate, time-complexity bound.
- **Suggested Fix**: After the nested `rebuild` returns and `collapse` declines (or merges nothing), continue the outer walk at the range end (`block.on_false` for `&&`, `block.on_true()` for `||`), the same way `reconstruct` does. Otherwise, mark blocks whose range has already been processed. Add a pin that decompiles a 40-deep nested `if` in under 1 s.


### MEDIUM

#### PEX-D3-2026-10-15-02: the auto-property default lookup (a649b55b4) rescans `object.variables` once per property. That is O(P × V), the #5683 class again: a 1.77 MB `.pex` stalls `decompile_script` for 39 s

- **Severity**: MEDIUM (same class and rating as the baseline's PEX-D3-2026-10-12-01 / #5683)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:460-464` (`lower_property`, `initial_value`)
- **Status**: NEW (introduced by a649b55b4, after the baseline's "this is the only quadratic site in `lower.rs`" grep)
- **Description**: `prop.auto_var_name … .and_then(|var| object.variables.iter().find(|v| v.name.eq_ignore_ascii_case(var)))` runs once per property. Both tables are bounded only by their `u16` wire counts. The parse string budget does not help, because one shared string index serves every name. #5683's `UserFlagMasks` fixed the user-flag rescan, but this lookup was added afterwards. The #5683 pin `a_hostile_user_flag_table_decompiles_in_linear_time` (`lower.rs:816`) builds no properties, so it cannot see this.
- **Evidence**: The probe (`quad` mode) has N variables named `::aaaaaaaaaaX` and N auto properties whose backing name `::aaaaaaaaaaY` never matches (same length, so each comparison runs to the last byte). FO4 format, release build:

  | N | file | `parse` | `decompile_script` |
  |---|---|---|---|
  | 4,000 | 108 KB | 1.7 ms | 0.15 s |
  | 8,000 | 216 KB | 2.4 ms | 0.60 s |
  | 16,000 | 432 KB | 4.6 ms | 2.36 s |
  | 32,000 | 864 KB | 8.7 ms | 9.43 s |
  | 65,535 | **1.77 MB** | 19 ms | **39.0 s** |

  Doubling the input multiplies the time by about 4. Vanilla is unaffected: the largest P×V in the census is 358,801 (Skyrim).
- **Impact**: Same as #5683. A mod script stalls its loader thread for tens of seconds, and no net catches it.
- **Related**: #5683 (fixed, same class), #3938
- **Suggested Fix**: Build a case-insensitive `name → &Variable` map once per object, so the cost is O(P + V). Fix -04 needs the same map, because it reads the backing variable's flags too. Add auto properties to the hostile linear-time test.

#### PEX-D3-2026-10-15-03: the short-circuit private-edge guard (944d0299e) counts in-edges by scanning the whole CFG on every fold. That is O(B) per collapse and O(B²) per function, about 47× slower than before the guard on a 65 k-instruction `||`-chain function

- **Severity**: MEDIUM
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:284-297` (`incoming` closure in `BoolPass::collapse`)
- **Status**: NEW (introduced by 944d0299e)
- **Description**: `incoming(key)` iterates `self.cfg.blocks.values()` in full, and `collapse` calls it up to twice for every candidate that passes the fall-through test. A function of chained `||`/`&&` operands, with each chain kept under `MAX_EXPR_DEPTH` and many chains per function, makes about B/2 successful folds against a graph of about B blocks.
- **Evidence**: The probe (`chains` mode) has n chains of `t = a || a || … (201 operands); x = t`, so each chain is 402 instructions. Boolean-stage time:

  | n | instrs | file | boolean @HEAD | boolean, pre-guard copy |
  |---|---|---|---|---|
  | 40 | 16 k | 129 KB | 234 ms | 8.8 ms |
  | 80 | 32 k | 257 KB | 1.04 s | 27 ms |
  | 162 | 65 k | 521 KB | **4.36 s** | 92 ms |

  The `u16` instruction count bounds each function at about 4.4 s, but functions per state are also a `u16` count. The cost is therefore about 8 s per MB of file, scaling linearly in file size, against about 0.2 s per MB before the guard.
- **Impact**: A stall of the same class as -02 that hostile files can reach. Vanilla is unaffected: the slowest vanilla decompile in the census is 18 ms.
- **Related**: 944d0299e (the guard is correct; only its cost is the problem), -01
- **Suggested Fix**: Build a predecessor-count map once per pass, before `rebuild`. Keep it current inside `collapse`: drop the operand's and rejoin's out-edges, and add the adopted rejoin edges under `current`. That makes each check O(1). Alternatively, count only the blocks whose `next`/`on_false` could reach the key, using a predecessor index.

#### PEX-D3-2026-10-15-04: `lower_local_decls` (0c6fe4d34) drops every `::`-prefixed local, including the compiler's `::mangled_<name>_<n>` user locals that Champollion declares. About 280 vanilla functions are left with undeclared real locals, and write-only ones are mis-bound by the VM as script variables

- **Severity**: MEDIUM (mostly a fail-closed coverage loss; a small write-only subset mis-binds silently)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:283-298` (`.filter(|local| !local.name.starts_with("::"))`)
- **Status**: NEW (an unbookkept Champollion departure introduced by 0c6fe4d34)
- **Description**: Champollion's `declareVariables` (PscDecompiler.cpp:1354-1364) skips only `isTempVar` locals, defined at PscDecompiler.cpp:20-24 as `::temp*` not ending in `_var`, or `::nonevar`. It declares `::mangled_*` locals: the compiler's renaming of a local re-declared in a sibling scope, which `getVarName` unmangles back to the source name (`:25-46`). Our filter treats any `::` prefix as a temporary, so these real user locals get no `VarDecl`. The body still assigns and reads them under the mangled name.
- **Evidence**: Census (`mangled` bin) of `::mangled_*` locals: Skyrim 31, FO4 87, FO76 12, Starfield 150. Of these, 14 are in `Fragment_*` functions: 1, 1, 0 and 12 respectively. Examples: FO4 `ms13script.DivideSpoils ::mangled_i_0` (7 uses), Skyrim `byohhousescript.BuildHousePart ::mangled_roomindex_0`. No other non-temp `::` local names occur. Downstream:
  - A read of the undeclared name fails the VM static check (`vm.rs:5067-5080`, "unknown name") and the fragment lowerer's #5638 guard, so the function declines. This is the bulk of the cases.
  - The checker's assignment arm accepts any `Ident` target unchecked (`vm.rs:4995`). A local that is only written (3 Skyrim and 3 Starfield mangled locals have ≤ 1 bytecode use, e.g. `dlc2bloodskalbladescript.OnAnimationEvent ::mangled_swordweapon_0`) therefore passes. `assign` then routes it to `write_quest_variable(owner, "::mangled_…")` (`vm.rs:1647`): a deferred write of a junk script variable on the owner.
- **Impact**: About 280 vanilla functions, including 14 fragments, decline that would otherwise lower or run. A handful of write-only cases persist a spurious variable. All four games are affected.
- **Related**: #5638, 0c6fe4d34, `user_locals_are_declared_ahead_of_the_body_and_temporaries_are_not` (it pins only `::temp0`)
- **Suggested Fix**: Use Champollion's `isTempVar` predicate (`::temp…` not ending in `_var`, or `::nonevar`, case-insensitive) as the filter, so `::mangled_*` locals are declared under the same mangled name the body uses. Add a `::mangled_i_0` case to the pin.

#### PEX-D4-2026-10-15-01: `preprocess` applies `\` line continuation inside `;` comments, so the CK's own example silently drops the next line (and the CK-allowed `\ ; comment` is rejected)
- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/lexer.rs:7-36` (`preprocess` elides every `\` immediately followed by `\n`/`\r\n`/`\r`, before lexing, without knowing about comments or strings)
- **Status**: NEW
- **Untrusted-Input**: Yes (`.psc` files, `script.exec` console text, offline preflight)
- **Description**: The FO4 and Skyrim CK *Script File Structure* pages say: "Single line comments … consume all text after the semicolon, including any backslash characters that would otherwise let you continue onto the following line". They also say a comment may follow the continuation slash. Their example is `; Slash in this line is ignored, because it is in a comment \` ⏎ `x = 1 + 2 ; This line isn't inside the above comment`. `preprocess` runs before the lexer and joins the two lines, so the next line becomes part of the comment and disappears. The opposite case also breaks: `\` followed by `; note` or by trailing blanks is not elided. It lexes as `Token::Backslash`, which is a parse error, and the statement is lost.
- **Evidence** (probe):
  - The CK example inside `Function F()` gives F 0 statements and **0 errors**.
  - `Int x = 1 ; see Data\Scripts\` ⏎ `Int y = 2` keeps only `x`, with **0 errors**.
  - Top level `; dir C:\` ⏎ `Int Property P Auto` loses property P, with **0 errors**.
  - `x = 1 + \ ; note` ⏎ `2` (CK-legal) gives 3 errors and F is dropped. `x = 1 + \   ` ⏎ `2` gives 3 errors and F is dropped.
- **Impact**: This is a silent wrong AST with content loss, the same class as #5019 (MEDIUM): any statement, property or function header after a comment ending in `\` vanishes with zero diagnostics. A `.psc` frontend that drops lines also skews the offline extender preflight, whose missed calls go unreported.
- **Related**: #5019, #4479; finding 02 has the same root.
- **Suggested Fix**: Make continuation a lexer rule instead of a pre-pass. Add a logos skip rule such as `\\[ \t]*(;[^\r\n]*)?(\r\n|\r|\n)`. The `;` comment rule then naturally consumes a `\` inside a comment, spans stay in original coordinates and `OffsetMap` can go, which also fixes 02. Pin the CK example verbatim.


### LOW

#### PEX-D3-2026-10-15-05: #5685 maps property `Const`/`Conditional` from user-flag table entries that vanilla files never contain. The real bits sit on the auto property's backing variable, so 22.5 k FO4 / 24.5 k Starfield `Auto Const` and 3 k `Auto Conditional` properties still lower without those flags

- **Severity**: LOW (no consumer keys on `PropertyFlags` yet, per f7afb0e07's own message and a grep of scripting/save/bin; this is the residual of the baseline's LOW PEX-D3-2026-10-12-02)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:436-451` (`lower_property` user-flag loop), `:533-536` (`"const"` arm of `UserFlagMasks::from_pex`); test `property_user_flags_lower_into_property_flags` (`:749`)
- **Status**: NEW (incomplete fix of #5685)
- **Description**: The loop tests only `prop.user_flags`, and it resolves `CONST` through a user-flag name `"const"`. In vanilla data, every user-flag table is exactly `hidden=0, conditional=1` (Skyrim) or adds `default=2, collapsedonref=3, collapsedonbase=4, mandatory=5` (FO4; FO76 also has `transactional=6`). No table names `const`. `Auto Const` is encoded as the backing variable's `const_flag`, and `Auto Conditional` as the backing variable's conditional user bit. Champollion `PscCoder::writeProperty` copies both onto the property (PscCoder.cpp:388-400: "The flags defined on the variable must be set on the property", `writeUserFlag(*var)` + `if (var->getConstFlag()) " Const"`). The test pins a synthetic `UserFlag { name: "const", flag_index: 4 }` layout that the corpus contradicts.
- **Evidence**: Census bins `constprop` and `condprop`:
  - FO4: 22,550 of 25,679 auto properties have a backing variable with `const_flag != 0`. Starfield: 24,531 of 25,213.
  - Conditional bit on the backing variable: Skyrim 2,235, FO4 804, Starfield 94. On the property itself: 0 in every game.
  - Property-level bits that do work: `hidden` (Skyrim 1,461, FO4 381) and `mandatory` (FO4 2,440).
  - Also, struct-member `user_flags` are ignored (`lower.rs:556-563`, `is_hidden`/`is_conditional: false`): 3 FO4 and 12 Starfield members carry a flag.
- **Impact**: The `.psc` and `.pex` frontends still disagree on `CONST`/`CONDITIONAL` for most FO4/Starfield properties, which is what #5685 set out to close. A future consumer (save-skip of const properties, `GetVM*Variable` conditionals) would silently see none.
- **Related**: #5685, #5398, PEX-D3-2026-10-12-02
- **Suggested Fix**: For an auto property, OR the backing variable's `user_flags` into the mask test, and map the backing variable's `const_flag` to `PropertyFlags::CONST`. This reuses the per-object map from -02. Repin the test with the vanilla table layout. Apply the masks to struct members as well.

#### PEX-D3-2026-10-15-06: `pex_corpus_smoke` exits 0 on decompile `Err`, panic and shape mismatch, so the "100% decompile, zero shape mismatches" claim is not gated. `pex_dump_fn` cannot dump a property getter or setter

- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower (instruments)
- **Untrusted-Input**: No
- **Location**: `crates/pex/examples/pex_corpus_smoke.rs:298-302` (the exit gate counts only `stats.failed`, the parse failures); `crates/pex/examples/pex_dump_fn.rs:85-87` (iterates `object.states` only)
- **Status**: NEW
- **Description**: The harness tallies `decompiled_err` and `decompiled_panic` correctly, and the percentage is computed over all three outcomes. However, the doc comment's "Non-zero exit … so this is usable as a gate" covers parse failures only. A decompile-rate or shape regression still exits 0 and is visible only by reading stdout. Separately, `pex_dump_fn` walks `state.functions` only. A smoke failure inside a property `Get`/`Set` body (bodies that `decompile_script` does decompile, via `build_named_function`) cannot be disassembled with it.
- **Impact**: The tooling can silently miss a decompile regression if anyone uses the exit status as a gate. Neither defect affects the engine.
- **Suggested Fix**: Exit non-zero when `decompiled_err + decompiled_panic + decompiled_shape_mismatch > 0`. Have `pex_dump_fn` also iterate `properties[*].read_function/write_function`, named `<prop>.Get` and `<prop>.Set`.

#### PEX-D4-2026-10-15-02: AST spans are never remapped through `OffsetMap`, despite the doc; the extender preflight reports wrong source lines after any continuation
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/lib.rs:56-110`. The doc at `:61-63` says "Spans in the returned `Script` are remapped back to original-source coordinates via the `OffsetMap`", but only error spans are remapped (`:93-96`, `:103-106`). `parse_expr` remaps only the root span (`:40-43`). The same claim is in `docs/engine/papyrus-parser.md:63-65`. The consumer is `crates/scripting/src/compatibility.rs:564-565,629-636`.
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: Every `Spanned<…>` inside the AST keeps preprocessed offsets. `analyze_source_compatibility` passes `expr.span.start` into `source_line(source_text, …)` against the original text, so each earlier `\` continuation moves the reported line back by one.
- **Evidence**: The probe parses `…x = 1 + \` ⏎ `2` ⏎ `SKSE.Foo()`. The second statement's span starts at 38, which in the original text is the `2` of the continued line (original `SKSE` is at 40), so `source_line` reports line 4 for a call on line 5. A separate scripting-side issue: `source_line` counts only `\n`, so a CR-only file reports every finding on line 1. Route that to `/audit-scripting`.
- **Impact**: Preflight diagnostics (`examples/extender_preflight.rs`) point at the wrong line. No runtime effect.
- **Related**: 01 (same fix removes the map), #2668, #3944
- **Suggested Fix**: Fix 01 with a lexer-level continuation and spans become original by construction. Otherwise add a span-remapping AST visitor in `parse_script` / `parse_expr`, or correct the doc.

#### PEX-D4-2026-10-15-03: a qualified ident's segment after `:` is pulled up from the next line with 0 errors, now including the ScriptName, Extends, Import and event sender (#5021 class)
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:421-427`. The `:` test is raw (`check_raw`), but the segment after it goes through the newline-skipping `expect_ident`. e96d687ec routes `parse_script_header` (`crates/papyrus/src/parser/script.rs:107-113`), `parse_import` (`:221-222`) and `parse_event_name` (`:377-378`) through it.
- **Status**: NEW (sibling of open #5445, closed #5021 / #5331)
- **Untrusted-Input**: Yes
- **Description**: e96d687ec guards only the first segment with `expect_same_line`. A segment after a line-ending `:` is a mandatory operand, and the parser takes it from the next line.
- **Evidence** (probe, all **0 errors**):
  - `ScriptName DLC01:` ⏎ `Foo` gives name `DLC01:Foo`.
  - `ScriptName Foo Extends DLC01:` ⏎ `Base` gives parent `DLC01:Base`.
  - `Import DLC01:` ⏎ `Utils` gives an import of `DLC01:Utils`.
  - `Event DLC01:` ⏎ `Q.OnX(…)` gives an event named `DLC01:Q.OnX`.
  - The type and expression positions behave the same way, pre-existing: `DLC01:` ⏎ `Bar x` gives `Var x`, and `parse_expr("a:\nb")` gives `Ident("a:b")`.
  - The new pin `a_namespaced_parent_on_the_next_line_is_still_not_the_parent` covers only a newline *before* the first segment.
- **Impact**: The parser accepts source the reference compiler rejects. The glued reading is what the author meant, so this is LOW, as with #5021.
- **Suggested Fix**: Use `expect_ident_raw` for every segment after `:` in `parse_qualified_ident`, and pin the header forms in `a_mandatory_operand_on_the_next_line_is_an_error`.

#### PEX-D4-2026-10-15-04: a same-line doc comment lets the next item or statement share the line, with 0 errors (`take_trailing_doc` never requires the line break after the doc)
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:170-180` (`take_trailing_doc` consumes the following `Newline` only "if present") and `:292` (`expect_eol` returns `Ok` on a `DocComment` without consuming it, so the terminator is a doc rather than a line break)
- **Status**: NEW. The function-body form is new in e96d687ec; the top-level form predates it.
- **Untrusted-Input**: Yes
- **Description**: #5322 made `expect_eol` enforce end-of-statement, but a `{…}` still counts as one. Before e96d687ec, a doc left by `expect_eol` hit `parse_block` and errored. Now `take_trailing_doc` swallows it and whatever follows on the same line parses as the next body statement or item.
- **Evidence** (probe, all **0 errors**):
  - `Function F() {d} Int x = 1` gives F with 1 body statement.
  - `Function F() native {d} Int Property P Auto` gives F and P.
  - `Int a {doc} Int b` gives two variables (pre-existing).
  - The intended forms parse as specified: a trailing doc on the next line, or a doc on the same line followed by a line break.
- **Impact**: The parser accepts malformed source; the CK grammar puts the doc on its own line. No crash, no loop.
- **Suggested Fix**: After consuming the doc in `take_trailing_doc`, require `Newline`/EOF by calling the `expect_eol` error path when anything else follows. Have `expect_eol`'s `DocComment` arm do the same (consume the doc, then require a line break).

#### PEX-D4-2026-10-15-05: remote and custom event handler names take two shapes: `.psc` gives `Sender.Event`, `.pex` gives `Sender_Event`, and the VM's matcher binds only the latter
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser (frontend parity)
- **Location**: `crates/papyrus/src/parser/script.rs:376-388` (`format!("{}.{}")`). The other side is `crates/pex/src/decompile/lower.rs:358-359`, which strips `::remote_` and leaves `Actor_OnDeath`. The matcher is `crates/scripting/src/fragment/events.rs:204-213`: `_<event>` suffix for remote events, exact mangled `script_Event` for custom events.
- **Status**: NEW (same family as open #5443)
- **Untrusted-Input**: No (consistency)
- **Description**: e96d687ec's rationale is that a dotted name makes name-matching consumers decline the handler. The engine's only consumer of these names, `handler_in`, wants to *bind* remote and custom handlers, and it matches the compiler's mangled spelling. So a `.psc`-sourced `Event ObjectReference.OnActivate(…)` or `Event QF_X.Ping(…)` never fires. The VM fixtures avoid this by hand-writing the mangled form in `.psc` (`fragment/vm/tests.rs:5255`, `Event ObjectReference_OnActivate(…)`).
- **Evidence**: `lower.rs:359` `name.strip_prefix("::remote_")`, and `events.rs:206-210` `lower.ends_with(&format!("_{event}"))`. Probe: `.psc` gives the name `Actor.OnDeath`.
- **Impact**: This is latent. `.psc` reaches the VM only through `script.exec`, which runs one statement and cannot define events, and through tests. A `.psc`-vs-`.pex` AST equality check is impossible for any FO4 script with remote handlers.
- **Suggested Fix**: Normalise at the parser boundary. Either the `.psc` frontend emits the compiler's `Sender_Event` spelling, which a script's own `OnDeath` still declines, or both frontends carry a structured `remote_sender: Option<Identifier>` on `Event`.

#### PEX-D4-2026-10-15-06: `Parent` is rejected in name positions, so F4SE `ObjectReference.psc`'s `struct ConnectPoint { string parent }` loses a member. It is listed as a KNOWN_GAP but no issue tracks it
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:391-407` (`keyword_as_ident` has no `KwParent`). Reached from `parse_variable_body` (`crates/papyrus/src/parser/stmt.rs:251`) and `parse_member_access` (`crates/papyrus/src/parser/expr.rs:270`). It is listed in the test's `KNOWN_GAPS` at `crates/papyrus/src/parser/script.rs` (`fo4_script_sources_parse_clean_when_present`).
- **Status**: NEW (an untracked known gap: closed/open searches for "Parent keyword", "ObjectReference.psc" and "string parent" return nothing)
- **Untrusted-Input**: Yes
- **Description**: F4SE's shipped, compiled `ObjectReference.psc:1211-1222` declares `string parent` as a struct member, so the reference compiler accepts `parent` as a name. The parser records an error and drops the member, leaving 9 of 10 members. `cp.parent` member access would fail the same way.
- **Evidence**: Corpus probe: `ObjectReference.psc items=295 errs=1 first="expected identifier (variable name), found 'Parent'"`. The other 32 files are clean.
- **Impact**: The `ConnectPoint` struct has the wrong shape. An error is recorded, so this is not silent.
- **Suggested Fix**: Accept `KwParent` as an identifier in declaration-name positions and after `.`. Keep `Parent` as `Expr::ParentAccess` only in expression-primary position. Then remove the KNOWN_GAPS entry; the gate fails until it is removed.

#### PEX-D4-2026-10-15-07: an unterminated `;/` block comment or string literal is accepted with 0 errors; the block comment silently swallows the rest of the file
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/token.rs:19-28` (`skip_block_comment` consumes to EOF and returns `Skip`), `:69-78` (`parse_string_literal` ends at the line break and returns the partial string), `:38-42` (unterminated doc comment, same pattern)
- **Status**: NEW (closed/open searches for "unterminated" find only #2185, the container hang)
- **Untrusted-Input**: Yes
- **Description**: Each callback recovers silently instead of routing to the `LexError` path that `parse_int` uses.
- **Evidence** (probe):
  - `…EndFunction` ⏎ `;/ oops` ⏎ `Function G()…` ⏎ `Function H()…` parses only F, with **0 errors**; G and H are gone.
  - `String s = "abc` ⏎ gives a `StringLit("abc")` with **0 errors**.
  - An unterminated `{` does surface an error, but only indirectly, through the enclosing body.
- **Impact**: A truncated or hand-edited file loses everything after the comment with no diagnostic. The input is invalid either way, so this is LOW, as with #4479, rather than #5019's MEDIUM.
- **Suggested Fix**: Make the three callbacks return an error on a missing terminator (logos `FilterResult::Error` / `Result`), so `lex()` records a `LexError` and inserts its placeholder.

#### PEX-D4-2026-10-15-08: doc and code-comment rot around the #5325 docs, logos 0.16 and a03cc727f
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser (doc-rot)
- **Location**:
  - `docs/engine/papyrus-parser.md:69` and `:102` still say `ignore(ascii_case)`. The code has used `ignore(case)` since `15a6b1d2d`.
  - `docs/engine/papyrus-parser.md:110-111` and `crates/papyrus/src/parser/script.rs:321-322` say "a leading `{…}` is still accepted as a fallback for functions, events and properties".
  - `crates/papyrus/src/parser/stmt.rs:77-79` says it "skips leading newlines / doc-comments".
  - `crates/papyrus/src/parser/mod.rs:279-280` calls `;;` a doc comment.
  - `crates/papyrus/src/token.rs:347-348` has an empty `impl Token {}`, left by a03cc727f.
- **Status**: NEW
- **Untrusted-Input**: No
- **Description / Evidence** (probe):
  - The leading-doc fallback never attaches anywhere. At top level the item loop discards `skip_newlines_collect_doc()`'s result (`script.rs:77`), so `{lead}` ⏎ `Function F()` gives F with `doc=None` and 0 errors. Inside State, Group or property-accessor containers the catch-all arm sends the `DocComment` to `parse_type`, which is a recorded parse error (`leading doc fn in state`: 1 error).
  - `parse_stmt_inner` calls `skip_newlines` only.
  - Doc comments are `{…}`, not `;;`.
  - No test pins the leading form.
- **Impact**: Readers are misled about accepted input; no runtime effect.
- **Suggested Fix**: Correct the three doc statements, or make the fallback real by threading the top-level loop's collected doc into the item parsers. Delete the empty `impl`.


## Already tracked: re-verified this pass, not re-filed

| Issue | State at HEAD |
|---|---|
| #4113 | OPEN. `MAX_REBUILD_DEPTH = 1024` (`control_flow.rs:43`, re-exported by `boolean.rs`) and `MAX_EXPR_DEPTH = 256` (`lift.rs:380`) are unchanged and still independent |
| #4115 | OPEN. The `control_flow.rs:27-29` doc still says the residual branch is "advanced past"; the code at `:206-217` fails closed |
| #5443 | OPEN. The `lower.rs:15-26` doc still says the AST has no `is` operator, but `Expr::Is` exists (`crates/papyrus/src/ast.rs:276`). Opcode 36 still lowers to `Cast`. Related to D4-05 (frontend parity) |
| #5445 | OPEN. `parse_base_type` still opens with `skip_newlines()` (`parser/mod.rs:445-446`). `x as` ⏎ `Actor` and `x is` ⏎ `Actor` both parse with 0 errors. D4-03 is its qualified-ident sibling |
| #5683, #5684 | Fixed and verified (`d8b1d333c`, `687d8353a`) |
| #5685 | Closed, but incomplete on real data (D3-05) |

## Cross-audit routing

- **`/audit-scripting`**:
  - `compatibility.rs`'s `source_line` counts only `\n`, so a CR-only file reports every preflight finding on line 1 (noted under D4-02).
  - D3-04's write-only `::mangled_*` locals reach `write_quest_variable` through the VM checker's unchecked assignment arm (`vm.rs:4995`). The decompiler fix removes the cause; the checker's leniency is that audit's call.
  - D4-05: the VM's `handler_in` is the consumer that needs one name shape.
- **`/audit-tech-debt`**: the empty `impl Token {}` (D4-08).

## Summary

| Severity | NEW | Existing |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 4 | 0 |
| LOW | 9 | 4 |

Suggested next step: `/audit-publish docs/audits/AUDIT_PAPYRUS_2026-10-15.md`. Every finding takes the domain label `scripting`; D4-08 is `documentation` + `doc-rot`, and D3-06 adds `test-gap`. None is title-specific except D3-05 (`game:fo4`, `game:starfield`).
