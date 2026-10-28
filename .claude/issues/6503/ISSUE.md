# RT-8: `capture.sh` teardown orphans one Xvfb server per capture

Labels: low, tech-debt, bug
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-8 · **Severity**: LOW (harness hygiene; recurring since the 2026-10-08 suite) · **Dimension**: Harness integrity
**Location**: `.claude/commands/audit-runtime/capture.sh:425-435` (`teardown`: engine TERM, `sleep 2`, KILL, then immediately `kill -TERM "${wrapper_pid}"`)

## Description
`/usr/bin/xvfb-run` is `/bin/sh` (dash), and its only cleanup is `trap clean_up EXIT`. A TERM that reaches the wrapper while it is reaping the engine aborts the cleanup, which prints `xvfb-run: error: problem while cleaning up temporary directory` and exits 5 before `kill "$XVFBPID"`. The Xvfb server then reparents to `systemd --user`. This run orphaned 4 of 5 captures (`Xvfb :99`-`:102`, killed by PID). A minimal reproduction (`xvfb-run -a sleep 37`, TERM the child, TERM the wrapper at once) leaves Xvfb alive; waiting 1 s before the wrapper TERM leaves none.

## Evidence
Every engine log ends `Terminated` / `xvfb-run: error: problem while cleaning up temporary directory`. Code at HEAD matches the description (`kill -TERM "${engine_pid}"; sleep 2; kill -KILL; kill -TERM "${wrapper_pid}"`). Previously noted in AUDIT_SUITE_SUMMARY_2026-10-08 / -10-12 (7, then 21 orphans killed by hand), never filed.

## Impact
Every runtime audit leaks X servers (RAM and display numbers) until someone sweeps them. The harness's survivor sweep checks only `byroredux`, so it never notices.

## Related
`capture.sh --self-test`; AUDIT_SUITE_SUMMARY_2026-10-08 / -10-12.

## Suggested Fix
After killing the engine, `wait` for the wrapper with a bounded timeout and TERM it only if it is still alive. Extend `sweep_survivors` to the `Xvfb` whose `-auth` names this run's `xvfb-run.*` dir, and add the case to `--self-test`.

## Completeness Checks
- [ ] **SIBLING**: Other smoke scripts under `docs/smoke-tests/` using `xvfb-run` teardown checked for the same pattern
- [ ] **TESTS**: `capture.sh --self-test` covers the orphan-Xvfb case

