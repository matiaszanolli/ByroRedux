# FO4-2026-11-10-D2-02: BGSM/BGEM Additive and Multiplicative blend forwarding is not pinned through the production merge arms

Labels: low,bug,test-gap,import-pipeline,game:fo4,legacy-compat
Source: docs/audits/AUDIT_FO4_2026-11-10.md

- **Severity**: LOW
- **Dimension**: 2 (guard liveness / test gap)
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs:591-700` (guard) vs `merge.rs:1184-1190` (BGSM arm) and `merge.rs:1755-1759` (BGEM arm)
- **Status**: NEW (not in `/tmp/audit/issues.json`; closed-issue search for BGSM blend returns only #1823's history)
- **Description**: the brief's named guard `bgsm_merge_forwards_alpha_blend_mode` is a hand-copied mirror: its local `fn apply(..)` re-states the three writes and calls only `bgsm_blend_to_gamebryo`, so it never executes `merge_bgsm_arm` / `merge_bgem_arm`. `bgsm_blend_to_gamebryo_is_identity_narrowing` pins the helper, not the arms. The only tests that drive the real arm with a blend triple use Standard (6,7), which is the fixed point of the 0<->1 swap that #1823 corrected (`precombined.rs::merge_precombine_materials_restores_blend_*`, which restores the blend anyway). `grep` for `dst_blend: 0` / `dst_blend: 1` in `byroredux/src` finds only the mirror test and one glass-classifier fixture. This is the failure mode #2702 documented ("hand-copied mirror diverges silently") applied to the exact regression class the checklist bullet names.
- **Evidence**: a change in either arm that bypasses the helper (an inline swap, a reintroduced `gl_to_gamebryo_blend`, or the BGEM arm dropping its block) passes the whole suite. Vanilla content that such a change would corrupt: 81 BGEMs Additive (6,0) and 8 Multiplicative (4,1) files (4 BGSM + 4 BGEM). Child-first `set_blend` ("a function-0 leaf must not clear a parent's blend") has no production-path test either. The new `cdb_blend_mode_tests` (#5509) show the right shape: it calls the real `apply_cdb_material`.
- **Impact**: the code is correct today (arms read at HEAD, real-data histogram above); this is a guard that would not fire. LOW per the severity file (test coverage gap, code works).
- **Related**: #1823, #2702.
- **Suggested Fix**: add two production-path tests (`insert_bgsm_for_test` / `insert_bgem_for_test` + `merge_external_material`) asserting `(has_alpha, src, dst)` for (1,6,0) and (1,4,1), plus the leaf-0/parent-1 chain case; keep or delete the mirror.
