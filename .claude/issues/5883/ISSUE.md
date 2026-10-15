# #5883: PEX-D3-2026-10-15-06: `pex_corpus_smoke` exits 0 on decompile `Err`, panic and shape mismatch, so the "100% decompile, zero shape mismatches" claim is not gated

- **Labels**: low,bug,scripting,test-gap
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: `pex_corpus_smoke` exits 0 on decompile `Err`, panic and shape mismatch, so the "100% decompile, zero shape mismatches" claim is not gated. `pex_dump_fn` cannot dump a property getter or setter

- **Severity**: LOW
- **Dimension**: Decompiler Boolean/Control-Flow/Lower (instruments)
- **Untrusted-Input**: No
- **Location**: `crates/pex/examples/pex_corpus_smoke.rs:298-302` (the exit gate counts only `stats.failed`, the parse failures); `crates/pex/examples/pex_dump_fn.rs:85-87` (iterates `object.states` only)
- **Status**: NEW
- **Description**: The harness tallies `decompiled_err` and `decompiled_panic` correctly, and the percentage is computed over all three outcomes. However, the doc comment's "Non-zero exit … so this is usable as a gate" covers parse failures only. A decompile-rate or shape regression still exits 0 and is visible only by reading stdout. Separately, `pex_dump_fn` walks `state.functions` only. A smoke failure inside a property `Get`/`Set` body (bodies that `decompile_script` does decompile, via `build_named_function`) cannot be disassembled with it.
- **Impact**: The tooling can silently miss a decompile regression if anyone uses the exit status as a gate. Neither defect affects the engine.
- **Suggested Fix**: Exit non-zero when `decompiled_err + decompiled_panic + decompiled_shape_mismatch > 0`. Have `pex_dump_fn` also iterate `properties[*].read_function/write_function`, named `<prop>.Get` and `<prop>.Set`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
