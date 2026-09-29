# Papyrus Frontends Audit — 2026-09-29

Delta-scoped pass over the two Papyrus frontends (`crates/pex` decode + 5-phase decompiler,
`crates/papyrus` lexer + Pratt parser), run solo as part of `/audit-suite --preset comprehensive`
(no sub-agent fan-out; each dimension written to `/tmp/audit/papyrus/dim_N.md` before the next).

**HEAD**: `9fcfdc3fc` · **Baseline**: [AUDIT_PAPYRUS_2026-09-22.md](AUDIT_PAPYRUS_2026-09-22.md)
(HEAD `ee6d3fb39`) · **Audited**: Dim 4 (`.psc` Lexer & Pratt Parser), in full ·
**Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader), Dim 2 (Decompiler CFG & Lift),
Dim 3 (Decompiler Boolean/Control-Flow/Lower + corpus instruments). `git log ee6d3fb39..HEAD --
crates/pex` returns zero commits. The named guards were re-run and key invariants spot-read.

## Delta since baseline: 1 commit

| Commit | Fixes | Dimension | Verdict |
|---|---|---|---|
| `20231b930` (2026-09-29) | #4763: seven more newline-skipping `peek()` sites moved to `peek_raw()` | 4 | **SOUND** (see below) |

## Build, test and corpus state: CLEAN

```
$ cargo test -p byroredux-pex -p byroredux-papyrus
   papyrus 118 unit + 4 round-trip · pex 74 unit + 1 doc-test · 0 failed
   (papyrus up from 110 at baseline: #4763's 7 regression tests + 1 single-line guard)

$ cargo build --release -p byroredux-pex --example pex_corpus_smoke
$ pex_corpus_smoke "Skyrim - Misc.bsa" "Fallout4 - Misc.ba2"   → 21,900 / 21,901, 0 panics
$ pex_corpus_smoke "Starfield - Misc.ba2"                      →  4,740 /  4,740, 0 panics
```

## Executive Summary

**2 new findings: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.** Both are in the `.psc` lexer and parser (Dim 4).
The `.pex` side is unchanged and its guards are green. The #4763 fix is sound for all seven of
its sites.

- **PEX-D4-2026-09-29-01 (MEDIUM)** is left over from the closed #4479. That fix made a lone CR a line
  terminator, but `skip_line_comment` still ends a `;` comment only at `\n`. In a CR-only file,
  the first line comment swallows the rest of the file with zero diagnostics. As a result, #4479's
  own fix does nothing for any real CR-only script, because real scripts contain comments.
- **PEX-D4-2026-09-29-02 (LOW)** is the `expect()`/`expect_ident()` newline-skipping class. #4763's
  commit message names it as a known residual, but no open issue tracks it. A construct keyword
  alone on a line pulls its mandatory operand up from the next line, with zero errors.

### Untrusted-input robustness verdict: still MET (NO panic, OOB, OOM or stack overflow)

- **`.pex`:** no code change. `take(n)` is still the only bounds gate (`reader.rs:86`). The
  compile-time `MAX_OPCODE` assert (`opcode.rs:74`) and the string budget are still in place. The
  exhaustive-prefix, hostile-vararg and budget guards all pass, and 26,641 real files parse with
  0 failures.
- **`.psc`:** neither new finding can crash, overflow the stack or exhaust memory. Both produce a
  wrong or over-accepting AST with no diagnostic. They are reachable through the same paths as the
  #4472/#4763 class: `extender_preflight`'s offline `scan_source`, and `parse_expr` in the
  localhost debug console.

### Decompile-rate verdict: re-measured, exactly reproduced

