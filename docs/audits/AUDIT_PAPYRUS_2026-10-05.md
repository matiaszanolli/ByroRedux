# Papyrus Frontends Audit — 2026-10-05

This is a delta-scoped pass over the two Papyrus frontends: `crates/pex` (decode plus the 5-phase
decompiler) and `crates/papyrus` (lexer plus Pratt parser). It ran solo as part of
`/audit-suite --preset comprehensive`, with no sub-agent fan-out. Each dimension was written to
`/tmp/audit/papyrus/dim_N.md` before the next one started.

**HEAD**: `a2c24b16e` · **Baseline**: [AUDIT_PAPYRUS_2026-09-29.md](AUDIT_PAPYRUS_2026-09-29.md)
(HEAD `9fcfdc3fc`) · **Audited**: Dim 3 (Boolean/Control-Flow/Lower; the `event_names.rs` delta plus a
corpus re-measure) and Dim 4 (`.psc` Lexer & Pratt Parser, in full, with probes) ·
**Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode) and Dim 2 (Decompiler CFG & Lift).
`git log 9fcfdc3fc..HEAD` returns zero commits on `reader/opcode/model/lib/call_sites.rs` and on
`decompile/{cfg,lift,node}.rs`. For those two, I re-ran the named guards and spot-read the key invariants.

## Delta since baseline: 5 commits

| Commit | Fixes | Dim | Verdict |
|---|---|---|---|
| `a75c7dc33` (09-29) | #5019: a `;` line comment ends at a bare CR | 4 | **SOUND** |
| `3b7ffa670` (09-29) | #5021: a keyword's mandatory operand must share the keyword's line | 4 | **SOUND** for all 13 sites. One sibling site is left (PEX-D4-2026-10-05-04) |
| `0611cd116` (10-04) | #4471: adds the 12 engine events that Champollion's `EVENT_NAMES` omits | 3 | **SOUND**. An independent census found no missing engine event |
| `4ad847a81` (10-01) | clippy: one blank line in a `script.rs` test doc | 4 | test-only, no effect |
| `097f51b48` (10-05) | #5227: the `script.rs` test-family note becomes a plain comment | 4 | test-only, no effect |

## Build, test and corpus state: CLEAN

```
$ cargo test -p byroredux-pex -p byroredux-papyrus
   papyrus 123 unit (+5: #5019 ×1, #5021 ×4) + 4 round-trip · pex 75 unit (+1: #4471 census) + 1 doc-test · 0 failed
$ cargo test -p byroredux-pex --test r5_fidelity -- --ignored        (Skyrim SE data)
   da10_main_door_decompiles_to_the_r5_reference_shape ... ok
$ pex_corpus_smoke "Skyrim - Misc.bsa" "Fallout4 - Misc.ba2"   → parse 21,901/21,901 · decompile 21,900/21,901, 0 panic
$ pex_corpus_smoke "Starfield - Misc.ba2"                      → parse  4,740/4,740  · decompile  4,740/4,740,  0 panic
```

## Executive Summary

**5 new findings: 0 CRITICAL, 0 HIGH, 3 MEDIUM, 2 LOW** (one of the LOWs is a doc-rot entry in Dim 3).
The four code findings are all in the `.psc` parser (Dim 4), and none comes from this window's commits.
All four are older parser behaviour that the earlier newline-glue passes never probed. I found them by
running the parser on idiomatic **FO4-era** source: the CK-wiki grammar forms, plus the 29 F4SE
`.psc` files under `Fallout 4/Data/Scripts/Source`. Before this pass, every probe was Skyrim-shaped.
On the `.pex` side, the #4471 change is sound and every guard is green.

- **PEX-D4-2026-10-05-01 (MEDIUM): `expect_eol` and `parse_expr` never require the statement or input
  to end.** Whatever the parser stops short on becomes a **silent** extra statement. The FO4 `is`
  operator has no token, so `If f is Actor` parses as `If f` with the body `is Actor;`, which is a
  `VarDecl` of type `is`. The parser reports 0 errors. In the console, `parse_expr("x is Actor")`
  returns `Ok(x)`.
- **PEX-D4-2026-10-05-02 (MEDIUM): the parser collects doc comments before a declaration, but Papyrus
  puts them on the line *after* it.** A function or event that has a body and a following `{doc}`
  line is dropped. Its body lines are then re-parsed as script items, so a local declaration is
  hoisted into script scope. Two of the 29 on-disk F4SE scripts fail this way.
