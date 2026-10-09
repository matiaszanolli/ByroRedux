**HEAD**: 00f580e09 · **Baseline**: [AUDIT_PAPYRUS_2026-10-05.md](AUDIT_PAPYRUS_2026-10-05.md) (HEAD `a2c24b16e`) · **Audited**: Dim 4 (`.psc` Lexer & Pratt Parser — the #5322 delta, with A/B probes), Dim 3 (Boolean/Control-Flow/Lower — corpus re-measure + opcode-36 census) · **Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode), Dim 2 (Decompiler CFG & Lift)

# Papyrus Frontends Audit — 2026-10-08

This pass was delta-scoped and run solo, as part of `/audit-suite --preset comprehensive`. Each dimension's
result is in `/tmp/audit/papyrus/dim_N.md`.

## Delta since baseline: 2 commits

| Commit | Change | Dim | Verdict |
|---|---|---|---|
| `a5c3f2428` (10-05) | #5322: `expect_eol` now enforces end of statement, `parse_expr` rejects trailing tokens, and `Is` is lexed into a new `Expr::Is` node | 4 | **SOUND** for the bug it fixes. It has two side gaps (findings 02 and 03 below) and left stale cross-frontend docs (D3-01) |
| `fbadf5c7b` (10-05) | Adds a transparent `Expr::Is` arm to `pex_corpus_shapes.rs:172` | 3 | Sound. It touches only an example |

`git log a2c24b16e..HEAD` shows no commits on `reader/opcode/model/lib/call_sites.rs` or on `decompile/*.rs`.

## Build, test and corpus state: CLEAN

```
cargo test -p byroredux-pex -p byroredux-papyrus
  papyrus 129 unit (+6 since 10-05: the #5322 tests) + 4 round-trip · pex 75 unit + 1 doc-test · 0 failed
pex_corpus_smoke  Skyrim - Misc.bsa  Fallout4 - Misc.ba2  Starfield - Misc.ba2
  parse 26,641 / 26,641 · decompile → AST 26,640 / 26,641, 0 panic
  only failure: stimboxscript.pex CheckTriggeringObjectAndDoOnceStatus (the known #1732 fail-closed case)
```

## Executive Summary

**4 new findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 3 LOW. 7 findings are already tracked:** #4113, #4115,
#5325, #5328, #5331 and #5333 are open and re-verified as still present; #5322 is closed and its fix is
verified.

- **PEX-D4-2026-10-08-01 (MEDIUM):** the script header accepts only `Native`, `Const`, `DebugOnly` and
  `Hidden`. Three other script flags are rejected with a parse error:
  - `Conditional` (Skyrim and FO4). 1,182 vanilla script objects carry it, measured from `.pex` user
    flags.
  - `BetaOnly` and `Default` (FO4).

  The AST cannot represent the Conditional flag at all. In the same family, a struct member's `Hidden`
  and a group's `Collapsed` are also rejected.
- **PEX-D3-2026-10-08-01 (LOW):** #5322 added `Expr::Is`, but the `.pex` decompiler still lowers opcode 36
  to `Expr::Cast`. Three pieces of documentation are now wrong:
  - `ast.rs` says the two frontends agree on one shape.
  - The `lower.rs` doc says the AST has no `is`.
  - The `lower.rs` doc also says "≈0 vanilla scripts" use opcode 36. The census found 78 instructions in
    37 scripts.

  The pin test says to flip it once the AST gains an `Is` node; that has not happened.
- **PEX-D4-2026-10-08-02 (LOW):** the new `skip_to_line_end` recovery does not stop at `EndStruct` or
  `EndGroup`. On `Int a = 1 EndStruct`, recovery consumes the terminator, and everything after it
  becomes part of the struct.
- **PEX-D4-2026-10-08-03 (LOW):** the type after `as` or `is` can be pulled up from the next line, with 0
  errors (`x is` ⏎ `Actor`). The root is the same as #5331: `parse_base_type` skips newlines.

### Untrusted-input robustness verdict: still MET (NO panic, OOB, OOM or stack overflow)

- **`.pex`:** no reader or decompiler source changed. Every bounds guard and every OOM guard passes. The
  compile-time assert at `opcode.rs:74` and the `>=` gate at `opcode.rs:137-142` are both unchanged. All
  26,641 real files parse.
- **`.psc`:** the new `Is` arm in the Pratt loop charges depth through `enter_chain_link`, the same way
  `as` does, and the depth-cap guards pass.

  I checked whether #5322's recovery can hang. A stray terminator keyword was placed after a statement
  inside every container (If, While, Function, Event, Property, State, Group, Struct, script level):
  12 probes. All 12 terminate. `skip_to_line_end` either consumes at least one token, or leaves a
  terminator that the enclosing block or `skip_to_next_line` then consumes.

  None of the new findings can crash the parser. The production paths that reach `.psc` parsing are
  unchanged: the offline `extender_preflight`, and the debug console via `parse_expr`.

### Decompile-rate verdict: re-measured, exactly reproduced

| Game | Decompiled | vs 10-05 |
|---|---|---|
| Skyrim SE | 14,026 / 14,026 | unchanged |
| FO4 | 7,874 / 7,875 | unchanged (the #1732 `\|\|` fail-closed case) |
| Starfield | 4,740 / 4,740 | unchanged |
| **Total** | **26,640 / 26,641**, 0 panics | unchanged |

The rate shows the decompiler is **robust**. It does not show the output is **faithful**. That evidence
comes from the `/audit-scripting` Dim 1 `.psc`-vs-`.pex` gate (`r5_fidelity`, which was not re-run this
pass because nothing in the decompiler changed).

All four `docs/r5/source` fixtures parse cleanly. None of them uses the `Conditional` script flag. That
is why finding 01 never showed up in the fidelity gate.

## Verification of #5322 (`a5c3f2428`)

**A/B method.** I exported the pre-fix parser with `git archive a5c3f2428^ crates/papyrus/src` into the
session scratchpad. Then I built two otherwise identical probe binaries, one against the old parser and
one against HEAD. No repo files were touched.

| Probe | Old | New |
|---|---|---|
| `If f is Actor && g` / `Bool b = x is Actor` | 0 items, 3 errors | `Expr::Is`, 0 errors ✔ |
| `(akX is Actor) == true` | error | 0 errors ✔ |
| `x = 1 EndIf` (glued) | 0 errors (silent) | 1 error, and the If still closes ✔ |
| `Debug.Trace("a") Debug.Trace("b")` | 0 errors (silent) | 1 error ✔ |
| `Return x y` | 0 errors (silent) | 1 error ✔ |
| `Int a = 1 EndStruct` / `… Auto EndGroup` | 0 errors, 2 items | **4 errors; the following Function is lost** (finding 02) |
| 29 F4SE `.psc` + 4 `docs/r5` fixtures | 28/33 clean | **identical** item and error counts per file |
| `Int Is = 5` / `Function Is()` | ok | now an error. But **no `.pex` string table in the 26,641-file corpus contains an `is` identifier**, so no vanilla source is affected |

`parse_expr` now calls `require_input_end` (`lib.rs:38`), and `parse_expr_rejects_trailing_tokens`
passes. The downstream exhaustive `Expr` matches gained transparent `Is` arms:
- `compatibility.rs:588`
- `translate/effects.rs:645`
- `mq101_conformance.rs:220,268`
- `pex_corpus_shapes.rs:172`

## Findings

Deduplication used three sources:
- `/tmp/audit/issues.json` (113 open issues)
- Closed-issue searches for "ScriptName Conditional", "script header flags", "ScriptFlags conditional",
  "struct member Hidden", "Collapsed group flag", "papyrus as newline" and "skip_to_line_end"
- All `AUDIT_PAPYRUS_*` reports

The only near-matches are #4763, #5021 and #5331, which are newline-glue siblings. None covers these
four findings.

---

### MEDIUM

#### PEX-D4-2026-10-08-01: Script-header `Conditional` (and `BetaOnly` / `Default`) is rejected and unrepresentable — 1,182 vanilla script objects carry it; struct-member `Hidden` and group `Collapsed` likewise rejected

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/script.rs:109-135` (`parse_script_header` flag loop),
  `crates/papyrus/src/ast.rs:47-55` (`ScriptFlags`: `NATIVE | CONST | DEBUG_ONLY | HIDDEN` only),
  `script.rs:255-273` (`parse_variable_flags`, which struct members also go through: `Conditional`
  and `Const` only), `script.rs:705-720` (group flags: `CollapsedOnRef` and `CollapsedOnBase` only)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The FO4 CK *Flag Reference* (local falloutck wiki dump) defines these flags:
  - **Script:** `Conditional`, `Const`, `DebugOnly`, `BetaOnly`, `Hidden`, `Native`, `Default`.
    `Conditional` is also the standard Skyrim script flag.
  - **Struct member:** `Hidden`.
  - **Group:** `Collapsed`, alongside `CollapsedOnRef` and `CollapsedOnBase`.

  The header loop stops at anything outside its four flags. Before #5322, the leftover token caused an
  "expected type" error. It now causes an "end of statement" error and the rest of the line is skipped,
  so either way the file records an error. `ScriptFlags` has no `CONDITIONAL` bit, so the flag cannot
  appear in the AST even after a fix to the loop. The `.pex` side (`lower.rs:505-508`) maps only
  `CONST`.
- **Evidence**:
  - Probes, identical on the old and new parser except for the error text:
    - `ScriptName DialogueFollowerScript extends Quest Conditional` → 1 error
    - `… Const Hidden Conditional` → 1 error at `Conditional`
    - `… BetaOnly` → 1 error
    - `… Default` → 1 error
    - `Group G Collapsed` → 1 error
    - struct member `Float b Hidden` → 1 error
  - Corpus census (scratch probe over the `.pex` user-flag tables, measured this pass): **1,182** script
    objects have the `conditional` user flag set. That is Skyrim SE 761 of 14,026, FO4 346 of 7,875, and
    Starfield 75 of 4,740. Each one's `.psc` source opens with a header this parser rejects.
- **Impact**: About 1,200 vanilla `.psc` files can never parse with zero errors. That includes the
  quest scripts whose variables the condition system reads (`GetVMQuestVariable`). So a strict-fail
  caller (`result.1.is_empty()`) refuses all of them. `extender_preflight` counts each one as an input
  error. Recovery skips only the header line and items still parse, so this is not silent and not a
  crash. MEDIUM, consistent with #5328.
- **Related**: #5328 (FO4+ grammar forms), #4763 (raw-stream flag sites), `script.rs:26-30` module doc
  (unknown flags become errors by design, but `Conditional` is a known keyword)
- **Suggested Fix**:
  - Add `CONDITIONAL`, `BETA_ONLY` and `DEFAULT` to `ScriptFlags`, accept them in the header loop, and lex
    a `Default` and a `Collapsed` keyword. Both need adding to `keyword_as_ident` so they stay valid
    names.
  - Let struct members take `Hidden`.
  - Map the `.pex` object user flags (`conditional`, `hidden`, `default`) into `ScriptFlags` in
    `lower.rs` so both frontends agree.

---

### LOW

#### PEX-D3-2026-10-08-01: `.pex` opcode 36 still lowers to `Expr::Cast` after #5322 added `Expr::Is`; three docs now claim agreement / absence that the code and corpus contradict

- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Location**: `crates/pex/src/decompile/lower.rs:125-134` (the `op == "is"` → `Expr::Cast` arm);
  `lower.rs:13-26` (module doc); `crates/papyrus/src/ast.rs:259-262` (`Expr::Is` doc);
  `lower.rs:558-561` (the `an_is_opcode_lowers_to_an_object_typed_cast` doc)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: These are the stale claims, each set against the current code or the corpus:

  | Location | Claim | Actual |
  |---|---|---|
  | `ast.rs` | "The two frontends agree: the `.pex` decompiler lifts `OpCode::Is` to the same binary shape" | `lift.rs:231` does produce a Node `BinaryOp "is"`, but `lower.rs:125` turns it into `Expr::Cast`. A `.psc` `f is Actor` gives `Expr::Is`, while the same line compiled to `.pex` gives `Expr::Cast` |
  | `lower.rs` module doc | "The shared AST has no `is`" | It now does |
  | `lower.rs` module doc | "≈0 vanilla scripts contain opcode 36 (no `.psc` construct emits it)" | The census found **78** `is` instructions in **37** FO4 and Starfield scripts |
  | Pin test doc | "If the AST ever gains an `Is`-capable expression, this is the test to flip" | The trigger has fired and the test was not flipped |

  The SKILL text already notes that the two frontends "do NOT yet share one shape". The code docs say
  the opposite.
- **Evidence**: Opcode census (scratch probe, this pass): all 78 type operands are object or script
  types, never a primitive. The largest groups are actor 21, container 11, referencealias 5 and
  refcollectionalias 5. `pex_corpus_smoke` independently reports `is 78`.
- **Impact**: For object types, `x as T` is truthy exactly when `x is T`, so every vanilla occurrence
  lowers to a condition with the same truth value. I found no recognizer that keys on a Cast in
  condition position and would mis-handle one. The impact is therefore the wrong documentation, plus
  two frontends that produce different AST shapes for the same source. A future consumer that matches
  `Expr::Is` will never see one from `.pex`. LOW.
- **Related**: #5322, closed #4477 (the original departure)
- **Suggested Fix**: Lower `op == "is"` to `Expr::Is`, flip the pin test to assert `Expr::Is`, and
  update the `lower.rs` module doc so it no longer lists this as a departure. Correct the census claim.

#### PEX-D4-2026-10-08-02: `skip_to_line_end` (#5322 recovery) consumes `EndStruct` / `EndGroup`, so a glued container terminator swallows the rest of the file

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:288-306`
- **Status**: NEW (side gap of the #5322 fix)
- **Untrusted-Input**: Yes
- **Description**: The un-consume list covers `Else`, `ElseIf`, `EndIf`, `EndWhile`, `EndEvent`,
  `EndFunction`, `EndProperty` and `EndState`. It omits `KwEndStruct` and `KwEndGroup`, even though
  the `expect_eol` doc and the commit message both promise that `End*` terminators are left in place.
  The struct and group member loops (`script.rs:694`, `:741`) call `expect_eol`. On a glued
  terminator, recovery eats it, so the container stays open until EOF.
- **Evidence**: A/B probe, `ScriptName Foo` ⏎ `Struct S` ⏎ `Int a = 1 EndStruct` ⏎ `Function F()` ⏎
  `EndFunction`:
  - Old parser: 2 items, 0 errors.
  - New parser: **1 item, 4 errors** ("type, found Function", …, "UnexpectedEof EndStruct"). `F` is
    lost.

  The same happens with `Int Property A Auto EndGroup`.
- **Impact**: This source is invalid Papyrus, so errors are correct. But the error cascade drops valid
  items that follow, which every other terminator avoids.
- **Suggested Fix**: Add `Token::KwEndStruct | Token::KwEndGroup` to the un-consume arm, and add the
  struct and group shapes to `glued_statement_tail_is_an_error_and_not_a_statement`.

#### PEX-D4-2026-10-08-03: The type operand of `as` / `is` is pulled up from the next line with 0 errors — `parse_base_type` skips newlines (#5331 sibling, same root)

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:425-426` (`parse_base_type` starts with
  `skip_newlines()`), reached from `parser/expr.rs:364-366` (`parse_cast`) and `:381-383` (`parse_is`)
- **Status**: NEW (sibling of open #5331)
- **Untrusted-Input**: Yes
- **Description**: #5021 moved each keyword's mandatory operand onto the raw token stream. `as`, and
  the new `is`, both have a mandatory type operand, but they reach it through `parse_type` →
  `parse_base_type`, which skips newlines first. #5331 reports this only for `New`, and its suggested
  fix is local to `parse_new_expr`. The shared cause is `parse_base_type`'s leading `skip_newlines`.
- **Evidence**: Probes, old and new parser alike (the `is` case only exists on the new one):
  - `Bool b = x is` ⏎ `Actor` → 1 item, **0 errors**
  - `Bool b = x as` ⏎ `Actor` → 1 item, **0 errors**
- **Impact**: The parser accepts source that the reference compiler rejects. The glued reading is what
  the author meant, so this is LOW, as with #5021 and #5331.
- **Suggested Fix**: Drop the leading `skip_newlines()` from `parse_base_type` and audit its callers.
  Alternatively, put `expect_same_line` before `parse_type` in `parse_cast`, `parse_is` and
  `parse_new_expr`, which fixes all three at once with #5331. Pin both probes in
  `a_mandatory_operand_on_the_next_line_is_an_error`.

---

## Already tracked: re-verified this pass, not re-filed

| Issue | Severity | State this pass |
|---|---|---|
| #5325 | MEDIUM | Still present: `Function F()` ⏎ `{doc for F}` ⏎ `Int x = 1` → F dropped, and `x` hoisted to a script variable (2 errors) |
| #5328 | MEDIUM | Still present: `extends DLC01:Base` → error at `:` |
| #5331 | LOW | Still present: `New` ⏎ `Int[5]` → 0 errors |
| #5333 | LOW | Still present: `near-term-action-plan.md:108` still lists the closed #4471–#4479 sweep |
| #4113 | MEDIUM | Still present: `MAX_EXPR_DEPTH` 256 (`lift.rs:380`) and `MAX_REBUILD_DEPTH` 1024 (`control_flow.rs:44`) are independent |
| #4115 | LOW | Still present: `control_flow.rs:28` still says "advanced past" |
| #5322 | (closed) | Fix verified sound (above) |

## Decompiler Soundness Matrix

Only the rows that changed this window carry new evidence. The full matrix is in AUDIT_PAPYRUS_2026-09-19.md.

| Pass | Bounds-safe | Terminates | Total | Fidelity-tested | Change this window |
|---|---|---|---|---|---|
| Reader | Yes | Yes | Yes | 26,641 parse | none |
| CFG | Yes | Yes | Yes | JmpF/JmpT pins | none |
| Lift + copy-prop | Yes (`lift.rs:469`) | Yes | Yes (`:441-455`) | Yes | none |
| Boolean | Yes | Yes (`RecursionLimit`) | Yes | corpus + gate | none |
| Control-flow | Yes | Yes | Yes (fails closed, #1732) | corpus + gate | none (#4115 open) |
| Lower + assembly | Yes | Yes | Yes | census | **`is` → `Cast` is now an unnecessary departure (D3-01)** |
| `.psc` lexer | Yes | Yes | Yes | CR/CRLF pinned; `Is` keyword added (no corpus collision) | #5322 |
| `.psc` parser | Yes (depth caps) | Yes (12 recovery probes) | Yes | Partial. EOL is now enforced, but there are flag gaps (01), glued struct/group terminators (02), `as`/`is` newline glue (03), plus #5325, #5328, #5331 | #5322 |

**The three Champollion departures.** The two in `boolean.rs` (no debug-line guard, and the termination
guard) are unchanged. The corpus rate reproduced exactly and those files have no commits, so the 09-19
"benign" ruling stands.

The third departure, `is` → `Cast` in `lower.rs`, is still benign in effect: all 78 vanilla uses are
object-typed. Its justification is gone, though, because the AST now has `Expr::Is` (D3-01).

## Coverage notes

- **Probes:** `probe_old` and `probe_new` (`.psc`) and `pexprobe` (an opcode-36 and user-flag census,
  compiled with `rustc` against the release rlibs). All were built in the session scratchpad, and no repo
  file was created or edited apart from this report.
- **`.psc` corpus:** the only one on disk is still the 29 F4SE sources plus the 4 `docs/r5` fixtures. No
  vanilla `Scripts.zip` source dump is installed. Finding 01's scale is therefore taken from the `.pex`
  Conditional user-flag census, not from parsing `.psc` files.
- **Skill-text rot, for `/audit-sync`:** this was also noted on 10-05. SKILL.md Dim 2 still says
  `replace_constant_id`'s `debug_assert!` is debug-only, but #2666 made it a hard `ExpressionRebuildFailed`
  (`lift.rs:441-455`).

## Cross-audit routing

- `/audit-scripting`: `papyrus_provider/lower_program.rs:1088` (`expression_mentions_provider`) has a
  `_ => false` wildcard. A `.psc`-sourced `Expr::Is` that wraps a provider call is therefore not
  scanned. In production the provider is fed from `.pex`, where `is` still arrives as `Cast`, so
  today this is reachable only in tests. It becomes live once D3-01 is fixed. Please confirm the
  wildcard sites when D3-01 lands.
- `/audit-tooling`: the debug-console `parse_expr` now rejects trailing garbage (#5322). That change is
  correct and has no tooling finding.

## Summary

| Severity | NEW | Already tracked |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 3 (#5325, #5328, #4113) |
| LOW | 3 | 3 (#5331, #5333, #4115) |

Next step: `/audit-publish docs/audits/AUDIT_PAPYRUS_2026-10-08.md` (domain label `scripting`).
