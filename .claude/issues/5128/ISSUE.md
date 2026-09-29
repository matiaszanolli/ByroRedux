# #5128: RT-2026-09-29-04: R6a-regress-22 ("FO4 frame time doubled, batching halved at flat draw counts") reproduces as a pure bench-camera move at `b9e961eeb`

**Labels**: medium, bug, performance, game:fo4

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-4`)
**Severity**: MEDIUM (the bench-of-record and its open regression rest on a false "content unchanged" premise; the prescribed bisect targets the wrong kind of change)
**Dimension**: Benchmark integrity / attribution

## Location
- `ROADMAP.md` "Active focus" bullet "Bisect **R6a-regress-22**" and the R6a-regress-22 section's "Entity, light and TLAS counts match … so the content is unchanged" inference
- Spawn ladder: `byroredux/src/cell_loader/interior_spawn.rs`

## Description
The ROADMAP rules out content change because entity, light and TLAS counts match. The camera origin, though, is not content. The bench-of-record's stepped camera starts from the spawn pose, and `b9e961eeb` (2026-09-23) moved that pose into the room for every COC-marked interior. It lies inside R6a-regress-22's window `4c9a5b36..99933f87b`. Measured on the R6a scene itself, FO4 `DmndDugoutInn01`, renderer-static:
- `cb44d99f6`: `1866/126b/7c` r163, p50 8.60 ms, gpu_main 8.82 ms.
- `b9e961eeb`: `2379/651b/41c` r1785, p50 18.20 ms, gpu_main 15.71 ms.
- Entities 11603, lights 156 and TLAS 1818 are identical on both.

The ROADMAP's own Dugout figures (`1935/326b/12c` → `2367/659b/43c`, fence 4.13 → 18.95 ms) have the same shape. FO4 InstituteBioScience shows the same: p50 9.97 → 41.58 ms at `b9e961eeb`, nothing else changed. For FNV-D6-01 (Prospector −32 entities / −24 draws / −35 TLAS in `cb44d99f6..a37fcba3c`), the only entity-changing commit in that window on the AtomicWrangler bisect is `5570c221c` (dismemberment caps: −92 entities, −92 draws); `a070baaad` and `f87490826` land after `a37fcba3c`. That FNV attribution is inferred, not measured on Prospector.

## Evidence
`/tmp/audit/runtime/bisect-fo4-DmndDugoutInn01-{cb44d99f6,b9e961eeb}/`; bisect table in the report.

## Impact
Time spent bisecting renderer code for a "regression" is misdirected. Before/after rows in the bench-of-record compare different views, so "FSR recovery" and per-scene deltas across `b9e961eeb` are not apples to apples. Some real cost may still hide under the camera move (e.g. the `186234944` reservoir-clear barrier and early-test split named by PERF); that can only be read once both sides use the same pose.

## Related
RT-3, FNV-2026-09-29-D6-01 (#5067), PERF 2026-09-29 (R6a static attribution), `186234944`.

## Suggested Fix
Re-run the `4c9a5b36` vs `99933f87b` control with the camera pinned to one pose on both sides (`scripts/fsr-bench-matrix.sh` would need a pose override or a pre-`b9e961eeb` spawn — not verified to exist). Then re-state R6a-regress-22 in ROADMAP.md as whatever residual survives. Record the spawn pose in the bench TSV header so a future pose change is visible.

Validated at HEAD 9fcfdc3fc: ROADMAP.md still lists "Bisect R6a-regress-22" as active focus and carries the "content is unchanged" inference; `interior_spawn.rs` (b9e961eeb) is in the window.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (pose recorded in the bench TSV header)