- **PEX-D4-2026-10-05-03 (MEDIUM): several FO4+ grammar forms are rejected or truncated.** Namespaced
  `ScriptName`, `Extends` and `Import` keep only the first segment, so the parent becomes `"DLC01"`.
  This contradicts `papyrus-parser.md`. Remote-event declarations (`Event Actor.OnDeath(…)`) are
  dropped, and `new Point` struct creation fails. Three of the 29 on-disk F4SE scripts fail this way.
- **PEX-D4-2026-10-05-04 (LOW): `New` still skips newlines.** It is the one keyword-operand site that
  #5021 did not convert: `New` ⏎ `Int[5]` glues with 0 errors.

### Untrusted-input robustness verdict: still MET (NO panic, OOB, OOM or stack overflow)

- **`.pex`:** no reader or decompiler code changed. `take(n)` is still the only bounds gate
  (`reader.rs:80-88`). The `MAX_OPCODE` compile-time assert is still at `opcode.rs:74`, and the
  `transmute` still sits behind `byte >= MAX_OPCODE` (`opcode.rs:137-142`). The exhaustive-prefix,
  hostile-vararg and string-budget guards all pass, and all 26,641 real files parse.
- **`.psc`:** none of the four findings can crash, overflow the stack or exhaust memory. #5021 added
  early-return error paths in `parse_if_stmt` and `parse_while_stmt`. They cannot leak `stmt_depth`,
  because depth is charged in an inc/call/dec wrapper (`stmt.rs:65-73`). The depth-cap guards all
  pass. What the four findings *do* produce is a wrong AST: finding 01 with zero errors, findings
  02-04 with errors (02 and 03) or without them (04). These are the only production paths that
  reach them:
  - the offline `extender_preflight` `scan_source`, which counts the errors but still analyses the
    wrong AST
  - the localhost debug console, through `parse_expr` (`crates/debug-server/src/evaluator.rs:596`)

  No runtime or game-load path parses `.psc`.

### Decompile-rate verdict: re-measured, exactly reproduced