| Game | Decompiled | vs 09-19 |
|---|---|---|
| Skyrim SE | 14,026 / 14,026 | unchanged |
| FO4 | 7,874 / 7,875 | unchanged. The one failure is `stimboxscript.pex` `CheckTriggeringObjectAndDoOnceStatus`, the known `\|\|` fail-closed case (#1732) |
| Starfield | 4,740 / 4,740 | unchanged |
| **Total** | **26,640 / 26,641** | 0 panics, 0 shape mismatches |

The harness wraps `decompile_script` in `catch_unwind` and counts both `Err` and panic as
failures (`pex_corpus_smoke.rs:179-215`). The shape-mismatch counter prints only when it is non-zero,
and it did not print.

What the rate proves: **robustness** (every vanilla script decompiles without error or panic).
What it does not prove: **fidelity**. That evidence is the `.psc`-vs-`.pex` gate that
`/audit-scripting` Dim 1 owns.

## Verification of `20231b930` (#4763)

All seven sites now use `peek_raw()`:

| Site | Construct |
|---|---|
| `script.rs:102` | `Extends` |
| `script.rs:261` | variable flags |
| `script.rs:370` | function flags |
| `script.rs:401` | property initializer |
| `script.rs:707` | group flags |
| `stmt.rs:212` | `parse_var_decl_or_expr` initializer |
| `stmt.rs:248` | `parse_variable_body` initializer (local keyword-typed declarations and `Struct` members) |

`advance()` after a `peek_raw()` match is correct, because no newline can sit between the two.

Each site has an `*_on_the_next_line_*` test that asserts two things: the glued shape is absent, and
`errors > 0`. `single_line_forms_of_the_4763_sites_still_parse` pins the happy path.

I re-classified the remaining `peek()`/`check()`/`eat()` sites independently, and my result agrees with
the commit's sibling sweep:
- **Paren- or bracket-bounded:** `expr.rs:284,288`, `mod.rs:388`, `script.rs:340,347,354`.
- **Behind an explicit `skip_newlines()`:** `script.rs:524,597,671,733` and `stmt.rs:138,146,306`.
- **Leading token after the dispatcher has committed:** `script.rs:576`.

## Findings

Deduplicated against `/tmp/audit/issues.json` (open issues), a closed-issue search (#4479, #4763,
"expect newline glue", "line comment CR") and every `docs/audits/*_2026-09-29.md` sibling. No sibling
report touches `crates/papyrus`. `AUDIT_SAFETY_2026-09-29.md` lists `crates/pex` as "no commits" and
has no finding there.

---

### MEDIUM

#### PEX-D4-2026-09-29-01: A `;` line comment in a CR-only file swallows the rest of the file silently (#4479's CR fix left `skip_line_comment` on `\n` only)

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/token.rs:5-14` (`skip_line_comment`)
- **Status**: NEW (left over from closed #4479, PEX-D4-2026-09-19-03)
- **Untrusted-Input**: Yes
- **Description**: #4479 made `\r\n|\r|\n` the `Newline` token, so a lone CR terminates a line. The
  line-comment callback still finds the end of the comment with `remainder.find('\n')`. A CR-only
  file has no `\n`, so the first `;` comment bumps the lexer to EOF, and every later line is
  discarded. CRLF files are unaffected: the `\r` is absorbed into the comment and `\n` still lexes as
  `Newline`.
- **Evidence**: Probe results from a scratch crate with a path dependency on `byroredux-papyrus`,
  calling `parse_script`:

  | Input | items | errors |
  |---|---|---|
  | `ScriptName Foo⏎; header comment⏎Function F()⏎EndFunction⏎Function G()⏎EndFunction⏎`, LF | 2 | 0 |
  | Same input, CRLF | 2 | 0 |
  | Same input, **CR-only** | **0** | **0** |
  | CR-only, no comment | 2 | 0 |
  | CR-only `Int x = 5 ; trailing⏎Function F()…` | 1 (F dropped) | 0 |
  | CR-only comment inside a function body | 0 | 1 |

  The #4479 regression test `bare_cr_is_a_newline_and_crlf_is_one_newline` has no comment in its
  fixture, which is why the suite stays green.
- **Impact**: On a CR-only `.psc`, every function, event and property after the first comment
  disappears from the AST. Recovery reports nothing when the comment sits between top-level items.
  #4479 rated the gluing variant of this input class LOW. This variant drops whole items instead of
  gluing statements, and it defeats #4479's fix on essentially every real CR-only script.
  Reachability is the same bracket as #4472/#4763: the offline `extender_preflight` `scan_source` and
  the debug-console `parse_expr`. No runtime or game-load path parses `.psc`.
- **Related**: #4479 (closed, incomplete), #4763
- **Suggested Fix**: End the comment at the first `\r` or `\n` (`remainder.find(['\r', '\n'])`), leaving
  the terminator for the `Newline` rule. Add a CR-only-plus-comment case to the #4479 test.

---

### LOW

#### PEX-D4-2026-09-29-02: `expect()` / `expect_ident()` skip newlines, so a keyword alone on a line pulls its mandatory operand up from the next line with zero errors

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:179-180` (`expect`) and `mod.rs:239-240`
  (`expect_ident`). Call sites include:
  - `script.rs:99-100` (ScriptName) and `:104` (Extends target)
  - `:194` (Import), `:287-288` (Function), `:319-320` (Event), `:398-399` (Property)
  - `:582-583` (`Auto` ⏎ `State`), `:661` (Struct), `:703` (Group)
  - `stmt.rs:245` (local keyword-typed declaration)
  - If/While condition start, through the Pratt prefix `advance()`.

  Also `docs/engine/papyrus-parser.md:243`.
- **Status**: NEW. #4763's commit message names this as a "broader class than this issue". No open
  or closed issue tracks it, and the skill lists it only as a known residual.
- **Untrusted-Input**: Yes
- **Description**: Since #4321, #4472 and #4763, every construct-*continuation* decision reads the raw
  token stream. But `expect()` and `expect_ident()` still call `skip_newlines()` before matching a
  construct's *mandatory* next token. So a keyword alone on a line still reaches across the newline.
- **Evidence**: Probe results. Every row returns errors=0:

  | Input | Parsed as |
  |---|---|
  | `Auto`⏎`State S` | `State{is_auto: true}` |
  | `ScriptName`⏎`Foo` | name `Foo` |
  | `ScriptName Foo Extends`⏎`Bar` | `parent = Some("Bar")` |
  | `Function`⏎`F()`, `Event`⏎`OnInit()`, `Int Property`⏎`P Auto`, `Import`⏎`Debug`, `Group`⏎`G`, `Struct`⏎`S` | items parsed normally |
  | local `Int`⏎`x = 5` | `VarDecl` |
  | `If`⏎`true`, `While`⏎`true` | conditions glued |

  At top level, `Int`⏎`x = 5` does raise 2 errors, which is inconsistent with the local path.
- **Impact**: The parser accepts source that the reference compiler rejects, because Papyrus treats
  EOL as significant. Unlike the #4472/#4763 class, the glued AST is the author's evident intent: a
  mandatory operand is pulled forward and nothing is attributed to the wrong construct. That is why
  this is LOW and not MEDIUM. The docs are also stale: `papyrus-parser.md:243` still says "#4472 tracks
  the remaining newline-skipping decision sites". Both #4472 and #4763 are closed, and this remaining
  class is not documented.
- **Related**: #4763, #4472, #4321 (same line-terminator contract)
- **Suggested Fix**: Add `expect_raw`/`expect_ident_raw` for operands that must share the line with their
  keyword: state, function, event, property, script, import, group and struct names, the `Extends`
  target, and the If/While condition start. Add one probe-style test per site, following the #4763 block
  in `script.rs`. Update the Pitfalls bullet.

---

## Carried-open findings (not re-filed, unchanged)

- **#4471** (MEDIUM): `EVENT_NAMES` is missing real engine events, so 20 vanilla handlers demote to
  `Function`. `event_names.rs` has 0 commits.
- **#4113** (MEDIUM): `MAX_EXPR_DEPTH` (256, `lift.rs:380` aliases the parser constant) and
  `MAX_REBUILD_DEPTH` (1024, `control_flow.rs:44`) are still independent, additively composing caps.
- **#4115** (LOW): the `control_flow.rs:28` module doc still says "advanced past". The issue is open, but
  it is absent from the pre-fetched list because of the list limit (confirmed with `gh issue view`).

## Decompiler Soundness Matrix

Delta from baseline only; the full matrix is in AUDIT_PAPYRUS_2026-09-19.md.

| Pass | Bounds-safe | Terminates | Total | Fidelity-tested | Change this window |
|---|---|---|---|---|---|
| Reader (`reader.rs`/`opcode.rs`/`lib.rs`/`call_sites.rs`) | Yes | Yes | Yes | Yes: 26,641 parse, 51-row pin | none |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes: JmpF/JmpT pins | none |
| Lift + copy-prop (`lift.rs`) | Yes: per-fold depth check (`:469`) | Yes: linear chain | Yes | Yes | none |
| Boolean (`boolean.rs`) | Yes | Yes: `RecursionLimit` | Yes | corpus + gate | none |
| Control-flow (`control_flow.rs`) | Yes | Yes | Yes: fails closed (#1732) | corpus + gate | none. #4115 open |
| Lower + assembly (`lower.rs`) | Yes | Yes | Yes | Yes | none |
| `.psc` lexer | Yes | Yes | Yes | Partial: **CR-only comment truncation (PEX-D4-2026-09-29-01)** | none |
| `.psc` parser | Yes: depth caps pinned | Yes | Yes | Partial: #4763 sites fixed. **`expect()` class open (PEX-D4-2026-09-29-02)** | #4763 |

**The three Champollion departures are unchanged.** `boolean.rs` has two: no debug-line guard, and a
termination guard. `lower.rs` has the `is`→`Cast` rewrite (#4477). The adjudication from 09-19 stands:
the departures are benign, because the corpus rate is exactly reproduced and none of these files has
a commit.

## Coverage notes

- The probe crate lived in the session scratchpad with its own target dir. No repo files were created
  or edited apart from this report.
- The FO76 dialect still has zero corpus files; this is unchanged from baseline.

## Cross-audit routing

- Recognizer and runtime consumption of these ASTs, the `translate_pex` clean-`None` contract, and the
  panic net belong to `/audit-scripting` Dims 1 and 5.
- Both new findings reach `extender_preflight`'s `.psc` `scan_source`. `/audit-scripting` Dim 7 owns the
  preflight consumer.

## Findings count

**2 new: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.** Carried open: #4471, #4113, #4115. Fix verified
sound: #4763.

Next step: `/audit-publish docs/audits/AUDIT_PAPYRUS_2026-09-29.md` (domain label `scripting`).
