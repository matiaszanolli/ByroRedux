# TD1-2026-10-25-04: `crates/renderer/src/vulkan/gpu_timers.rs` spells 25 timer brackets as 50 near-identical methods (~850 of its 1929 prod lines); each new pass costs ~45 lines in four places

Labels: low, renderer, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6358
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 (also Dim 2) · **Effort**: small-medium
- **Age**: 1 commit in window (`994620123`, #6316 cloud-shadow bracket, +~45 lines); file 1874 -> 1929. 640 of its lines are comments.
- **Location**: `gpu_timers.rs:1009-~1900` (`cmd_<pass>_start` / `cmd_<pass>_end`, 50 methods in `impl GpuPerFrameTimers` L683-1930), consts `Q_*_START/END` L140-213, `BIT_*` L450-477
- **What**: every `_start` is `cmd_write_timestamp(cmd, <STAGE>, self.pools[frame], Q_X_START)`; every `_end` the same plus `self.active_bits[frame] |= BIT_X`. The only variation is the stage flag: 18 `TOP_OF_PIPE`, 20 `COMPUTE_SHADER`, 12 `BOTTOM_OF_PIPE`, 2 `ACCELERATION_STRUCTURE_BUILD_KHR` (52 literals across the impl; a few methods carry extra logic, e.g. geometry-phase brackets, so check each row before folding). 73 call sites in the renderer. #6337 (closed) already made the bench-line side of these brackets a keyed table; the emit side is still hand-written.
- **Fix**: data-table shape, not a file split (skill: tables want tables). `enum TimerPass` with a `const` row `{start_q, end_q, bit, start_stage, end_stage}` and two methods `cmd_start(pass,..)` / `cmd_end(pass,..)`; keep the `Q_*`/`BIT_*` consts as they are (the pin tests `every_timer_slot_and_active_bit_is_unique_and_in_range` and `doc_table_slot_count_matches_queries_per_frame` parse `const Q_*: u32 = N;` lines and the `//! |` doc table) and migrate the 73 call sites mechanically. Timestamp writes only, no barrier/layout change, but stage flags must be preserved verbatim per row; verify with `BYRO_VALIDATION=1` (feedback_speculative_vulkan_fixes). Expected ~ -600 lines.
- **Hazard**: four `include_str!("gpu_timers.rs")` self-scans (L2244, 2326, 2372, 2413) and `renderer/src/source_scan.rs:64` allowlist row; do not move the file.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