| Game | Decompiled | vs 09-29 |
|---|---|---|
| Skyrim SE | 14,026 / 14,026 | unchanged |
| FO4 | 7,874 / 7,875 | unchanged. The one failure is `stimboxscript.pex` `CheckTriggeringObjectAndDoOnceStatus`, the known `\|\|` fail-closed case (#1732) |
| Starfield | 4,740 / 4,740 | unchanged |
| **Total** | **26,640 / 26,641** | 0 panics. The shape-mismatch counter prints only when it is non-zero, and it did not print |

What the rate proves is **robustness**: every vanilla script decompiles without an error or a panic.
It is not **fidelity**. That evidence is the `.psc`-vs-`.pex` gate owned by `/audit-scripting`
Dim 1. Its `r5_fidelity` DA10 leg passes, as re-run above. This pass also shows the gate covers
only Skyrim-shaped source. FO4-only grammar (findings 01 and 03) has no `.psc` side that could be
compared against the `.pex` side.

## Verification of the delta

### `a75c7dc33` (#5019): the CR-only comment fix

`token.rs:13` now ends a line comment at `remainder.find(['\r', '\n'])` and leaves the terminator to
the `Newline` regex, so CRLF still lexes as one `Newline`. I ran these probes through a scratch crate
that calls `parse_script`:

| Input (CR-only unless stated) | items | errors |
|---|---|---|
| Header comment, then `F` and `G` | 2 | 0 |
| `Int x = 5 ; trailing`, then `F` | 2 (`Var x`, `Fn F`) | 0 |
| Comment inside `F`'s body, then `G` | 2 | 0 |
| CRLF header comment | 1 | 0 |

The block-comment path (`;/ … /;`) has no line semantics and is unaffected.

### `3b7ffa670` (#5021): mandatory operands

`expect_same_line` (`parser/mod.rs:207`) rejects the raw token at `pos` when it is a `Newline`.
`expect_raw` and `expect_ident_raw` wrap it. I re-checked all 13 converted sites (`script.rs:100,104,
167,194,288,320,338,399,584,588,666,708`, `stmt.rs:134,142,173,250`). The remaining
newline-skipping `self.expect(` sites are the following:
- Closing keywords and brackets: `stmt.rs:157,177`, `script.rs:298,325,361`, `expr.rs:200,240,268,294`.
- Leading keywords after the dispatcher has committed: `script.rs:99,287,319,398,586,665,707`.

The one exception is **`expr.rs:237-238` (`New`)**, which is finding 04. The `stmt.rs:214` and
`script.rs:222` variable-name sites keep `expect_ident`, but both sit behind a `peek_raw()` = `Ident`
check, so they are safe.

### `0611cd116` (#4471): EVENT_NAMES superset

All 12 names are in sorted position. `list_is_sorted_for_binary_search` (strict `<`, so it also
checks dedup) and `the_census_engine_events_classify_as_events` both pass. I ran an **independent
census** over the full 26,641-file corpus. A scratch probe ran `decompile_script` and collected every
`on`-prefixed item that lowered to `Function`. **21 distinct names remain demoted, and none is an
engine event.** They fall into two groups:
- **User-defined helpers in vanilla script hierarchies:** `Critter.OnCritterGoal*`,
  `RQScript.OnQuestGiverSet` and `OnAliasChangedSpecific`,
  `WorkshopAddLocationsScript.OnQuestInitCustom`, `onghostactivation`,
  `ongalbankarchivestriggerentered`, and the PressurePlate-derived `onenter` and `onleave`.
- **Vanilla typos or near-misses of real events:** `oncelldetatch`, `onequip`/`onunequip` (the real
  events are `OnEquipped`/`OnUnequipped`), `ondead`, `onload3d`, `oncombatstatechange`,
  `oneffectend`, `onmagiceffectstart`, `oncontainerchanges`, `onadd`.

`lower.rs:312` (`build_handler`) is still the only consumer.

## Findings

I deduplicated against `/tmp/audit/issues.json` (97 open issues) and against closed-issue searches for
"expect_eol", "papyrus doc comment", "papyrus remote event", "psc namespace" and "papyrus is operator".
None matched; the nearest is closed #4477, which covers the `.pex` side of `is`. I also checked every
`AUDIT_*_2026-10-05.md` sibling. The only one that mentions `crates/pex` is `AUDIT_SAFETY`, and it
has no finding there. All `AUDIT_PAPYRUS_*` and `AUDIT_SCRIPTING_*` reports were checked as well.
The 2026-09-06 scripting report mentions an "unreachable `expect_eol` arm" only as a tech-debt
aside. It never names the missing end-of-statement check.

---

### MEDIUM

#### PEX-D4-2026-10-05-01: `expect_eol` / `parse_expr` never enforce end-of-statement or end-of-input, so the FO4 `is` operator silently becomes `If f` plus a `VarDecl` named `Actor`

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:241-262` (`expect_eol`, every non-`Newline` arm
  returns `Ok(())`, commented "be lenient — many Papyrus scripts don't have strict EOL") ·
  `crates/papyrus/src/lib.rs:18-43` (`parse_expr` returns `Ok` without checking that input is
  exhausted) · `crates/papyrus/src/token.rs` (no `Is` token; only `As` at `:213`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The statement terminator is not enforced. When the Pratt loop stops at a token
  it does not recognise, the statement ends there without an error. The rest of the line is then
  parsed as a new statement. `parse_expr` likewise returns its longest prefix and ignores whatever
  follows. Valid source normally hides this. FO4's `is` type-check operator exposes it: `is` has no
  token and no AST node, so it lexes as an identifier and starts a second "statement". (On the `.pex`
  side, #4477 lowers it to a `Cast`.)
- **Evidence**: These are probe results from a scratch crate with a path dependency on
  `byroredux-papyrus`:

  | Input | Result | errors |
  |---|---|---|
  | `If f is Actor` ⏎ `EndIf` (inside a function) | `If { condition: f, body: [VarDecl { ty: Object("is"), name: "Actor" }] }` | **0** |
  | `parse_expr("x is Actor")` | `Ok(x)`, span 0..1 | n/a |
  | `parse_expr("a > 5 b")` | `Ok(a > 5)` | n/a |
  | `parse_expr("Game.GetPlayer() garbage tokens")` | `Ok(Game.GetPlayer())` | n/a |

  `If f is Actor` is documented FO4 syntax (falloutck wiki, Operator Reference).
- **Impact**: This is a silent wrong AST. The condition changes from a type check to a
  truthiness check, and a fabricated declaration is injected, with zero errors, so a strict-fail
  caller that checks `result.1.is_empty()` accepts it. A recognizer could match the wrong condition.
  The same root cause makes the debug console evaluate a prefix of a mistyped expression without
  complaint. It is reachable through the same two paths as the #4321/#4472/#4763 class: the offline
  `extender_preflight` `scan_source` and the debug-console `parse_expr`. That reachability, and
  precedent (#4321, #4472 and #4763 were all MEDIUM), are why this is MEDIUM rather than HIGH.
- **Related**: #4477 (the `.pex` side of `is`), #4321, #4472, #4763 (silent-glue class), finding 03
- **Suggested Fix**: Make `expect_eol` report an error when the next raw token is neither `Newline`,
  EOF, nor a `DocComment`. Then recover to the next line. Make `parse_expr` reject tokens left after
  the expression. Separately, lex `Is` and add an AST form for it (or the same `Cast` the `.pex` side
  uses), so the two frontends agree.

#### PEX-D4-2026-10-05-02: Doc comments are only collected *before* a declaration; Papyrus puts them on the line *after*, so a bodied function or event with a doc comment is dropped and its body is hoisted into script scope

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/script.rs:286` (`parse_function`), `:318` (`parse_event`),
  `:397` (`parse_property`). Each calls `skip_newlines_collect_doc()` *before* its keyword. Nothing
  consumes a `DocComment` after the header line, so `parse_block` reaches it as the first statement.
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The falloutck wiki (*Script File Structure*, §Documentation Comments) says doc
  comments "can only appear on the line following a script header, property definition, group
  definition, struct member definition, or function definition". The `.pex` `doc_string` fields are
  written from that position. The parser instead attaches the `{…}` that comes *before* an item.
  The result depends on the item:
  - **Native function or property:** the doc after the header is lost, and `doc_comment` is `None`.
  - **Function or event with a body:** the `DocComment` becomes the first body statement, which
    raises "expected expression, found doc comment". The item is dropped, and script-level
    recovery re-parses its body lines as top-level items.
- **Evidence**:

  | Input | Result | errors |
  |---|---|---|
  | `Function F()` ⏎ `{doc for F}` ⏎ `Int x = 1` ⏎ `EndFunction` ⏎ `Function G()…` | items `[Var x, Fn G]`: F is gone, and its local `x` became a **script variable** | 2 |
  | `Event OnInit()` ⏎ `{doc}` ⏎ `EndEvent` | items `[]` | 2 |
  | `Function F() native` ⏎ `{doc for F}` | `F.doc_comment = None` | 0 |

  On the 29 F4SE `.psc` files on disk, `Location.psc` fails with 11 errors and
  `ObjectReference.psc` with 31 (the first is `bool Function IsSameLocation(...)` ⏎ `{Returns true
  if …}` at `Location.psc:39-40`).
- **Impact**: Bodied functions that follow the documented convention vanish from the AST, and their
  locals pollute script scope. `extender_preflight` counts the file as an input error but still runs
  `analyze_source_compatibility` on the damaged AST. Extender calls inside a dropped function are
  then missed, in exactly the documented SKSE/F4SE sources the tool exists to scan. Errors are
  reported, so this is not silent, and it is MEDIUM.
- **Related**: AUDIT_SCRIPTING_2026-08-12 noted that the `r5_round_trip` rumble fixture only passed
  because of trailing same-line `{doc}` comments. This is the general case.
- **Suggested Fix**: After each header's `expect_eol()`, accept an optional `DocComment` (with the
  newlines around it) and attach it to that item: script header, function, event, property, group,
  and struct member. Keep the leading-doc path only if a corpus shows it is used. Add the
  `Location.psc:39-41` shape as a test.

