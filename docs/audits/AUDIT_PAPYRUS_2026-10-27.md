**HEAD**: fa6be4a6b · **Baseline**: [AUDIT_PAPYRUS_2026-10-21.md](AUDIT_PAPYRUS_2026-10-21.md) (HEAD `4a3e89d56`) · **Audited**: Dim 3 (Boolean/Control-Flow/Lower and the corpus instruments: `475004d25`, `18ce09bff`, `bbda0c975`), Dim 4 (`.psc` Lexer & Pratt Parser: `a87650237`, `22669f049`) · **Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode), Dim 2 (Decompiler CFG & Lift)

# Papyrus Frontends Audit: 2026-10-27

Run inside `/audit-suite --preset comprehensive`. No sub-agents, no engine launch, built only in the repo `target/`.

## Delta since the baseline: 5 commits

`git log 4a3e89d56..HEAD -- crates/pex crates/papyrus`. Four of them fix findings from the 2026-10-21 report. The fifth is a clippy cleanup.

| Commit | Change | Dim | Verdict |
|---|---|---|---|
| `475004d25` (10-21) | #6262: `lower.rs` drops its private `is_temp_var` and calls `node.rs`'s Champollion-exact predicate | 3 | **SOUND.** Fixes PEX-D3-2026-10-21-01. One predicate now serves both `check_assign` (`lift.rs:327`) and `lower_local_decls` (`lower.rs:287`). The pin now declares `::TEMP1` and a bare `::temp`. No other copy remains in `crates/pex` |
| `a87650237` (10-21) | #6263: `Parent` is removed from the shared `keyword_as_ident`. The new `expect_member_name` (`parser/mod.rs:438`) accepts it only as a struct-member name (`stmt.rs:256`, same line, via `parse_variable_body(true)` from `script.rs:748`) and after `.` (`expr.rs:270`) | 4 | **SOUND.** Fixes PEX-D4-2026-10-21-01. Bounds use `tokens.get(pos)`. The struct-member path keeps the #5021 same-line rule. The FO4 vanilla + F4SE corpus gate passes |
| `22669f049` (10-21) | #6264: `papyrus-parser.md` Event row is now `Sender_EventName`; the Expr row gains `Is` and `NewStruct`; the `r5_round_trip.rs` doc no longer mentions `preprocess` | 4 | Verified. Fixes PEX-D4-2026-10-21-02 |
| `18ce09bff` (10-27) | #4115: the `control_flow.rs:28-31` module doc now says the leftover `||` shape is "declined with an error, not advanced past" | 3 | Verified against `control_flow.rs:218` (`return Err(self.fail())`). #4115 is CLOSED |
| `bbda0c975` (10-22) | #6193: one `..StructInfo::default()` removed from a `lower.rs` test fixture | 3 | No behaviour change |

Nothing touched `reader.rs`, `opcode.rs`, `model.rs`, `lib.rs`, `call_sites.rs`, `decompile/{cfg,lift,node,boolean,event_names,mod}.rs`, `lexer.rs`, `token.rs` or the corpus examples.

## Build, test and corpus state: CLEAN

```
cargo test -p byroredux-pex -p byroredux-papyrus
  papyrus 146 unit + 4 round-trip · pex 86 unit + 1 doc-test (r5_fidelity: 1 ignored, needs data) · 0 failed
FO4_PSC_DIR=".../Fallout 4/Data/Scripts/Source" cargo test -p byroredux-papyrus --lib fo4_script_sources_parse_clean_when_present
  ok (30 vanilla + F4SE .psc, KNOWN_GAPS empty)
pex_corpus_smoke (release): Skyrim - Misc.bsa 14,026 · Fallout4 - Misc.ba2 7,875 · Starfield - Misc.ba2 4,740 · SeventySix - MiscClient.ba2 7,234
  parse 33,875 / 33,875 · decompile → AST: ok 33,875 (100.0%) · err 0 · panic 0 · no shape-mismatch block · exit 0
```

FO76 has one more file than on 10-21 (7,233 → 7,234), from the 2026-10-24 game patch. That file parses and decompiles.

**Dim 1/2 guard spot-check** (no code changes). Every named guard is present and passes:
- `every_prefix_of_every_wire_valid_sample_is_rejected`
- `hostile_vararg_count_errors_instead_of_ooming`
- `string_copies_past_the_budget_are_refused`
- `call_site_copies_are_bounded_by_the_string_budget`
- `discriminants_match_on_disk_order`
- `from_u8_round_trips_and_rejects_oob`
- `metadata_matches_champollion{,_full_table}`
- `forward_jmpf_builds_an_if_diamond`
- `backward_jmpt_builds_a_loop_edge`
- `rebuild_expression_is_linear_up_to_the_wire_format_ceiling`
- `a_temp_chain_past_the_depth_cap_is_declined_not_aborted`

