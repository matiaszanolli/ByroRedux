# Papyrus Frontends Audit — 2026-09-19

First dedicated pass over the two Papyrus frontends (`crates/pex` decode +
5-phase decompiler, `crates/papyrus` lexer + Pratt parser). Run as the skill
prescribes: an orchestrator plus four dimension agents, all concurrent, each
writing `/tmp/audit/papyrus/dim_N.md` before this consolidation.

**HEAD**: `41aed20eb` · **Baseline**: [AUDIT_SCRIPTING_2026-09-14.md](AUDIT_SCRIPTING_2026-09-14.md)
(HEAD `5278e163` — that report's Dims 1–4 are this domain; it audited the
**pre-fix** state) · **Audited**: Dims 1–4 full (first-ever AUDIT_PAPYRUS) ·
**Unchanged since baseline**: `decompile/{cfg,node,lift}.rs` (Dim 2),
`decompile/{control_flow,event_names,mod}.rs` (Dim 3), all of `crates/papyrus`
except `parser/expr.rs` (Dim 4) — each verified via `git log` and read in full
anyway. **Delta**: exactly two commits landed after the baseline report, same
day: `a194afc1f` ("Fix #4317, Fix #4319") and `b23fda1eb` ("Fix #4320, Fix
#4321"). Both are verified below.

## Build & test state — CLEAN

```
$ cargo test -p byroredux-pex -p byroredux-papyrus
   papyrus 100 unit + 4 round-trip · pex 70 unit + 1 doc-test · 0 failed
   (r5_fidelity #[ignore]d without --ignored, as documented)

$ cargo run --release -p byroredux-pex --example pex_corpus_smoke -- \
    "Skyrim - Misc.bsa" "Fallout4 - Misc.ba2" "Starfield - Misc.ba2"
   26,640 / 26,641 · 0 panics · 0 shape mismatches   (see below)

$ cargo test -p byroredux-scripting recognizes_da10_and_reproduces_hand_builder
   1 passed   (.psc-vs-.pex fidelity gate half; byte-for-byte half owned by /audit-scripting)
```

## Executive Summary

**10 findings: 0 CRITICAL, 0 HIGH, 3 MEDIUM, 7 LOW** (one LOW is the open
#4115). The domain's two standing HIGHs from 09-14 are **fixed and verified**;
no new HIGH emerged from a fresh-eyes full sweep of code that prior passes had
already measured clean.

### Untrusted-input robustness — **MET (first "clean" verdict for this domain's memory axis)**

A hostile `.pex` or `.psc` **cannot panic, OOB, OOM, or abort** the cell
loader, extender preflight, or the localhost console at `41aed20eb`:

- `.pex` (Dim 1): the #4317 string-copy budget is charged before every owned
  copy on every path and refuses with `Err(StringBudgetExceeded)`; the 09-14
  abort shape (≈328 KB wire-valid file demanding ~8.6 GB) now returns a clean
  `Err` in 9.5 ms at **23 MB peak RSS** under a 2 GiB ulimit (was: process
  abort). The 3.8 GB `call_sites` shape stops at 255 calls + 1 budget
  diagnostic at 37 MB. `take(n)` is confirmed the single bounds gate;
  var-arg counts never pre-allocate; the `transmute` guard holds (51-row
  full-table pin, accurate SAFETY comment); endianness can't leak; corpus
  max model/file ratio 9.0×.
- `.psc`/console (Dim 4): the #4320 depth ledger is tight and leak-free
  (measured boundary: 255-link chain parses, 256 errors; restore wraps every
  exit including error paths — a probe erroring deep inside one function
  leaves the next function parseable). 1M-link chains error cleanly on an
  8 MB thread; the maximum *legal* tree (256 deep) parses, Debug-prints,
  Clones, and Drops safely. No silent wrong-AST from valid source was found
  post-#4321.
- Reachability caveat unchanged: no runtime path feeds game/mod `.psc` into
  the parser (only offline `extender_preflight` and the localhost debug
  console), so `.psc` findings stay one severity step below their `.pex`
  twins.

### The two post-baseline fix commits — both VERIFIED

| Fix | Verdict |
|---|---|
| `a194afc1f` → **#4317** (string-copy OOM) | **SOUND.** Budget computed once per decode/scan (`lib.rs:86-90`, `reader.rs:47`, `call_sites.rs:70-73`), charged before every copy, refuses rather than truncates; `call_sites` stops with a `CallSiteDiagnostic`. Grep found no escaped uncharged copy. Measured: table above. |
| `a194afc1f` → **#4319** (named-auto-state inversion) | **CORRECT.** Script scope keyed on `state.name.is_empty()` (`lower.rs:445`); every named state → `State { is_auto: is_auto_state(..) }` (`lower.rs:457-462`); rewritten guard pins the right rule (`lower.rs:602-663`). Four hand-built adversarial assemblies (auto-with-no-`""`, empty default, case collision, no-Auto control) all produce Champollion-correct ASTs for compiler-producible inputs. |
| `b23fda1eb` → **#4320** (iterative chain depth bypass) | **FIXED, deep-verified.** `enter_chain_link` charges each postfix/binary wrap (`expr.rs:63-70`); `parse_expr_bp` value-restores the ledger after the entire inner body (`expr.rs:41-51`), so no exit path leaks a unit; the cap boundary is exactly 256 nodes; all recursive entry funnels through the guarded `parse_expr_bp`. |
| `b23fda1eb` → **#4321** (`(`-line glue) | **FIXED, no regressions.** Pratt loop decides on `peek_raw()`; the regression shape parses as 2 statements / 0 errors; multi-line `If` conditions, operator-ends-line, `\` continuations, and lenient multi-line call args all still parse. Six *residual* newline-skipping sites outside the loop are filed (PEX-D4-2026-09-19-01). |

### The 26,640/26,641 decompile-rate claim — re-measured, exactly reproduced

| Game | Decompiled | vs baseline (09-14) |
|---|---|---|
| Skyrim SE | 14,026 / 14,026 | unchanged |
| FO4 | 7,874 / 7,875 | unchanged (the known legit `stimboxscript.pex` `\|\|` fail-closed, #1732) |
| Starfield | 4,740 / 4,740 | unchanged |
| **Total** | **26,640 / 26,641** | **exact reproduction** |

The harness tallies `Err` and panics after `decompile_script` inside
`catch_unwind`; its `expected_top_level_item_count` shape check is now
**independent** of `decompile_script` (no shared helper or predicate — the
#3017/#4319 blind-spot class is closed; a regression to the old rule could no
longer pass). What the rate proves: the pipeline is panic-free and fail-closed
on every vanilla script in these archives, and the coarse top-level item shape
matches an independently derived prediction. What it does **not** prove:
statement-level fidelity inside an item — #4319's 983 inverted scripts passed
this exact harness. Fidelity is owned by the `.psc`-vs-`.pex` R5 gates
(`recognizes_da10_and_reproduces_hand_builder` re-run green here; the
byte-for-byte half is `/audit-scripting` Dim 1).

## Decompiler Soundness Matrix

| Pass | Bounds-safe | Terminates | Total (no panic/abort) | Memory ≈ input | Fidelity-tested |
|------|:---:|:---:|:---:|:---:|:---:|
| Reader (`reader.rs`+`opcode.rs`+`lib.rs`) | Yes (`take(n)` sole gate; exhaustive-prefix guard + 09-14's 1.8 M flips/22 k truncations) | Yes | Yes | **Yes (post-#4317 budget; 9.0× worst model ratio, 14 MB corpus RSS)** | Yes (51-row Champollion full-table pin; 26,641 corpus) |
| `call_sites.rs` preflight | Yes | Yes (O(D) build, O(1) lookup, #3938) | Yes | **Yes (ScanBudget; 255-call stop + 1 diagnostic, 37 MB)** | Yes |
| CFG (`cfg.rs`) | Yes (`checked_target` incl. exit anchor; `split(at==0)` unreachable) | Yes | Yes (3 `expect`s guarded by the block-partition invariant) | Yes | Yes (0 CFG errors / 99,918 fns, 09-14; code unchanged) |
| Lift + copy-prop (`lift.rs`) | Yes (every index bounded by exact `arg_count` decode + full-table pin) | Yes (#2024 linear, re-pinned) | Yes (#2666 runtime fail-closed, no `debug_assert!` anywhere) | Yes | **Yes — structurally strengthened**: fold-into-dest and cross-block folding proved impossible (`is_final`/`is_temp_var` are complementary sets); node-memoized depth un-bypassable across all three fold paths |
| Boolean (`boolean.rs`) | Yes | Yes (each reprocess removes exactly 2 blocks) | Yes (depth gate at the merged re-fold, `:294`) | Yes | Yes (departure 1 below) |
| Control-flow (`control_flow.rs`) | Yes | Yes (cap 1024, derived) | Yes (`\|\|` shape fails closed, #1732; doc stale #4115) | Yes | Yes |
| Lower + assembly (`lower.rs`) | Yes | Yes | Yes (`lower_expr` total over all 16 `NodeKind`s; default-arm `Eq` unreachable — 13 op strings re-enumerated) | Yes | **Yes (post-#4319, adversarially probed); event classification has a gap — 12 engine events missing (PEX-D3-01)** |
| `.psc` lexer + parser | Yes (depth ledger tight: 255/256 measured) | Yes (recovery consumes ≥1 token, pinned) | Yes (no panics; truncation probes end in µs) | Yes | **Partial** — newline contract fixed in the Pratt loop; six residue sites outside it (PEX-D4-01, invalid-source only); three silent-mislex shapes (PEX-D4-03) |

### The documented Champollion departures

1. **No debug-line guard in `boolean.rs`: benign in practice (re-adjudicated).**
   The structural gates that substitute for Champollion's per-instruction line
   check were re-verified line-by-line (source's last statement computes the
   condition var `:179-181`; the single operand statement recomputes the same
   var via `take_operand` `:105-125`; the operand block must fall through
   non-conditionally to the rejoin `:260-263`). The 09-14 census (10,733
   collapses, all Bool-typed, zero non-Bool) stands — the code is unchanged
   since the measurement — and today's corpus run is clean (0 shape
   mismatches). Accepted, documented risk.
2. **`||`-skip in `control_flow.rs`: correct and fail-closed** (#1732). The
   conditional-predecessor arm returns `Err(ControlFlowFailed)` before
   `take_scope` — nothing advances past the block. Only the module doc is
   stale (#4115, still open, LOW).
3. **NEW, third in-code departure needing bookkeeping**: `is` (opcode 36)
   lowers to `Expr::Cast` (`lower.rs:110-118`) — an object-typed `as` where a
   Bool type-test belongs; ill-typed in a condition position. Documented
   in-code as deliberate, ≈0 vanilla reachability (no `.psc` construct emits
   opcode 36). Filed LOW (PEX-D2-2026-09-19-02) with the suggestion to record
   it beside the other two in the scripting baseline.

## Findings

**3 MEDIUM, 7 LOW.** Deduplicated against `/tmp/audit/papyrus/issues.json`
and all `AUDIT_SCRIPTING_*.md`. The four 09-14 front-end findings (#4317,
#4319, #4320, #4321) are CLOSED and verified above — not re-filed. Open
cross-references carried forward: #4113 (depth-margin composition; constants
unchanged at 1024/256), #4115 (filed below as Existing).

---

### MEDIUM

#### PEX-D3-2026-09-19-01: `EVENT_NAMES` is missing real engine events — 20 vanilla handlers demote to `Function`
- **Severity**: MEDIUM
- **Dimension**: Decompiler Boolean/Control-Flow/Lower (event classification)
- **Location**: `crates/pex/src/decompile/event_names.rs:13-281`; consumed at `crates/pex/src/decompile/lower.rs:295-297`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The classification list misses engine events that vanilla handlers actually implement, so the `on`-prefix AND `is_event_name` rule demotes those handlers to `Function` in the AST. A full-corpus census (26,641 scripts; 12,096 `on`-prefixed functions) found 33 distinct demoted names, of which 12 names / 20 handlers are engine events per the strongest available signal — implemented in base-class scripts (`actor.pex`, `spaceshipreference.pex`, `quest.pex`, …) that can only override engine hooks: Skyrim `OnAttach` ×2; FO4 `OnStoryClearLocation` ×1; Starfield `OnShipCruiseArrival`, `OnGameplayOptionChanged`, `OnUnconscious`, `OnStoryChangeLocationEx`, `OnSpaceshipCombatListAdded/Removed`, `OnPlayerFastTravel`, `OnStorySpeechChallengeCompletion`, `OnPlayerScanPlanet`, `OnPlayerShip` (17 handlers). The other 21 demoted names are correct demotions (vanilla source typos like `oncelldetatch`, script-defined `oncritter*`). The header pins the list to Champollion's `EventNames.hpp`, but the shipped list already contains Starfield events beyond Champollion v1.3.2, so its generation source is a Starfield-era union that is itself incomplete.
- **Evidence**: census at `/tmp/audit/papyrus/demotion_census.txt` (dim-3 probe); absent names grep-confirmed in `event_names.rs`; classification path `lower.rs:295-297` → `event_names.rs:285-288`.
- **Impact**: ~0.17% of `on`-prefixed vanilla functions (17 of 20 in Starfield) come out as `Function` instead of `Event` — bodies and names survive, but any recognizer or round-trip consumer keying on event-ness misses them and a `.psc` rendering loses the `Event` keyword. Fidelity gap, not robustness.
- **Related**: #3786/#3943 (adjacent classification fixes, closed)
- **Suggested Fix**: Regenerate the union from a complete per-game event dump (game source `.psc` event sets / CK event index) instead of Champollion's frozen lists, keeping the sorted+lowercase+binary-search invariant and `list_is_sorted_for_binary_search`; add the 12 names to the smoke assertions.

#### PEX-D4-2026-09-19-01: six cross-line lenient `peek()`/`check()` sites outside the Pratt loop still glue adjacent lines into one construct with zero errors
- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:290` (colon-qualified ident), `mod.rs:369` (`[` type suffix), `stmt.rs:262` (assign-op), `stmt.rs:206` (VarDecl disambiguation), `script.rs:203-214` (top-level type→item peek), `script.rs:112` (header flag loop)
- **Status**: NEW (residue class of #4321 — exactly the "audit the other same-line peek/check decisions" follow-up named by SCR-D4-2026-09-14-02; #2656 was fixed only at `parse_property_flags`)
- **Untrusted-Input**: Yes
- **Description**: `b23fda1eb` made the Pratt loop newline-terminating, but the six sites above still decide on newline-skipping `peek()`/`check()`. All six were probe-confirmed to glue the next line in with zero errors: `foo`⏎`:bar()` → one `Call`; `x`⏎`= 5` → one `Assign`; `Foo`⏎`bar = 1` → one `VarDecl`; `Actor`⏎`[] props` → Array-typed Variable; `Int`⏎`Property P` glues at top level; and the header flag loop consumes a following line's `Native`/`Const`/`Hidden` into `ScriptFlags` — probe showed a function's own `Native` silently promoted to a script flag with `Function.flags` left empty. Unlike #4321's valid-source shape, every one of these requires at least one invalid line — hence MEDIUM, not HIGH.
- **Evidence**: dim-4 probe table (all zero-error); most consequential is the header-flag swallow, which corrupts a flag bitfield rather than merely re-shaping already-broken source.
- **Impact**: malformed mod source is silently accepted as a plausible-but-wrong AST; `extender_preflight` under/over-reports. No valid-source shape found in the probe set.
- **Related**: #4321 (fixed), #2656 (fixed at one site), SCR-D4-2026-09-14-02
- **Suggested Fix**: Apply the `peek_raw()` discipline (or a same-line span check) at the six sites; pin each with a zero-error-glue regression test like `a_line_opening_with_a_paren_is_not_a_call_on_the_previous_line`.

#### PEX-D3-2026-09-19-02: case-colliding state names both get `is_auto: true` (malformed input only)
- **Severity**: MEDIUM (wrong-AST property with no current consumer; input not compiler-producible)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower (script assembly)
- **Location**: `crates/pex/src/decompile/lower.rs:408-410,450-463`
- **Status**: NEW
- **Untrusted-Input**: Yes (requires a hand-assembled/hostile `.pex`; the Papyrus compiler rejects duplicate case-insensitive state names)
- **Description**: `is_auto_state` matches case-insensitively (#3786, correct and retained), so a `.pex` carrying states `waiting` + `WAITING` with `auto_state_name = "Waiting"` marks **both** `State` items `is_auto: true` — an AST no Papyrus source can express. Champollion's case-sensitive comparison marks at most one.
- **Evidence**: dim-3 adversarial probe case C: `State(AUTO) waiting; State(AUTO) WAITING`, both flagged auto.
- **Impact**: Cosmetic today — no recognizer or runtime consumer reads `State::is_auto` (repo-wide grep: producers + tests only); vanilla corpora cannot contain the input. Becomes a HIGH-class wrong-AST-accepted-by-consumer only if a state-aware consumer boots on `is_auto` without its own case handling.
- **Related**: #4319 (assembly rule), #3786/#3943
- **Suggested Fix**: Mark `is_auto` on the first case-insensitive match and skip duplicate case-collisions, or document the malformed-input behavior at `is_auto_state`.

---

### LOW

#### PEX-D1-2026-09-19-01: exhaustive-prefix guard's samples never reach the FO4+ debug skip paths, full-property bodies, struct infos, or `Value::Float`
- **Severity**: LOW (test gap; the paths are corpus-proven correct today)
- **Dimension**: PEX Reader
- **Location**: `crates/pex/src/lib.rs:892-918` (sample array); gaps at `reader.rs:257-283` (`skip_property_groups`/`skip_struct_orders`), `reader.rs:413-419` (full getter/setter wire bodies), `reader.rs:342-362` (`read_struct_infos`), `reader.rs:173` (`Value::Float`)
- **Status**: NEW (extends #3942's guard)
- **Untrusted-Input**: Yes
- **Description**: The exhaustive-prefix test enumerates every prefix of 4 handbuilt samples, but none exercises: FO4+/Starfield debug info (the extender sample is BE, which stops before the skip functions; the LE samples are debug-absent), a non-auto property with getter/setter bodies on the wire, non-empty struct infos, or a `Value::Float` operand. A one-byte regression in any of those ships green — 12,308 LE+debug and 4,740 Starfield corpus files prove the paths work *now*, but there is no mechanical pin. Same class: the FO76 dialect (`game_id 3`) appears in zero corpus files and no handbuilt sample.
- **Evidence**: dim-1 sample-builder analysis + corpus probe (0 failures on the uncovered paths).
- **Impact**: A future skip-reader refactor (the "one wrong u16" class) silently desyncs FO4/FO76/Starfield debug-compiled scripts.
- **Related**: #3942
- **Suggested Fix**: Add two samples to the prefix array: FO4/Starfield debug-present with non-empty property groups + struct orders, and one with a full property (getter/setter bodies), non-empty struct info, and a `Value::Float` operand (~30 lines each on the existing `PexWriter`).

#### PEX-D1-2026-09-19-02: `transmute` invariant pinned by a runtime test, not a compile-time assert
- **Severity**: LOW (hardening)
- **Dimension**: PEX Reader
- **Location**: `crates/pex/src/opcode.rs:68,130-137,160-166`
- **Status**: NEW
- **Untrusted-Input**: Yes (the invariant protects `from_u8` on hostile bytes)
- **Description**: The `unsafe transmute` in `from_u8` is sound only while `MAX_OPCODE == 51 == last discriminant + 1` with contiguous `#[repr(u8)]` discriminants — today true, SAFETY comment accurate, and the 51-row full-table test pins it at runtime. But nothing fails at *compile time* if a variant is added or `MAX_OPCODE` edited without re-deriving: a mid-enum insertion shifts every discriminant and can push `OPCODES[self as usize]` out of bounds for out-of-table bytes.
- **Evidence**: `opcode.rs:134-136` SAFETY; `OPCODES` length tied to `MAX_OPCODE` (`:73`); pin lives in `#[cfg(test)]` only.
- **Impact**: None today (CI runs the test); the failure mode if the test is skipped/deleted is a wrong-arg-count decode or index panic on hostile bytes.
- **Related**: #2127, #3948
- **Suggested Fix**: `const _: () = assert!((OpCode::TryLockGuards as u8) + 1 == MAX_OPCODE, "…");` next to `MAX_OPCODE`.

#### PEX-D2-2026-09-19-01: `backward_jmpt_builds_a_loop_edge` guard tests a JmpF, not a JmpT
- **Severity**: LOW (test-naming rot; no coverage hole)
- **Dimension**: Decompiler CFG & Lift
- **Location**: `crates/pex/src/decompile/cfg.rs:371-401` (misnomer); JmpT polarity actually pinned at `cfg.rs:474-475` in the differently-named #2122 sibling
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The guard a reader would credit for JmpT loop polarity builds its conditional from `OpCode::JmpF` (`:379`) with an unconditional backedge — it re-pins JmpF polarity a second time. JmpT *is* pinned, in `backward_jmpt_target_inside_own_block_conditions_the_right_block` (`on_true` = jump target, `on_false` = fall-through). An auditor grepping by name would credit the wrong test and could break the real pin while "updating" this one.
- **Evidence**: `cfg.rs:379` uses `OpCode::JmpF` inside the `jmpt`-named test; contrast `:459`.
- **Impact**: Auditability/regression-hunt misdirection only.
- **Related**: #2122
- **Suggested Fix**: Rename (e.g. `forward_jmpf_with_unconditional_backedge_builds_a_loop`) or add the missing JmpT-conditional loop-head mirror.

#### PEX-D2-2026-09-19-02: `is` opcode decompiles to an object-typed cast — documented departure needs bookkeeping
- **Severity**: LOW
- **Dimension**: Decompiler CFG & Lift (observed at `lift.rs:231`; rewrite in `lower.rs:110-118`)
- **Status**: NEW (departure dates to `56a2beef1`, June 22 — pre-baseline; first filed)
- **Untrusted-Input**: Yes
- **Description**: `a is T` (opcode 36, Bool result) lowers to `Expr::Cast` — ill-typed in a condition position — because the shared AST has no `is` operator. Documented in-code as deliberate and "recognizer-irrelevant"; recognizers key on names/calls/condition shape and ≈0 vanilla scripts contain opcode 36 (no `.psc` construct emits it).
- **Evidence**: `lower.rs:110-118` comment; 09-14 corpus shows no `is`-related errors.
- **Impact**: A future consumer that renders lower-only ASTs to source text or evaluates them strictly would emit/reject ill-typed conditions on FO4+/Starfield scripts.
- **Related**: #4115-style departure bookkeeping
- **Suggested Fix**: Add an `Is`-capable expression (or condition-only `TypeTest`) to the shared AST, or record the departure beside the other two in the scripting baseline so it is findable.

#### PEX-D3-2026-09-19-03: `control_flow.rs` module doc still describes the fail-closed branch as "advanced past"
- **Severity**: LOW (doc rot)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Location**: `crates/pex/src/decompile/control_flow.rs:27-29`
- **Status**: **Existing: #4115** (OPEN — confirmed still stale at `41aed20eb`)
- **Untrusted-Input**: Yes
- **Description**: The module doc's residual-else sentence says the conditional-predecessor branch is "left unmerged and advanced past"; the code returns `Err(ControlFlowFailed)` before `take_scope` (`:207-218`). The in-code comment is correct; only the module doc is stale.
- **Suggested Fix**: One-line doc edit, as #4115 says.

#### PEX-D4-2026-09-19-02: `docs/engine/papyrus-parser.md` depth-cap section predates both fixes; stale inline comment in a cap test
- **Severity**: LOW (doc rot)
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `docs/engine/papyrus-parser.md:223-229` (depth caps), `:108-110` + `:234-236` (newlines), `:267-279` (test counts); `crates/papyrus/src/parser/expr.rs:944-949` (comment)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The contract doc still describes the cap as increment/decrement around each recursion — it omits #4320's `enter_chain_link` per-chain-link charging, the load-bearing half since the fix. The newline section names only empty-`Return` as a `peek_raw` load-bearer; "a newline ends an expression; an operator ending a line still continues" (#4321) is undocumented. Test counts are stale (says 73; crate is now 100+4). Separately, `depth_cap_accepts_legitimate_nesting`'s comment claims 2 depth units per paren-pair — measured 1/pair.
- **Impact**: A future editor touching the loop reads a wrong model of the depth ledger and newline contract.
- **Related**: #4320, #4321
- **Suggested Fix**: Update the depth-cap section (entry-charge + per-link charge + restore-on-every-exit), add the newline-terminates-expression bullet, refresh counts, fix the paren comment.

#### PEX-D4-2026-09-19-03: lexer turns malformed/exotic source into plausible valid tokens with zero diagnostics (`0x`, `1e5`, CR-only line endings)
- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Pratt Parser
- **Location**: `crates/papyrus/src/token.rs:241-247` (literal regexes), `:105` (skip `[ \t\r]+`), `:108` (`Newline` = `\n` only)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: (a) `0x` with no digits lexes as `IntLit(0)` + `Ident("x")`; (b) `1e5` lexes as `IntLit(1)` + `Ident("e5")` (Papyrus has no exponents); (c) a CR-only (classic-Mac) file produces **no `Newline` tokens at all** — the whole file is one line to the parser, voiding the terminator contract and the #4321 protection on such files (probe: two statements glue into one, 0 errors). CRLF is handled correctly. Literal *overflow* is fine (`0xFFFF…FFFF` errors cleanly, no panic).
- **Evidence**: dim-4 probe rows.
- **Impact**: Exotic/typo'd source is silently mis-tokenized instead of precisely diagnosed. No known corpus hits; hence LOW.
- **Related**: #1908 (same "lex must not lie" theme)
- **Suggested Fix**: Lex-error on a digitless `0x` prefix and on `e`-adjacent mislexes; treat lone `\r` as `Newline` or error on `\r` not followed by `\n`.

---

## Coverage notes and caveats

- **Champollion is not vendored** in this repo. Opcode rows 0–47 and the
  debug/property-group/struct-order byte layouts were verified against the
  auditors' Champollion/UESP knowledge plus the 26,641-file corpus (0
  failures); Starfield rows 48–50 against community RE. Row-level doubt is
  bounded by the exact-`arg_count` decode + full-table pin structure.
- **FO76** (`game_id 3`): zero corpus files, no handbuilt sample (folded into
  PEX-D1-2026-09-19-01).
- **Boolean collapse census not re-run** (would require instrumenting tracked
  files, read-only this audit); the single-recompute gate was re-verified
  line-by-line and is unchanged since the 09-14 measurement.
- Pre-fix memory figures are quoted from the 09-14 report; the post-fix side
  was measured directly.
- Copy-prop cross-block census (3,516 reads) re-verified structurally, not
  re-measured; consistent with the code as it stands.

## Cross-audit routing

- Recognizer/runtime consumption of these ASTs, the `translate_pex` clean-
  `None` contract, the panic net, and the byte-for-byte R5 fidelity half →
  `/audit-scripting` (Dims 1, 5).
- `call_sites`' budget-stop `CallSiteDiagnostic` consumer (extender preflight
  handling in `byroredux/src`) → `/audit-scripting` Dim 7: confirm the
  diagnostic surfaces rather than being swallowed.
- Event-classification blast radius (PEX-D3-2026-09-19-01) lands on
  recognizer keying → note in `/audit-scripting` when event-ness becomes
  load-bearing.

## Findings count

**10 findings: 3 MEDIUM · 7 LOW (1 Existing: #4115) · 0 HIGH · 0 CRITICAL.**
Fixes verified: #4317, #4319, #4320, #4321. Carried open: #4113, #4115.