#### PEX-D4-2026-10-05-03: FO4+ grammar forms are rejected or truncated — namespaced `ScriptName`/`Extends`/`Import`, remote-event declarations, and `new Struct`

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**:
  - `crates/papyrus/src/parser/script.rs:100` (script name), `:104` (`Extends` target) and `:194`
    (`Import` target). All three use `expect_ident_raw`, not `parse_qualified_ident`.
  - `script.rs:320` + `:338` (`Event` name, then `(` is expected immediately).
  - `crates/papyrus/src/parser/expr.rs:235-248` (`parse_new_expr` always requires `[size]`).
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `docs/engine/papyrus-parser.md:115-116` says namespaces are "parsed by
  `parse_qualified_ident`". Its grammar summary gives `import ::= "Import" qualified_ident`. In the
  code, only *type* positions and expression primaries go through `parse_qualified_ident`. The
  header, `Extends` and `Import` read one bare identifier and leave `:rest` as a stray token. Two other
  FO4 forms from the falloutck wiki are also unparseable:
  - remote-event handlers, `Event <Type>.<EventName>(<Type> akSender, …)`
  - struct creation without a size, `Point myPoint = new Point`. The `.pex` side lowers
    `StructCreate` to `New` with size 0, so the AST can already represent it.
- **Evidence**:

  | Input | Result | errors |
  |---|---|---|
  | `ScriptName DLC01:Foo Extends ObjectReference` | `name = "DLC01"`, `parent = None` | 1 |
  | `ScriptName Foo Extends DLC01:Base` | **`parent = Some("DLC01")`** | 1 |
  | `Import DLC01:Utils` | `Import("DLC01")` | 1 |
  | `Event Actor.OnDeath(Actor akSender, Var[] akArgs)` … `EndEvent` | event dropped ("expected (, found '.'") | 2 |
  | `S v = new S` | function dropped ("expected '[' after new type") | 1 |

  On disk, `Weapon.psc`, `Actor.psc` and `UI.psc` (F4SE) fail on `new InstanceData:Owner` and
  `new MenuData`. Together with finding 02, that makes **5 of the 29** on-disk FO4 `.psc` files that
  fail to parse cleanly.
