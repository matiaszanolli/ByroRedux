# Papyrus Frontends Audit — 2026-09-22

Delta-scoped pass over the two Papyrus frontends (`crates/pex` decode + 5-phase decompiler,
`crates/papyrus` lexer + Pratt parser), run solo per this session's instructions (no sub-agent
fan-out) — each dimension analysed in turn, with `/tmp/audit/papyrus/dim_N.md` written before
starting the next.

**HEAD**: `ee6d3fb39` · **Baseline**: [AUDIT_PAPYRUS_2026-09-19.md](AUDIT_PAPYRUS_2026-09-19.md)
(HEAD `41aed20eb`) · **Audited**: Dims 1–4, full (all four had commits since baseline) ·
**Unchanged since baseline (skimmed)**: `crates/pex/src/decompile/{lift,node,boolean,
control_flow,event_names,mod}.rs`, `crates/pex/examples/pex_corpus_smoke.rs`,
`crates/pex/src/{reader,model,call_sites}.rs`, `crates/papyrus/src/{token,lexer,ast,error,lib}.rs`
minus the #4479 lexer diagnostics — each confirmed via `git log 41aed20eb..HEAD` returning zero
hits for the file, then still spot-read to re-verify baseline guards hold.

## Delta since baseline — 9 commits, all 2026-09-21

Every commit landed the day after the baseline report, all closing findings from
AUDIT_PAPYRUS_2026-09-19.md:

| Commit | Fixes | Dimension |
|---|---|---|
| `f525bbcde` | #4474 — fifth wire sample reaches debug/full-property/struct-info/Float paths | 1 |
| `72c687600` | #4475 — compile-time `MAX_OPCODE` transmute-invariant assert | 1 |
| `1460b1eb0` | #4476 — `backward_jmpt_builds_a_loop_edge` misnomer + real JmpT loop pin | 2 |
| `ad96380be` | Cleanup — duplicate `#[test]` left by the #4476 rename (+ unrelated groundcover/objectives fallout) | 2 |
| `7cc8a8313` | #4473 — mark exactly one auto state under a case-insensitive collision | 3 |
| `585478d6e` | #4477 — bookkeep the `is`→`Cast` lowering as the third Champollion departure | 3 |
| `352455afd` | #4472 — six newline-skipping `peek()`/`check()` sites → `check_raw`/`peek_raw` | 4 |
| `ff26d3b69` | #4479 — lex diagnostics for `0x`, exponent shapes, CR-only files | 4 |
| `7e8181a9e` | #4478 — `papyrus-parser.md` doc-rot cleanup + one comment fix | 4 |

## Build & test state — CLEAN

```
$ cargo test -p byroredux-pex -p byroredux-papyrus
   papyrus 110 unit + 4 round-trip · pex 74 unit + 1 doc-test · 0 failed
   (up from 100+4 / 70+1 at baseline — net new tests from #4472/#4474/#4479;
    r5_fidelity still #[ignore]d without --ignored, as documented)
```

`pex_corpus_smoke` **not re-run this pass** — requires `--release` and on-disk game archives,
both outside this session's constraints (no release builds, no smoke scripts). No code in the
harness changed since baseline, so the 09-19 measurement (26,640/26,641) is cited, not
re-verified.

## Executive Summary

**1 new finding (MEDIUM), 0 CRITICAL, 0 HIGH.** All eight baseline findings that received a fix
commit this window (#4472–#4479) were individually re-verified against the diff and are
**correctly fixed** — see the Verification table below. One of those fixes (#4472) turned out to
be incomplete: it corrected six of at least thirteen decision sites sharing the same
newline-skipping-`peek()` defect; a targeted sweep of the rest of the parser (verified by direct
execution, not just reading) found **seven more, unfixed, of the identical class** — filed as
PEX-D4-2026-09-22-01 below. Three prior findings carry open, unchanged: #4471 (EVENT_NAMES
gaps), #4113 (depth-cap composition), #4115 (doc rot).

### Untrusted-input robustness — **still MET**

Nothing in this window's commits weakens the `.pex` memory-safety story (both `.pex` fixes are
additive: a wider test sample, a compile-time assert). The `.psc`/console side gained real
protection (#4479's CR-only fix restores the newline-terminator contract on classic-Mac line
endings, which previously voided it entirely) but the new finding shows the "construct
gluing" defect class from #4321/#4472 is broader than believed — every instance still requires
an already-invalid continuation line and produces a wrong-but-plausible AST with zero
diagnostics, not a crash/OOB/OOM. Severity and reachability are therefore unchanged from the
baseline's PEX-D4 bracket (offline `extender_preflight` + localhost debug console only).