`OpCode::from_u8` still checks `byte >= MAX_OPCODE` (`opcode.rs:137`) before the `transmute` at `:142`. `const _: () = assert!(OpCode::TryLockGuards as u8 == MAX_OPCODE - 1)` is still at `:74`.

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 0 | 2 (#4113, #5445) |

All three findings from the 2026-10-21 report are fixed: #6262, #6263 and #6264. So is the long-open #4115. Nothing new was found.

### Untrusted-input robustness verdict: MET

- I found no way for a hostile `.pex` or `.psc` to cause a panic, out-of-bounds read, OOM or stack overflow.
- The reader, CFG, lift and boolean passes have had no code changes since the 10-21 run. That run's 3M-input `.psc` fuzz and hostile-shape timings still apply.
- The only new code that reads untrusted input is `expect_member_name`. It indexes only through `tokens.get(self.pos)`, then falls through to `expect_ident`, which already checks bounds.
- I did not rerun the fuzzer. That would have needed a private cargo target dir, which the suite rules forbid, and the new helper is too small to justify it.

### Decompile-rate verdict: re-measured at 33,875 / 33,875. It proves robustness, not fidelity

- **The gate holds.** It exits non-zero on any decompile `Err`, panic or shape mismatch (`pex_corpus_smoke.rs:288-311`, unchanged), and this run exited 0.
- **#6262 changes output only for names the corpus never uses.** The two `isTempVar` copies disagreed on 0 corpus locals (10-21 census), and the 100% rate is unchanged.
- **Fidelity is still not measured here.** It remains the `.psc`-vs-`.pex` gate owned by `/audit-scripting` Dim 1.

## Decompiler Soundness Matrix

| Stage | Bounds-safe | Terminates | Total (no panic) | Fidelity-tested |
|---|---|---|---|---|
| Reader (`reader.rs`, `opcode.rs`, `call_sites.rs`) | YES: single `take(n)` gate, string budget, guarded `transmute` | YES, linear | YES | Exhaustive-prefix test over five builders |
| CFG (`cfg.rs`) | YES: `checked_target` | YES | YES | `jmpf`/`jmpt` pins |
| Lift + copy-propagation (`lift.rs`, `node.rs`) | YES: memoised depth vs `MAX_EXPR_DEPTH` | YES, linear (#2024 pin) | YES | Depth-cap pins; corpus |
| Boolean (`boolean.rs`) | YES: `MAX_REBUILD_DEPTH` | YES, linear (#5877, #5879) | YES | Private-edge pins; in-edge recount; 10-21 A/B |
| Control flow (`control_flow.rs`) | YES: `RecursionLimit` | YES | YES; the leftover `||` shape fails closed, and the doc now says so (#4115) | `conditional_predecessor_fails_closed` |
| Lower (`lower.rs`) | YES: `flag_index < 32` | YES, linear | YES. The `lower_binary_op` `_ => Eq` arm is unreachable: lift and boolean emit only `+ - * / % == < <= > >= is && \|\|` | **One `isTempVar` (#6262)**; `Is`, `Auto Const` and local-declaration pins |
| `.psc` lexer and parser | YES: two 256 depth caps; `Parent` admitted only through `expect_member_name` | YES | YES | Newline-terminator pins; `parent_is_not_a_local_or_parameter_name`; FO4 corpus gate |

**Champollion departures:**
1. `boolean.rs` has no debug-line guard. Benign, unchanged from 10-21.
2. `boolean.rs` termination guard. Benign, unchanged.
3. `is` lowered to `Cast` (#4477). Closed by #5443.
4. The unbookkept second `isTempVar` from 10-21 is gone (#6262). `is_final` (`node.rs:382`) still differs from `is_temp_var`, but that asymmetry is Champollion's own and is ported on purpose.

## Findings

No new findings.

### Existing (open) issues, re-confirmed against HEAD

- **#4113**: two independent depth caps still feed the same recursive tree walks:
  - `MAX_EXPR_DEPTH` (256). `lift.rs:380` aliases `crates/papyrus/src/parser/expr.rs:24`.
  - `MAX_REBUILD_DEPTH` (1024). Defined at `control_flow.rs:45`; `boolean.rs:61` derives it.

  Unchanged.
- **#5445**: the type after `as`/`is` can still come from the next line. `parse_cast`/`parse_is` (`expr.rs:376`/`:393`) call `parse_type`, and `parse_base_type` starts with `skip_newlines()` (`parser/mod.rs:497`). Unchanged.

### Checked and not reported

- **`Parent` member name spelling.** In `.psc` the member name becomes `"Parent"`, while the `.pex` string table keeps `"parent"`. The VM's struct-member lookup is case-insensitive (`crates/scripting/src/fragment/vm/value.rs:102`), so both bind.
- **Remaining `keyword_as_ident` entries** (`Auto`, `Hidden`, `Mandatory`, `Conditional`, `Native`, `Const`, `Global`). `parse_prefix` has no arm for any of them. A local with one of these names gives a parse error when used, not a silently wrong tree, so it is not the #6263 class.
- **`papyrus-parser.md:279-281`.** It lists the keyword-as-identifier set without `Parent`. That is accurate, and the struct-member and `.member` exception is documented at `expect_member_name`. Not doc rot.
- **Out-of-scope `::` checks.** `crates/scripting/src/translate/effects/{guards.rs:106,lower.rs:666}` test `::` prefixes and `_var` suffixes. Those are scripting-side backing-variable checks, not a second `isTempVar` in the frontends.

## Cross-audit notes

- None of the findings already appended to the suite rules (§ 6) touch `crates/pex` or `crates/papyrus`, so nothing here duplicates them.
- `/audit-scripting` (running concurrently) owns `translate_pex`'s clean-`None` contract, the fidelity gate and the int/float operator-typing erasure (SCR-2026-10-21-02).

Next step: nothing to publish. The two open issues are already filed.
