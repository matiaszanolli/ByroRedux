# EXT-D7-2026-10-31-01: `m-exteriors.sh --self-test` (the #6458 exemption decision) is run by no CI lane or contract checker

Labels: low,terrain-exterior,bug,test-gap,game:fo4

Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-31.md` (finding `EXT-D7-2026-10-31-01`).

- **Severity**: LOW (test-gap)
- **Dimension**: 7 Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: FO4
- **Location**: `docs/smoke-tests/m-exteriors.sh:48-95` (`fo4_known_seam_crack`, `--self-test`); `scripts/check-playable-smoke-contracts.sh:255` (runs only `m47-triggers.sh --self-test`); `.github/workflows/ci.yml`
- **Status**: NEW. This is a gap left by `37a7e9ff9` (#6458).
- **Description**: #6458 turned the FO4 seam exemption into a pure shell function and added a `--self-test` for it, so the decision can be proven without Vulkan or game data. `m47-triggers.sh` does the same and its self-test is run by `check-playable-smoke-contracts.sh:255`. No lane runs `m-exteriors.sh --self-test`, so the self-test only runs when a developer thinks to call it. The exemption is the only thing between a new 3-vertex FO4 crack and a green boundary gate. I ran the self-test by hand at HEAD and it passes.
- **Evidence**: `grep -rn "self-test" scripts .github docs/smoke-tests/m-exteriors.sh` lists `m47-triggers`, `check-issue-traceability`, `check-bench-harness-provenance`, `package-release` and `fsr_bench_report` as gated, and `m-exteriors.sh` only at its own definition.
- **Impact**: Edits to `fo4_known_seam_crack` or to the `first_dirty_pair=` parse can break the exemption (in either direction) with no CI signal until an FO4 boundary run on a machine with game data.
- **Suggested Fix**: Add `bash "$ROOT/docs/smoke-tests/m-exteriors.sh" --self-test >/dev/null || fail "m-exteriors.sh --self-test failed"` beside the m47 line in `check-playable-smoke-contracts.sh`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
