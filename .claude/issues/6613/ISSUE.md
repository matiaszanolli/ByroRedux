# RT-2026-10-31-06: `p6-loading-model skyrim_se` can never pass: its `RUST_LOG` omits `byroredux::app_step`, and its "artifacts" path is deleted on exit

Labels: medium,tech-debt,test-gap,bug,game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-06 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `p6-loading-model.sh:93` `RUST_LOG` still omits `byroredux::app_step`; the `Cell transition applied:` lines are now at `app_step.rs:1446` and `:1663`.

- **Severity**: MEDIUM (a declared Skyrim gate is red by construction)
- **Dimension**: Harness integrity
- **Location**: `docs/smoke-tests/p6-loading-model.sh:93` (`RUST_LOG`), `:177` (the wait), `:38-47` and `:51-52` (`fail()` / `cleanup`)
- **Status**: NEW
- **Game / Cell**: skyrim_se
- **Baseline**: PASS expected
- **Current**: Gates 1 to 6 PASS (census, cover pick, art resolves, cover began from a real LSCR, presented before teardown, cover frame 89,345 B). Then `FAIL -- skyrim_se: timeout waiting for the authored transition` after 300 s. A kept re-run shows the transition did apply (`Transition: … → exterior 'whiterunworld' (6,-2)`, `loading.screen: dismissed after destination frame` 5 s later).
- **Description**: The wait greps the engine log for `Cell transition applied:` containing `P0_APPLIED_LOG`. That line is `log::info!` in `byroredux/src/app_step.rs:1352`, target `byroredux::app_step`, but the gate sets `RUST_LOG="warn,byroredux::loading_screen=info,byroredux::cell_loader::transition=info,byroredux::interaction=info"`, so it is filtered out (p0 and p1 include `app_step=info`). Separately, `fail()` prints `artifacts: $LOG_DIR` and the `EXIT` trap then `rm -rf`s it, so the path in the failure message never exists; two runs left nothing to inspect.
- **Evidence**: `/tmp/audit/runtime/p6_keep.log` and the kept engine log.
- **Suggested Fix**: Add `byroredux::app_step=info` to the filter; honour `BYROREDUX_KEEP_ARTIFACTS` in the trap as `p3-player-body` does.

## Completeness Checks
- [ ] **SIBLING**: The same harness defect checked in the other gates of its family
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
