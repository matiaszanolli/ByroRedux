# TD9-2026-10-19-04: `skeever_ragdoll_survives_six_hundred_floor_ticks` is an isolation probe in test clothing that cannot run on any machine as shipped

Labels: low,physics,test-gap,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `byroredux/src/ragdoll_skeever_probe_tests.rs:88-100` (`#[ignore = "real-data isolation probe (needs BYRO_SKEEVER_NIF)"]`, default `/tmp/byro-5161-probe3/skeever_skeleton.nif`), `:132-` (`BYRO_PROBE_NOFLOOR`, `BYRO_PROBE_PENETRATION`, `println!` instrumentation)
- **Age**: `441d90c2f` (2026-10-10), guards closed #5161 (labelled high-class multi-SAP/bone-pose fix)
- **Effort**: small
- **Description**: the input file lives in `/tmp` and is not in the repo or an archive (`ls /tmp/byro-5161-probe3` -> missing), so the test `panic!`s with "set BYRO_SKEEVER_NIF" on every machine that has not hand-extracted it, three env knobs steer the scenario, and it is in no pin list. It is the same shape the `no_committed_example_self_describes_as_disposable` guard exists to stop, but as a `#[test]` where the guard cannot see it.
- **Evidence**: `let Ok(nif_bytes) = std::fs::read(&path) else { panic!("set BYRO_SKEEVER_NIF to the extracted skeever skeleton (missing: {path})") };`
- **Suggested Fix**: read `skeleton.nif` of the skeever from the Skyrim archive through the shared archive helper (like `race_skeletons_match_authored_paths_on_real_skyrim_data`), drop the probe env knobs/`println!`, add it to the pins list; if #5161 already has a unit-level regression, delete this probe.

Related: #5161, #5746 (closed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
