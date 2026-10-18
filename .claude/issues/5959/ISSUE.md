# SKY-D4-2026-10-17-02: Whiterun BanneredMare is 7-13% slower than the bench-of-record's own commit (same machine, same harness)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5959 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-4-02) · **Severity**: MEDIUM · **Dimension**: 4 (control bench) · **Game**: Skyrim SE

## Description
Whiterun BanneredMare is 7–13% slower at HEAD than the bench-of-record's own commit, measured on the same machine in the same session. Per the standing methodology the outgoing record's commit `a3a7a43b0` was rebuilt in a worktree (`--release`) and run through `scripts/fsr-bench-matrix.sh` (harness `a4ede5fa0`, `renderer-stepped/orbit`, 3 runs x 300 frames, `FSR_BENCH_SCENES=whiterun`), then HEAD `eec7e00cf` was run the same way immediately after. Camera pose is identical to the record and to each other (`177.223,188.000,1048.424`).

## Evidence (medians)
| Config | Control `a3a7a43b0` | HEAD `eec7e00cf` | Delta |
|---|---:|---:|---:|
| TAA native | 83.8 FPS / 11.94 ms | 74.3 FPS / 13.46 ms | +1.52 ms (+12.7%) |
| FSR native-AA | 79.9 / 12.52 | 72.6 / 13.77 | +1.25 ms |
| FSR Quality | 131.2 / 7.62 | 120.4 / 8.30 | +0.68 ms (+8.9%) |
| FSR Balanced | 157.7 / 6.34 | 143.8 / 6.95 | +0.61 ms |
| FSR Performance | 189.7 / 5.27 | 178.2 / 5.61 | +0.34 ms (+6.5%) |

Per pass (TAA): `gpu_main` 11.161 -> 12.546 ms (+1.385), `gpu_svgf` 0.506 -> 0.856 (+0.350, +69%), `gpu_volumetrics` 0.818 -> 0.902, `gpu_volumetrics_inject` 0.790 -> 0.877. At Quality: `gpu_main` +0.664, `gpu_svgf` 0.265 -> 0.443. Entities 5,825 -> 5,837; draws (`1289/392b/24c`), TLAS (1,226) and lights (18) are equal. Run spread is 0.01–0.35 ms, well below the delta. An earlier HEAD-only pass the same hour agreed (TAA 76.3 FPS / 13.10 ms, Quality 122.0 / 8.20). Run conditions: RTX 4070 Ti, desktop apps resident, 1-min load average 3.5 at the start of the control bench and 2.2 at the end of the HEAD bench (HEAD was not disadvantaged by ordering).

## Impact
About 1.5 ms of a 12 ms frame at TAA native on the project's Skyrim control scene. The ROADMAP record (92.4 FPS TAA) is no longer reproducible; today's control build itself reads 83.8, so the machine is also ~9% slower than at record time. Only the same-session A/B is meaningful. No correctness impact observed.

## Candidate commits (NOT bisected)
Renderer range `a3a7a43b0..eec7e00cf`: `3f8194117` (SVGF progressive accumulation no longer waits for every NPC), `1092b5b9e` / `24eb7dba8` (SVGF history age / invalidation), `c2647d468`, `17e0b7464`, `6110b6078`, `2e7d5bead` (ReSTIR reservoir ownership / temporal reuse), `triangle.frag` +253 lines (`triangle.frag.spv` 432,576 -> 444,880 B), and a new `shadow_filter.comp` pass whose GPU time is not among the harness columns (unmeasured).

## Suggested fix
Bisect `a3a7a43b0..eec7e00cf` on this exact harness (the control-worktree build takes ~2 minutes); decide per commit whether the cost is the intended price of the feature; refresh the bench-of-record once the number is understood.

Related: ROADMAP bench-of-record; `/audit-performance`, `/audit-renderer`. Closed #2367 / #5117 are earlier, unrelated regressions.

## Completeness Checks
- [ ] **SIBLING**: Re-run the other four bench-of-record scenes (Cornell, Prospector, MedTek, Dugout) on the same control to see whether the cost is Skyrim-specific
- [ ] **TESTS**: Add the per-pass numbers to the refreshed bench TSV so the next audit can compare passes, not just wall time