## Verification of the nine delta commits

| Fix | Dim | Verdict |
|---|---|---|
| `f525bbcde` → **#4474** | 1 | **SOUND.** `build_sample_full_coverage()` wired into the exhaustive-prefix sweep as a fifth sample; covers debug-info-present skip paths, full getter/setter property bodies, non-empty `struct_infos`, and `Value::Float` (twice). |
| `72c687600` → **#4475** | 1 | **SOUND.** `const _: () = assert!(OpCode::TryLockGuards as u8 == MAX_OPCODE - 1);` — compiles (a failing const-assert is a hard build error), backstops the runtime pin. |
| `1460b1eb0` → **#4476** | 2 | **SOUND.** Misnamed test renamed to what it actually pins; a genuinely new `backward_jmpt_builds_a_loop_edge` test asserts JmpT polarity inside a loop head (`on_true` = backedge, `on_false` = exit) — the mirror the old name promised. |
| `ad96380be` → cleanup | 2 | **SOUND.** Removed a stray duplicate `#[test]` attribute; build is clean, no test lost (74 pex tests, same as baseline's 70 + the #4474/#4476 additions). |
| `7cc8a8313` → **#4473** | 3 | **SOUND.** Auto-state index resolved once (first case-insensitive match) before the state loop; `is_auto` can now be `true` for at most one state structurally (an `Option<usize>` equals only one loop index). Pinned by a hand-assembled collision fixture. |
| `585478d6e` → **#4477** | 3 | **SOUND.** Doc-only + a pin test; the `is`→`Cast` rewrite itself is unchanged. Third departure now bookkept alongside the two in `boolean.rs`'s module doc. |
| `352455afd` → **#4472** | 4 | **SOUND for the six sites it names** (`mod.rs` colon-loop + array-suffix, `stmt.rs` VarDecl-disambiguation + assign-op, `script.rs` header-flag-loop + top-level type-prefixed dispatch), each with a dedicated zero-error-glue regression test. **Incomplete as a class fix** — see PEX-D4-2026-09-22-01. |
| `ff26d3b69` → **#4479** | 4 | **SOUND.** `\r` removed from lexer whitespace-skip, `Newline` now matches `\r\n\|\r\|\n` (CRLF stays one token); `MalformedNumber` patterns for bare `0x` and exponent shapes lose to real literals on longest-match and always route to `LexError`. String-literal lexing unaffected (quote-delimited regex matches as one greedy token). |
| `7e8181a9e` → **#4478** | 4 | **SOUND** for what it documents (depth-cap per-link charge, corrected paren-depth measurement, refreshed test counts) — but its own Pitfalls-section wording ("#4472 tracks the remaining newline-skipping decision sites") reads as if #4472 closed that residue class; PEX-D4-2026-09-22-01 shows it did not. |

## Findings

Deduplicated against `/tmp/audit/issues.json` (refreshed this session) and every
`docs/audits/AUDIT_*_2026-09-21*.md` / `AUDIT_*_2026-09-22.md` sibling report. No collision —
this finding's keywords (`check_raw`, `peek_raw`, newline-glue, `parse_variable_body`,
`Extends`, group/function/property flags) appear in no other open or closed issue and no
sibling report.

---

### MEDIUM

#### PEX-D4-2026-09-22-01: seven more newline-skipping `peek()`/`check()` sites glue adjacent lines with zero errors — #4472 fixed six, left at least seven

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `crates/papyrus/src/parser/script.rs:101` (`Extends` header clause),
  `script.rs:259` (`parse_variable_flags`), `script.rs:366` (`parse_function_flags`),
  `script.rs:396` (`parse_property` initializer), `script.rs:701` (`parse_group` flags),
  `crates/papyrus/src/parser/stmt.rs:211` (`parse_var_decl_or_expr` initializer),
  `stmt.rs:245` (`parse_variable_body` initializer — backs both `stmt.rs:185` local
  keyword-typed decls and `script.rs:676` struct members)
