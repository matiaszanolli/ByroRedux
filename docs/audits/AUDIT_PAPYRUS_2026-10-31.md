**HEAD**: 670f81917 · **Baseline**: [AUDIT_PAPYRUS_2026-10-27.md](AUDIT_PAPYRUS_2026-10-27.md) (HEAD `fa6be4a6b`) · **Audited**: `crates/pex/examples/pex_list_fns.rs` (new, `4e6132a73`) · **Unchanged since baseline (skimmed)**: Dim 1 (PEX Reader & Opcode Decode), Dim 2 (Decompiler CFG & Lift), Dim 3 (Boolean/Control-Flow/Lower), Dim 4 (`.psc` Lexer & Pratt Parser)

# Papyrus Frontends Audit: 2026-10-31

Run inside `/audit-suite --preset comprehensive`. No sub-agents, no engine launch, built only in the repo `target/`, no private target dir.

## Delta since the baseline: 1 commit

`git log fa6be4a6b..HEAD -- crates/pex crates/papyrus` returns one commit, and it touches no library code.

| Commit | Change | Dim | Verdict |
|---|---|---|---|
| `4e6132a73` (10-30) | m43: adds `crates/pex/examples/pex_list_fns.rs` (26 lines), a read-only lister of a `.pex`'s typed function signatures. The rest of the commit is outside these crates | instruments | Not a gate and not on any production path. It uses `parse` only, and `expect`/`args[1]` panics are acceptable in a developer example fed a local file. `cargo check -p byroredux-pex --examples` is clean. The skill scope already lists it |

No file under `crates/pex/src` or `crates/papyrus/src` changed. Dimensions 1 to 4 are therefore re-verified by running their guards, not by re-reading source.

## Build, test and corpus state: CLEAN

```
cargo test -p byroredux-pex -p byroredux-papyrus
  papyrus 146 unit + 4 round-trip · pex 86 unit + 1 doc-test (r5_fidelity: 1 ignored, needs data) · 0 failed
pex_corpus_smoke (release): Skyrim SE Misc.bsa, Fallout4 Misc.ba2, Starfield Misc.ba2, SeventySix MiscClient.ba2
  decompile -> AST: ok 33,875 (100.0%) · err 0 · panic 0 · no shape-mismatch block
```

The corpus is the same size as on 10-27 (33,875 files). The guard names listed in the 10-27 report all ran inside the green unit runs. The `MAX_EXPR_DEPTH` (256, `expr.rs:24`) and `MAX_REBUILD_DEPTH` (1024, `control_flow.rs:45`) constants are unchanged, and `opcode.rs` is unchanged, so the `transmute` bound check and its compile-time assert still stand.

## Executive Summary

| Severity | NEW | Existing (open) re-confirmed |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 0 |
| LOW | 0 | 2 (#4113, #5445) |

### Untrusted-input robustness verdict: MET

No code on any untrusted-input path changed since 10-27, so the 10-27 analysis stands. I found no way for a hostile `.pex` or `.psc` to panic, read out of bounds, OOM or stack-overflow the cell loader or console. I did not rerun the fuzzer, because nothing it exercises changed.

### Decompile-rate verdict: re-measured at 33,875 / 33,875. It proves robustness, not fidelity

The smoke gate still fails on any decompile `Err`, panic or shape mismatch, and this run reported 0 of each. The rate does not measure fidelity. That evidence is the `.psc`-vs-`.pex` gate owned by `/audit-scripting` Dim 1.

## Decompiler Soundness Matrix

Unchanged from [10-27](AUDIT_PAPYRUS_2026-10-27.md#decompiler-soundness-matrix); every cell re-confirmed by the green guards above. All seven stages are bounds-safe, terminating and total. The three Champollion departures stay adjudicated: the two in `boolean.rs` are benign, and `is` as `Cast` is closed by #5443.

## Findings

No new findings.

### Existing (open) issues, re-confirmed against HEAD

- **#4113**: `MAX_EXPR_DEPTH` and `MAX_REBUILD_DEPTH` are still independent caps feeding the same recursive walks. Unchanged.
- **#5445**: the type after `as`/`is` can still come from the next line, via `parse_base_type`'s `skip_newlines()` (`crates/papyrus/src/parser/mod.rs:497`). Unchanged.

## Cross-audit notes

- None of the findings appended to the suite rules (§ 6) touch `crates/pex` or `crates/papyrus`.
- `4e6132a73` also reads `Quest.pex` from the game (FO4 event payloads). That is the scripting consumer's concern (`/audit-scripting`), not a frontend change.

Next step: nothing to publish. Both open issues are already filed.