- **Impact**: Any FO4, FO76 or Starfield `.psc` that uses namespaces, remote events or struct
  creation parses with errors, and with a wrong `name`/`parent`. All three forms are idiomatic in
  FO4+ content. Errors are reported, so this is not silent, and it is MEDIUM. The parent truncation
  (`"DLC01"`) is a wrong value, though, not a missing one. The decompiler emits remote events as
  `Event OnDeath` (it strips `::remote_`), so the two frontends cannot round-trip FO4 remote handlers.
  Reachability is the same as finding 01.
- **Related**: findings 01 and 02. This is the parser-side counterpart of the FO4 `.pex` coverage in
  `/audit-scripting` Dim 1.
- **Suggested Fix**: Use `parse_qualified_ident` for the script name, the `Extends` target and the
  `Import` target, keeping the #5021 same-line check before each. Accept an optional `Ident '.'`
  prefix on event names (store the sender type, or mangle it the same way the `.pex` `::remote_`
  form is). Make `[size]` optional in `parse_new_expr` when the type is an object (struct) type. Add
  an FO4 round-trip fixture.

---

### LOW

#### PEX-D4-2026-10-05-04: `parse_new_expr` still skips newlines — `New` ⏎ `Int[5]` and `New Int` ⏎ `[5]` glue with 0 errors (#5021 sibling)

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/expr.rs:237` (`parse_base_type`, which calls
  `skip_newlines`) and `:238` (`expect(&Token::LBracket, …)`)
- **Status**: NEW (sibling of closed #5021)
- **Untrusted-Input**: Yes
- **Description**: #5021 moved every keyword with a *mandatory operand* onto the raw stream.
  `New` is a prefix keyword with a mandatory type and `[`, but it still goes through the
  newline-skipping helpers.
- **Evidence**: These probes produce `Int[] a = New Int[5]` with 0 errors:
  - `Int[] a = New Int` ⏎ `[5]`
  - `Int[] a = New` ⏎ `Int[5]`
- **Impact**: The parser accepts source that the reference compiler rejects. As with #5021, the
  glued form is what the author evidently meant, so this is LOW.
- **Suggested Fix**: Call `expect_same_line` before `parse_base_type` and use `expect_raw` for the `[`.
  Add both probes to `a_mandatory_operand_on_the_next_line_is_an_error`.

#### PEX-D3-2026-10-05-01 (doc rot): `near-term-action-plan.md` lists the closed PEX sweep #4471–#4479 as open backlog

- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Location**: `docs/engine/near-term-action-plan.md:108`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The Wave-3 table row "PEX / scripting sweep | #4474–#4479 (low) + #4471–#4473
  (medium)" says "verified open 2026-09-20". All nine issues are now CLOSED (`gh issue view`; #4471
  closed by `0611cd116` this window).
- **Suggested Fix**: Delete the row, or mark it done with the closing commits.

---

## Carried-open findings (not re-filed, still present)

- **#4113 (MEDIUM): two independent depth caps.** `MAX_EXPR_DEPTH` (256; `lift.rs:380` aliases the
  parser constant) and `MAX_REBUILD_DEPTH` (1024, `control_flow.rs:44`) are still independent and
  compose additively.
- **#4115 (LOW): stale module doc.** `control_flow.rs:28` still says "advanced past".
- **#4471 is closed.** The fix is verified sound above.

## Decompiler Soundness Matrix

This lists the delta from baseline only. The full matrix is in AUDIT_PAPYRUS_2026-09-19.md.

| Pass | Bounds-safe | Terminates | Total | Fidelity-tested | Change this window |
|---|---|---|---|---|---|
| Reader (`reader.rs`/`opcode.rs`/`lib.rs`/`call_sites.rs`) | Yes | Yes | Yes | Yes: 26,641 parse, 51-row pin | none |
| CFG (`cfg.rs`) | Yes | Yes | Yes | Yes: JmpF/JmpT pins | none |
| Lift + copy-prop (`lift.rs`) | Yes: per-fold depth check (`:469`) | Yes: linear chain | Yes: a >1 match is a hard `ExpressionRebuildFailed` (`:440-455`) | Yes | none |
| Boolean (`boolean.rs`) | Yes | Yes: `RecursionLimit` | Yes | corpus + gate | none |
| Control-flow (`control_flow.rs`) | Yes | Yes | Yes: fails closed (#1732) | corpus + gate | none. #4115 open |
| Lower + assembly (`lower.rs`, `event_names.rs`) | Yes | Yes | Yes | Yes: census re-run, 0 engine events missing | #4471 **fixed** |
| `.psc` lexer | Yes | Yes | Yes | Yes for CR/CRLF (#5019 fixed). **No `Is` token (01)** | #5019 |
| `.psc` parser | Yes: depth caps pinned | Yes | Yes | **Partial.** Skyrim-shaped only: EOL not enforced (01), doc-comment placement (02), FO4 forms (03), `New` (04) | #5021 |

**The three Champollion departures are unchanged.** `boolean.rs` has two: no debug-line guard, and a
termination guard. `lower.rs` has one: `is` becomes an object-typed `Cast` (#4477). The 09-19
adjudication stands: they are benign. The corpus rate is exactly reproduced, and none of those files
has a commit. Note on the `is` departure: the `.psc` side has no `is` at all (finding 01). That
departure therefore cannot be cross-checked through the fidelity gate until the parser lexes `Is`.

## Coverage notes

- **Probes.** The probes (`pexprobe`: census, `psc`, `dbg`, `expr`) were built in the session
  scratchpad with their own `CARGO_TARGET_DIR`. No repo files were created or edited apart from
  this report.
- **FO4 `.psc` corpus.** The only FO4 `.psc` corpus on disk is the 29 F4SE base-script sources.
  No vanilla Skyrim or FO4 `Scripts.zip` source dump is installed, so the FO4-grammar failure rate
  across a full vanilla corpus is not measured. Findings 01-03 rest on the CK-wiki grammar plus
  those 29 files.
- **FO76 dialect.** It still has zero corpus files, unchanged from baseline.
- **Skill-text rot, for `/audit-sync`.** SKILL.md Dim 2 still says that `replace_constant_id`'s
  `debug_assert!(slot.is_none())` is debug-only and that a release >1-match would silently drop the
  producer. `lift.rs:440-455` turned that into a hard `ExpressionRebuildFailed` (#2666).

## Cross-audit routing

- `/audit-scripting` owns recognizer and runtime consumption of these ASTs, the `translate_pex`
  clean-`None` contract, and the panic net (Dims 1 and 5). It also owns the extender-preflight
  consumer that findings 02 and 03 under-report through (Dim 7).
- `/audit-tooling` owns the debug-console `parse_expr` path (`evaluator.rs:596`) that finding 01
  reaches.

## Findings count

**5 new: 0 CRITICAL, 0 HIGH, 3 MEDIUM, 2 LOW** (4 parser findings plus 1 doc-rot entry).
Carried open: #4113, #4115. Fixes verified sound: #5019, #5021, #4471.

Next step: `/audit-publish docs/audits/AUDIT_PAPYRUS_2026-10-05.md` (domain label `scripting`).