- **Status**: NEW (residue class of #4472, the same way #4472 was itself the residue class of
  #4321)
- **Untrusted-Input**: Yes
- **Description**: `352455afd` (#4472) made six specific decision sites use `Parser::check_raw`/
  `peek_raw` instead of the newline-skipping `peek()`/`check()`, so a malformed continuation
  line no longer silently glues into the previous single-line construct. A systematic sweep of
  every remaining `self.peek()`/`self.check(&Token::…)` call site in
  `crates/papyrus/src/parser/{mod,script,stmt,expr}.rs` — cross-checked against each site's
  grammar (single-line construct vs. legitimate multi-line container/paren-bounded continuation)
  — found seven more sites of the *identical* defect, none touched by #4472's fix. Every one was
  verified by direct execution (a throwaway probe crate depending on `byroredux-papyrus` by
  path; deleted at session end, no repo files touched), not just by reading:

  | Site | Construct | Probe input | Result (want: recovered error) |
  |---|---|---|---|
  | `script.rs:101` | `ScriptName IDENT (Extends IDENT)?` header | `ScriptName Foo⏎Extends Bar⏎Function F()⏎EndFunction⏎` | `parent: Some("Bar")`, 0 errors |
  | `script.rs:259` | top-level `Variable` `Const`/`Conditional` flags | `Int Foo⏎Const⏎Function F()…` | `is_const: true`, 0 errors |
  | `script.rs:366` | `Function`/`Event` `Global`/`Native`/`DebugOnly`/`BetaOnly` flags | `Function F()⏎Global⏎EndFunction⏎` | `flags: GLOBAL`, 0 errors |
  | `script.rs:396` | `Property` `(= expr)?` initializer (sibling of the fixed top-level-Variable case) | `Int Property P⏎= 5 Auto⏎Function F()…` | `initial_value: Some(5), flags: AUTO`, 0 errors |
  | `script.rs:701` | `Group` `CollapsedOnRef`/`CollapsedOnBase` flags | `Group MyGroup⏎CollapsedOnRef⏎Int Property P Auto⏎EndGroup⏎` | `flags: COLLAPSED_ON_REF`, 0 errors |
  | `stmt.rs:211` | local `Ident`-typed VarDecl initializer (`Actor x = None`) | `Function F()⏎Actor x⏎= None⏎EndFunction⏎` | `initial_value: Some(NoneLit)`, 0 errors |
  | `stmt.rs:245` | local **keyword**-typed VarDecl initializer *and* `Struct` member initializer (one function, two call sites: `stmt.rs:185`, `script.rs:676`) | `Function F()⏎Int x⏎= 5⏎EndFunction⏎` and `Struct S⏎Int a⏎= 5⏎EndStruct⏎` | both: `initial_value: Some(...)`, 0 errors |

  `stmt.rs:245` is the highest-value gap: `Type name (= expr)?` local declaration is the single
  most common statement shape in real Papyrus scripts, and the one unfixed `self.peek()` there
  backs two call sites (local decls and struct members) at once.

  Every remaining `peek()`/`check()` site in the four files was also individually classified and
  is **not** part of this bug: container body-item loops (`script.rs:519,592,666,727`) already
  call `skip_newlines()` immediately before the check by design (they're meant to skip blank
  lines between sibling items, same as `ElseIf`/`Else` at `stmt.rs:138,146` and the top-level
  item loop); parameter-list and call-argument checks inside parens
  (`script.rs:338,345,352`, `expr.rs:284`, `mod.rs:388`) are bounded by an unambiguous `)`/`]`
  terminator, matching the pre-existing, deliberate multi-line-call-argument precedent.
- **Evidence**: `/tmp/audit/papyrus/probe/` (deleted at cleanup) — a path-dependency crate on
  `byroredux-papyrus`, built into the shared `target/` dir; each row above is a captured program
  run, not a manual trace. The happy-path (single-line) shape of every one of these eight
  constructs already has test coverage per `docs/engine/papyrus-parser.md`'s test-count
  breakdown (e.g. `script.rs`'s 21 inline tests cover "header, Extends + flags") — none of it
  exercises the malformed-continuation shape, which is exactly why `cargo test` stays green
  while the bug is live.
- **Impact**: malformed `.psc` source is silently accepted as a different, plausible-but-wrong
  AST instead of surfacing a recovered parse error — script flags/initializers/parent class end
  up attributed to the wrong construct. Reachable via the same path the baseline named for
  #4472's own class: `crates/scripting/examples/extender_preflight.rs`'s `scan_source`
  (`extender_preflight.rs:74-90`) calls `byroredux_papyrus::parse_script(&source)` directly on
  `.psc` files read from disk during the offline compatibility scan, plus the localhost debug
  console's expression evaluator. No runtime/game-load path is affected. Every reproduction
  requires at least one already-invalid line, matching the baseline's rationale for keeping
  this class MEDIUM rather than HIGH.
- **Related**: #4472 (fixed, but incomplete), #4321 (the original class), #2656 (the
  `parse_property_flags` sibling that *was* fixed correctly, for comparison)
- **Suggested Fix**: apply the same `check_raw`/`peek_raw` discipline to all seven sites, one
  regression test per site following the pattern `crates/papyrus/src/parser/script.rs:836-946`
  already established. `parse_variable_body` backs two call sites, so fixing it clears both the
  local-decl and struct-member halves in one edit; `parse_var_decl_or_expr`
  (`stmt.rs:211`) is a separate function and needs its own fix. Worth a standing grep-based test
  or lint (`self.peek()`/`self.check(&Token` outside an explicit allow-list of the
  container-loop/paren-bounded sites) so a newly added single-line grammar construct doesn't
  reintroduce this class silently — the same suggestion the baseline made for #4321's own
  residue, which is exactly how #4472 was found, and how this finding was found in turn.

---

## Carried-open findings (not re-filed — unchanged since baseline)

- **#4471** (PEX-D3-2026-09-19-01, MEDIUM) — `EVENT_NAMES` missing 12 real engine events, 20
  handlers demote to `Function`. `event_names.rs` has zero commits since baseline; confirmed
  still OPEN in the refreshed issue list.
- **#4113** (MEDIUM) — `MAX_EXPR_DEPTH`/`MAX_REBUILD_DEPTH` independent, additively-composing
  caps. `control_flow.rs` unchanged this window.
- **#4115** (LOW) — `control_flow.rs` module doc still says the fail-closed `||` branch is
  "advanced past". Unchanged this window.

## Decompiler Soundness Matrix (delta from baseline only — full matrix in AUDIT_PAPYRUS_2026-09-19.md)

| Pass | Change this window | Verdict |
|------|---|---|
| Reader (`reader.rs`+`opcode.rs`+`lib.rs`) | +1 exhaustive-prefix sample (#4474), +1 compile-time assert (#4475) | Strengthened, no regression |
| CFG (`cfg.rs`) | Test rename + new JmpT-loop pin (#4476) | Strengthened, no regression |
| Lift + copy-prop (`lift.rs`) | None | Unchanged from baseline's clean verdict |
| Boolean (`boolean.rs`) | None | Unchanged from baseline's clean verdict |
| Control-flow (`control_flow.rs`) | None | Unchanged; #4115 doc rot still open |
| Lower + assembly (`lower.rs`) | Case-collision fix (#4473), `is`-departure bookkeeping (#4477) | Strengthened, no regression |
| `.psc` lexer | CR-only / `0x` / exponent diagnostics (#4479) | Strengthened, no regression |
| `.psc` parser | 6 of ≥13 newline-glue sites fixed (#4472) | **Partially strengthened — 7 sites remain (PEX-D4-2026-09-22-01)** |

## Coverage notes and caveats

- `pex_corpus_smoke` not re-run this session (constraints: no `--release`, no smoke scripts);
  the 09-19 measurement (26,640/26,641) is cited unchanged, not re-verified.
- The new finding's probe crate and its build artifacts were removed from the shared `target/`
  dir at cleanup; no repo files were created, edited, or committed.
- FO76 dialect coverage gap (zero corpus files) unchanged from baseline; not re-probed.

## Cross-audit routing

- Recognizer/runtime consumption of these ASTs, `translate_pex`'s clean-`None` contract, the
  panic net, and the byte-for-byte R5 fidelity half → `/audit-scripting` (Dims 1, 5).
- `call_sites`' budget-stop `CallSiteDiagnostic` consumer → `/audit-scripting` Dim 7.
- PEX-D4-2026-09-22-01's reachability note (`extender_preflight`'s `scan_source`) is shared
  infrastructure with `/audit-scripting` Dim 7's `.pex` preflight path — worth a one-line
  cross-check there that malformed `.psc` inputs to the same tool are handled the same way.

## Findings count

**1 new finding: 0 CRITICAL, 0 HIGH, 1 MEDIUM, 0 LOW.** Matched to existing open issues: 0 (no
collision found). Fixes verified sound: #4473, #4474, #4475, #4476, #4477, #4478, #4479 (7 of 8
fully); #4472 verified sound for its own six sites, incomplete as a class (residue filed as the
new finding above). Carried open, unchanged: #4471, #4113, #4115.
