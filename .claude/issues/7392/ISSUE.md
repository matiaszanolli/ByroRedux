# #7392 — REN-D12-2026-11-23-03: the `gpu_timers.rs` module-header narrative stops at 68/34 and the prose-count guard's retired-spelling list stops at 64/32

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D12-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (doc rot; judged not to matter functionally)
- **Dimension**: Debug/Telemetry
- **Location**: module header of `crates/renderer/src/vulkan/gpu_timers.rs`; test `prose_outside_the_module_header_carries_no_bracket_counts`
- **Status**: NEW
- **Description**: The bump list ends "and from 68/34 by the TSD-4 terrain patch pass (#7152)", which reads as 70/35; it omits the #7221 patch BLAS bump to 72/36, so `QUERIES_PER_FRAME = 72` is never named in the narrative. The `rotted` list in the prose guard last retired 64 / 32 (`64-query`, `full 64`, `currently 32`); the 68, 70 and 34/35 spellings were not added (the file carries none of them today, so nothing is rotting yet). The slot table itself is complete (rows 0..=71) and is what `doc_table_slot_count_matches_queries_per_frame` guards; `QUERIES_PER_FRAME` is the single source.
- **Impact**: None on behaviour; a reader taking the narrative as the count gets 35 brackets. The table and the compile-time `const _` asserts are authoritative, so I judge this not worth more than a one-line edit on the next bump.
- **Related**: #4210 / #4981 history of the same rot.
- **Suggested Fix**: append "and from 70/35 by the patch BLAS bracket (#7221)" and add `68-query`/`full 68`/`currently 34`/`70-query`/`full 70`/`currently 35` to the retired list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
