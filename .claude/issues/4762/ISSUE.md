# TOOL-D6-2026-09-22-02: run_manifest aborts the whole texture-upscale batch on the first per-set failure

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4762

## Description
`run_manifest` (`tools/texture-upscale/src/pipeline.rs:45-`) propagates `sources.extract(...)?` and `decode_texture(...)?` via `?` from inside the per-set loop (confirmed at lines ~90-92), which returns `Result<RunReport>` — any error on any set aborts the entire function immediately, discarding the already-accumulated `report.sets` entries for prior successfully-processed sets along with it. `decode_texture`'s BC5/BC7 error message is clear and well-worded, but one unsupported texture anywhere in a large manifest aborts the run before any other, perfectly-decodable set is processed or reported on — even though `RunReport { sets: Vec<...> }`'s shape implies per-set results were the intended design.

## Evidence
```rust
// pipeline.rs:90-92 (inside the per-set loop, run_manifest -> Result<RunReport>)
let reference_bytes = sources.extract(&set.reference)
    .with_context(|| format!("load reference for set {:?}", set.name))?;
let reference_low = decode_texture(&reference_bytes, &set.reference)?;
```

## Impact
A large batch job (the tool's core use case) loses all progress on the first bad entry rather than skipping and summarizing it. LOW — full clear error, non-zero exit, no data corruption, and the failure mode itself is untested (no test exercises a manifest with one bad set alongside good ones to confirm the all-or-nothing behavior is intentional vs. accidental).

## Related
None found.

## Suggested Fix
Collect a `Result` per set, push success/failure into `report.sets`, continue to the next set, and summarize failures at the end. Also worth a one-line README note that `[upscaler]` is trusted executable input (`Command::new`/`args`, confirmed no shell — not itself a vulnerability, just currently undocumented).

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D6-2026-09-22-02)

## Completeness Checks
- [ ] **TESTS**: A regression test runs a manifest with one bad set alongside good ones and asserts the good sets still complete and appear in `RunReport`, with the bad set recorded as a per-set failure
